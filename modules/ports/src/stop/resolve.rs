use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use navori_macos::{ProcessSource, ReadError, Signals};

use super::reason;
use crate::action::{Outcome, Selector, Target};
use crate::actor::with_docker;
use crate::image::database_image;
use crate::infra::docker::Containers;
use crate::kind::is_docker_backend;
use crate::scan::{HarnessMemo, Scan, Scanner};
use crate::state::{DockerStatus, PortEntry, PortKind};

const MAX_ANCESTORS: usize = 64;

/// What a fresh scan and a fresh Docker read found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FreshRead {
    pub scan: Scan,
    pub entries: Vec<PortEntry>,
    pub docker: DockerStatus,
}

impl FreshRead {
    /// Reads the listeners and the containers again, the same two steps a scheduled scan takes.
    ///
    /// # Errors
    ///
    /// The fixed sentence for a process list that could not be read.
    pub async fn take(
        source: Arc<dyn ProcessSource>,
        home: PathBuf,
        containers: Arc<dyn Containers>,
    ) -> Result<Self, &'static str> {
        let scanner = Scanner::new(source, home.clone());
        let scan = tokio::task::spawn_blocking(move || scanner.scan(HarnessMemo::new()).0)
            .await
            .map_err(|_| reason::PROCESS_LIST_TIMEOUT)?
            .map_err(|_| reason::PROCESS_LIST_TIMEOUT)?;
        let (mut scan, docker) = with_docker(containers, home, scan).await;
        let entries = std::mem::take(&mut scan.entries);
        Ok(Self {
            scan,
            entries,
            docker,
        })
    }
}

/// How far a stop may reach for one entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopClass {
    Free,
    GroupProtected,
    Never,
}

/// The one rule for what may be stopped, from a fresh entry alone.
pub fn stop_class(entry: &PortEntry, own_pid: i32, helper: Option<&Path>) -> StopClass {
    if entry.pid == own_pid || helper.is_some_and(|path| entry.exe.as_deref() == Some(path)) {
        return StopClass::Never;
    }
    if let Some(held) = &entry.container {
        let database = held.image.as_deref().is_some_and(database_image);
        return if database {
            StopClass::GroupProtected
        } else {
            StopClass::Free
        };
    }
    if is_docker_backend(entry.exe.as_deref()) || entry.kind == PortKind::Docker {
        return StopClass::Never;
    }
    match entry.kind {
        PortKind::Database | PortKind::System => StopClass::GroupProtected,
        _ => StopClass::Free,
    }
}

/// What a stop must never signal: Navori itself, what started it, and the helper.
pub struct Guard {
    own_pid: i32,
    pub(super) ancestors: BTreeSet<i32>,
    pub(super) complete: bool,
    source: Arc<dyn ProcessSource>,
    helper: Option<PathBuf>,
}

impl std::fmt::Debug for Guard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Guard")
            .field("own_pid", &self.own_pid)
            .finish_non_exhaustive()
    }
}

impl Guard {
    pub fn new(
        signals: Arc<dyn Signals>,
        source: Arc<dyn ProcessSource>,
        helper: Option<PathBuf>,
    ) -> Self {
        let own_pid = signals.own_pid();
        let mut ancestors = BTreeSet::new();
        let mut complete = false;
        let mut at = own_pid;
        for _ in 0..=MAX_ANCESTORS {
            let Ok(parent) = source.entry(at).map(|entry| entry.ppid) else {
                break;
            };
            if parent <= 0 {
                complete = true;
                break;
            }
            if !ancestors.insert(parent) {
                break;
            }
            at = parent;
        }
        Self {
            own_pid,
            ancestors,
            complete,
            source,
            helper,
        }
    }
}

/// A stop that has passed every check and is ready to carry out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Plan {
    Process(ProcessPlan),
    Container(ContainerPlan),
}

/// Whom a process stop signals.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reach {
    Group(Vec<(i32, u64)>),
    Alone,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessPlan {
    pub pid: i32,
    pub start_us: u64,
    pub reach: Reach,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContainerPlan {
    pub id: String,
}

/// The verdict for one target. `AlreadyGone` is refused without a sentence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolved {
    Go(Plan),
    Refused(Outcome, Option<&'static str>),
}

/// Judges each target against the fresh read, in request order. The selector only checks; it adds nothing.
pub fn resolve(
    fresh: &FreshRead,
    guard: &Guard,
    selector: &Selector,
    targets: &[Target],
) -> Vec<Resolved> {
    targets
        .iter()
        .map(|target| match target {
            Target::Process { pid, start_us } => {
                resolve_process(fresh, guard, selector, *pid, *start_us)
            }
            Target::Container { id } => resolve_container(fresh, guard, selector, id),
        })
        .collect()
}

fn resolve_process(
    fresh: &FreshRead,
    guard: &Guard,
    selector: &Selector,
    pid: i32,
    start_us: u64,
) -> Resolved {
    if !guard.complete {
        return Resolved::Refused(Outcome::Failed, Some(reason::GUARD_INCOMPLETE));
    }
    let current = match guard.source.entry(pid) {
        Ok(current) if !current.zombie => current,
        Ok(_) | Err(ReadError::Gone) => return Resolved::Refused(Outcome::AlreadyGone, None),
        Err(_) => {
            return Resolved::Refused(Outcome::Failed, Some(reason::PROCESS_LIST_TIMEOUT));
        }
    };
    if current.start.0 != start_us || current.ruid != guard.source.current_uid() {
        return Resolved::Refused(Outcome::Stale, Some(reason::PROCESS_CHANGED));
    }
    let held: Vec<&PortEntry> = fresh
        .entries
        .iter()
        .filter(|entry| entry.pid == pid && entry.start_us == start_us)
        .collect();
    let Some(first) = held.first() else {
        return Resolved::Refused(Outcome::Stale, Some(reason::PORT_CLOSED));
    };
    if guard.ancestors.contains(&pid) || held.iter().any(|entry| never(entry, guard)) {
        return Resolved::Refused(Outcome::Protected, Some(reason::PROCESS_PROTECTED));
    }
    let grouped_out = !matches!(selector, Selector::Pid(_))
        && held.iter().any(|entry| {
            stop_class(entry, guard.own_pid, guard.helper.as_deref()) != StopClass::Free
        });
    if grouped_out {
        return Resolved::Refused(Outcome::Protected, Some(reason::NOT_STOPPED_WITH_GROUP));
    }
    if !within(selector, first) {
        return Resolved::Refused(Outcome::OutsideSelector, Some(reason::NOT_IN_PROJECT));
    }
    Resolved::Go(Plan::Process(ProcessPlan {
        pid,
        start_us,
        reach: Reach::Alone,
    }))
}

pub(super) fn never(entry: &PortEntry, guard: &Guard) -> bool {
    entry.container.is_some()
        || stop_class(entry, guard.own_pid, guard.helper.as_deref()) == StopClass::Never
}

fn resolve_container(fresh: &FreshRead, guard: &Guard, selector: &Selector, id: &str) -> Resolved {
    if fresh.docker != DockerStatus::Read {
        return Resolved::Refused(Outcome::Failed, Some(reason::DOCKER_UNREACHABLE));
    }
    let held: Vec<&PortEntry> = fresh
        .entries
        .iter()
        .filter(|entry| {
            entry
                .container
                .as_ref()
                .is_some_and(|container| container.id == id)
        })
        .collect();
    let Some(first) = held.first() else {
        return Resolved::Refused(Outcome::Stale, Some(reason::CONTAINER_NOT_RUNNING));
    };
    let class = |entry: &&PortEntry| stop_class(entry, guard.own_pid, guard.helper.as_deref());
    if held.iter().any(|entry| class(entry) == StopClass::Never) {
        return Resolved::Refused(Outcome::Protected, Some(reason::PROCESS_PROTECTED));
    }
    if !matches!(selector, Selector::Pid(_))
        && held.iter().any(|entry| class(entry) != StopClass::Free)
    {
        return Resolved::Refused(Outcome::Protected, Some(reason::NOT_STOPPED_WITH_GROUP));
    }
    if !within(selector, first) {
        return Resolved::Refused(Outcome::OutsideSelector, Some(reason::NOT_IN_PROJECT));
    }
    Resolved::Go(Plan::Container(ContainerPlan { id: id.to_owned() }))
}

fn within(selector: &Selector, entry: &PortEntry) -> bool {
    match selector {
        Selector::Pid(_) => true,
        Selector::Worktree { project, worktree } => {
            entry.place.project.as_ref() == Some(project)
                && entry.place.worktree.as_ref() == Some(worktree)
        }
        Selector::Project(project) => &entry.place.project == project,
    }
}

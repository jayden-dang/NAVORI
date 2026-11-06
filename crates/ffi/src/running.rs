use std::collections::BTreeMap;
use std::path::PathBuf;

use navori_mod_ports::{
    Harness, PortEntry, PortKind, PortsState, Selector, StopClass, Target, stop_class,
};
use serde_json::json;

use crate::{AwakeView, LeaseView};

#[derive(Debug, Clone, PartialEq, uniffi::Enum)]
pub enum ProjectState {
    Working,
    Idle,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, uniffi::Enum)]
pub enum StopTargetView {
    Process { pid: i32, start_us: u64 },
    Container { id: String },
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct RunningSession {
    pub harness: Option<String>,
    pub title: Option<String>,
    pub branch: Option<String>,
    pub stopped: bool,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct RunningPort {
    pub port: u16,
    pub name: String,
    pub kind: String,
    pub worktree: Option<String>,
    pub harness: Option<String>,
    pub container: bool,
    pub openable: bool,
    pub protected_in_group: bool,
    pub target: StopTargetView,
    pub stop_request: String,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct RunningProject {
    pub name: String,
    pub is_other: bool,
    pub state: ProjectState,
    pub path: Option<String>,
    pub up_since_ms: Option<i64>,
    pub sessions: Vec<RunningSession>,
    pub ports: Vec<RunningPort>,
    pub stoppable_ports: u32,
    pub child_stop_request: Option<String>,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct RunningView {
    pub unknown_reason: Option<String>,
    pub projects: Vec<RunningProject>,
    pub hidden_system_ports: u32,
    pub stop_all: Vec<String>,
    pub stop_all_ports: u32,
    pub in_use_ports: u32,
    pub other_ports_left: u32,
}

/// Navori's own identity, which a stop must leave alone.
#[derive(Debug, Clone)]
pub struct Own {
    pub pid: i32,
    pub helper: Option<PathBuf>,
}

/// What the Awake view says while its Module is not running.
pub(crate) const AWAKE_STOPPED: &str = "The Awake module stopped.";
const OTHER: &str = "Other";
const AWAKE_UNREADABLE: &str = "Awake cannot be read, so Stop is off.";
const TRACKING_OFF: &str = "Tracking agents is off, so Stop is off.";
const NO_LABEL: &str = "A session has no project label, so Stop is off.";

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum Key {
    Named(String),
    Other,
}

#[derive(Default)]
struct Draft<'a> {
    sessions: Vec<(&'a LeaseView, Option<&'a str>)>,
    entries: Vec<(&'a PortEntry, StopClass)>,
}

pub fn build(
    awake: Option<&AwakeView>,
    ports: Option<&PortsState>,
    own: Own,
) -> Option<RunningView> {
    let awake_down = awake.is_none_or(|awake| awake.problem.as_deref() == Some(AWAKE_STOPPED));
    if awake_down && ports.is_none() {
        return None;
    }
    let unknown_reason = unknown_reason(awake);
    let known = unknown_reason.is_none();
    let helper = own.helper.as_deref();

    let mut drafts: BTreeMap<Key, Draft> = BTreeMap::new();
    for lease in awake.iter().flat_map(|awake| &awake.leases) {
        if lease.kind != "session" {
            continue;
        }
        let Some(label) = lease.label.as_deref().filter(|label| !label.is_empty()) else {
            continue;
        };
        let (name, branch) = split_label(label);
        drafts
            .entry(Key::Named(name.to_owned()))
            .or_default()
            .sessions
            .push((lease, branch));
    }
    for entry in ports.iter().flat_map(|ports| ports.entries()) {
        let key = match &entry.place.project {
            Some(name) => Key::Named(name.clone()),
            None => Key::Other,
        };
        let class = stop_class(entry, own.pid, helper);
        drafts.entry(key).or_default().entries.push((entry, class));
    }

    let mut hidden_system_ports = 0;
    let mut projects: Vec<RunningProject> = drafts
        .into_iter()
        .map(|(key, draft)| {
            hidden_system_ports += count(
                draft
                    .entries
                    .iter()
                    .filter(|(_, class)| *class == StopClass::GroupProtected)
                    .count(),
            );
            project_of(key, draft, known)
        })
        .collect();
    projects.sort_by_cached_key(project_key);

    let mut stop_all = Vec::new();
    let mut stop_all_ports = 0;
    let mut in_use_ports = 0;
    let mut other_ports_left = 0;
    for project in &projects {
        match project.state {
            ProjectState::Idle if project.is_other => other_ports_left = project.stoppable_ports,
            ProjectState::Idle => {
                if let Some(request) = &project.child_stop_request {
                    stop_all.push(request.clone());
                    stop_all_ports += project.stoppable_ports;
                }
            }
            ProjectState::Working => in_use_ports += project.stoppable_ports,
            ProjectState::Unknown => {}
        }
    }

    Some(RunningView {
        unknown_reason,
        projects,
        hidden_system_ports,
        stop_all,
        stop_all_ports,
        in_use_ports,
        other_ports_left,
    })
}

fn count(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

fn project_of(key: Key, mut draft: Draft<'_>, known: bool) -> RunningProject {
    let (name, selector_name, is_other) = match key {
        Key::Named(name) => (name.clone(), Some(name), false),
        Key::Other => (OTHER.to_owned(), None, true),
    };
    let state = if !known {
        ProjectState::Unknown
    } else if draft.sessions.iter().any(|(lease, _)| !lease.stopped) {
        ProjectState::Working
    } else {
        ProjectState::Idle
    };

    let mut targets: Vec<Target> = Vec::new();
    let mut stoppable_ports = 0;
    for (entry, class) in &draft.entries {
        if known && *class == StopClass::Free {
            stoppable_ports += 1;
            let target = target_of(entry);
            if !targets.contains(&target) {
                targets.push(target);
            }
        }
    }
    let group_request =
        (!targets.is_empty()).then(|| request(&Selector::Project(selector_name), &targets));

    let oldest = draft
        .entries
        .iter()
        .map(|(entry, _)| *entry)
        .min_by_key(|entry| (entry.start_us, entry.pid));
    let ports = draft
        .entries
        .iter()
        .map(|(entry, class)| port_of(entry, *class, known))
        .collect();

    draft
        .sessions
        .sort_by_cached_key(|(lease, _)| (lease.stopped, shown(lease), lease.pid));
    let sessions = draft
        .sessions
        .iter()
        .map(|(lease, branch)| RunningSession {
            harness: lease.harness.clone(),
            title: lease.title.clone(),
            branch: branch.map(str::to_owned),
            stopped: lease.stopped,
        })
        .collect();

    RunningProject {
        name,
        is_other,
        path: oldest
            .and_then(|entry| entry.place.root.as_ref())
            .map(|root| root.to_string_lossy().into_owned()),
        up_since_ms: oldest.and_then(|entry| i64::try_from(entry.start_us / 1000).ok()),
        sessions,
        ports,
        stoppable_ports,
        child_stop_request: group_request,
        state,
    }
}

fn port_of(entry: &PortEntry, class: StopClass, known: bool) -> RunningPort {
    let target = target_of(entry);
    let stop_request = if !known
        || class == StopClass::Never
        || (class == StopClass::GroupProtected && entry.container.is_some())
    {
        String::new()
    } else {
        let selector = match &entry.container {
            Some(_) => Selector::Project(entry.place.project.clone()),
            None => Selector::Pid(entry.pid),
        };
        request(&selector, std::slice::from_ref(&target))
    };
    let container = entry.container.is_some();
    RunningPort {
        port: entry.port,
        name: entry
            .container
            .as_ref()
            .map_or_else(|| entry.name.clone(), |held| held.name.clone()),
        kind: kind_name(entry.kind).to_owned(),
        worktree: entry.place.worktree.clone(),
        harness: entry
            .harness
            .filter(|_| !container)
            .map(|harness| harness_name(harness).to_owned()),
        container,
        openable: !matches!(entry.kind, PortKind::Database | PortKind::System),
        protected_in_group: class == StopClass::GroupProtected,
        target: match target {
            Target::Process { pid, start_us } => StopTargetView::Process { pid, start_us },
            Target::Container { id } => StopTargetView::Container { id },
        },
        stop_request,
    }
}

fn shown(lease: &LeaseView) -> String {
    lease
        .title
        .as_deref()
        .or(lease.label.as_deref())
        .unwrap_or_default()
        .to_lowercase()
}

fn project_key(project: &RunningProject) -> (bool, bool, String, String) {
    (
        project.is_other,
        project.state == ProjectState::Working,
        project.name.to_lowercase(),
        project.name.clone(),
    )
}

/// The project a label joins and the branch after it, when both sides are non-empty.
fn split_label(label: &str) -> (&str, Option<&str>) {
    match label.split_once('/') {
        Some((project, branch)) if !project.is_empty() && !branch.is_empty() => {
            (project, Some(branch))
        }
        _ => (label, None),
    }
}

fn unknown_reason(awake: Option<&AwakeView>) -> Option<String> {
    let Some(awake) = awake else {
        return Some(AWAKE_UNREADABLE.to_owned());
    };
    if let Some(problem) = &awake.problem {
        return Some(problem.clone());
    }
    if !awake.tracking {
        return Some(TRACKING_OFF.to_owned());
    }
    awake
        .leases
        .iter()
        .filter(|lease| lease.kind == "session")
        .any(|lease| lease.label.as_deref().is_none_or(str::is_empty))
        .then(|| NO_LABEL.to_owned())
}

fn harness_name(harness: Harness) -> &'static str {
    harness.id()
}

fn kind_name(kind: PortKind) -> &'static str {
    match kind {
        PortKind::DevServer => "dev_server",
        PortKind::Database => "database",
        PortKind::Docker => "docker",
        PortKind::System => "system",
        PortKind::Other => "other",
    }
}

fn target_of(entry: &PortEntry) -> Target {
    match &entry.container {
        Some(held) => Target::Container {
            id: held.id.clone(),
        },
        None => Target::Process {
            pid: entry.pid,
            start_us: entry.start_us,
        },
    }
}

fn request(selector: &Selector, targets: &[Target]) -> String {
    json!({"kill": {"selector": selector, "targets": targets}}).to_string()
}

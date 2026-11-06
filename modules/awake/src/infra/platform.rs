use std::path::PathBuf;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use navori_macos::{
    ExitSink, ExitSource, KqueueExits, LidError, LidGuard, LidSetting, MacLid, MacPower,
    MacPowerSource, MacProcesses, PlatformError, PowerAssertions, PowerKind, PowerSource,
    PowerSourceError, PowerWatch, ProcEntry, ProcessSource, ReadError, StartTime,
};
use navori_procs::{
    Branch, Harness, HarnessLookup, Location, ProcId, ProcWatcher, ProcsError, Snapshot,
    harness_of, resolve_location,
};

use crate::domain::Reading;

pub type ProcessFactory = Box<dyn FnMut() -> Result<Arc<dyn ProcessSource>, PlatformError> + Send>;
pub type ExitsFactory =
    Box<dyn FnMut(ExitSink) -> Result<Box<dyn ExitSource>, PlatformError> + Send>;

/// Why `pid` is not a command this user can hold.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProcessClass {
    Live(ProcId),
    /// Gone, or a zombie.
    Finished,
    /// This user's uid does not own the process. The id is still the start time we read.
    OtherUser(ProcId),
    /// `entry` failed and the pid may still be the stored process.
    Unreadable,
}

/// Basename of the executable, when the process is still there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ExecutableName {
    Known(Option<String>),
    Gone,
}

/// Why a session hold could not name a harness process.
#[derive(Debug)]
pub enum Rejected {
    NotRunning,
    NotAHarness,
    Platform(PlatformError),
}

#[derive(Debug)]
struct UnsupportedLid;

impl LidSetting for UnsupportedLid {
    fn hold(&self) -> Result<LidGuard, LidError> {
        Err(LidError::Unavailable)
    }

    fn give_back(&self) -> Result<(), LidError> {
        Err(LidError::Unavailable)
    }

    fn sleep_reason(&self) -> Option<String> {
        None
    }
}

#[derive(Debug)]
struct UnsupportedPowerSource;

impl PowerSource for UnsupportedPowerSource {
    fn current(&self) -> Result<PowerKind, PowerSourceError> {
        Ok(PowerKind::Battery)
    }

    fn watch(&self) -> Result<PowerWatch, PowerSourceError> {
        Err(PowerSourceError::Unavailable)
    }
}

pub struct Platform {
    power: Arc<dyn PowerAssertions>,
    lid: Arc<dyn LidSetting>,
    power_source: Arc<dyn PowerSource>,
    processes: ProcessFactory,
    exits: ExitsFactory,
    open: Option<Arc<dyn ProcessSource>>,
}

impl std::fmt::Debug for Platform {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Platform")
            .field("power", &Arc::strong_count(&self.power))
            .field("lid", &Arc::strong_count(&self.lid))
            .field("power_source", &Arc::strong_count(&self.power_source))
            .finish_non_exhaustive()
    }
}

impl Platform {
    pub fn new(
        power: Arc<dyn PowerAssertions>,
        processes: ProcessFactory,
        exits: ExitsFactory,
    ) -> Self {
        Self {
            power,
            lid: Arc::new(UnsupportedLid),
            power_source: Arc::new(UnsupportedPowerSource),
            processes,
            exits,
            open: None,
        }
    }

    pub fn mac() -> Self {
        Self::new(
            Arc::new(MacPower),
            Box::new(|| Ok(Arc::new(MacProcesses::new()?) as Arc<dyn ProcessSource>)),
            Box::new(|sink| Ok(Box::new(KqueueExits::start(sink)?) as Box<dyn ExitSource>)),
        )
        .with_lid(Arc::new(MacLid))
        .with_power_source(Arc::new(MacPowerSource))
    }

    #[must_use]
    pub fn with_lid(mut self, lid: Arc<dyn LidSetting>) -> Self {
        self.lid = lid;
        self
    }

    #[must_use]
    pub fn with_power_source(mut self, power_source: Arc<dyn PowerSource>) -> Self {
        self.power_source = power_source;
        self
    }

    pub(crate) fn power(&self) -> Arc<dyn PowerAssertions> {
        Arc::clone(&self.power)
    }

    pub fn open_processes(&mut self) -> Result<Arc<dyn ProcessSource>, PlatformError> {
        (self.processes)()
    }

    pub fn open_exits(&mut self, sink: ExitSink) -> Result<Box<dyn ExitSource>, PlatformError> {
        (self.exits)(sink)
    }

    /// Harness for `pid`, or for `ppid` when `pid` is not running.
    ///
    /// The snapshot runs on a blocking thread. The label is the project location, or `None`
    /// when the working directory cannot be read. A Codex session uses `reported`, when absolute,
    /// instead of the process's directory. `Err` from the join means that task stopped.
    pub(crate) async fn resolve_session(
        &mut self,
        pid: i32,
        ppid: i32,
        reported: Option<&str>,
    ) -> Result<Result<(Harness, ProcId, Option<String>), Rejected>, tokio::task::JoinError> {
        let source = match self.ensure_processes() {
            Ok(source) => source,
            Err(err) => return Ok(Err(Rejected::Platform(err))),
        };
        let reported = reported.map(PathBuf::from);
        tokio::task::spawn_blocking(move || resolve_open(source, pid, ppid, reported)).await
    }

    /// One `entry`, kept only when it is this user's live process.
    pub fn identify(&mut self, pid: i32) -> Result<Option<ProcId>, PlatformError> {
        Ok(match self.classify(pid)? {
            ProcessClass::Live(id) => Some(id),
            ProcessClass::Finished | ProcessClass::OtherUser(_) | ProcessClass::Unreadable => None,
        })
    }

    pub(crate) fn classify(&mut self, pid: i32) -> Result<ProcessClass, PlatformError> {
        let source = self.ensure_processes()?;
        let uid = source.current_uid();
        let class = match source.entry(pid) {
            Ok(entry) if entry.zombie => ProcessClass::Finished,
            Ok(entry) if entry.ruid != uid => ProcessClass::OtherUser(proc_id(&entry)),
            Ok(entry) => ProcessClass::Live(proc_id(&entry)),
            Err(ReadError::Gone) => ProcessClass::Finished,
            Err(ReadError::Denied | ReadError::Malformed) => ProcessClass::Unreadable,
        };
        Ok(class)
    }

    pub(crate) fn executable_name(&mut self, pid: i32) -> Result<ExecutableName, PlatformError> {
        let source = self.ensure_processes()?;
        let name = match source.exe(pid) {
            Ok(path) => {
                let name = path
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .filter(|name| !name.is_empty());
                ExecutableName::Known(name)
            }
            Err(ReadError::Gone) => ExecutableName::Gone,
            Err(ReadError::Denied | ReadError::Malformed) => ExecutableName::Known(None),
        };
        Ok(name)
    }

    /// One snapshot of descendants. Start time decides busy; CPU time is not read.
    pub(crate) async fn read_descendants(
        &mut self,
        process: ProcId,
        first_hold: SystemTime,
    ) -> Reading {
        let source = match self.ensure_processes() {
            Ok(source) => source,
            Err(err) => {
                tracing::warn!(
                    %err,
                    "the process list could not be opened; the session stays until the cap"
                );
                return Reading::Unknown;
            }
        };
        match tokio::task::spawn_blocking(move || classify_descendants(source, process, first_hold))
            .await
        {
            Ok(reading) => reading,
            Err(err) => {
                tracing::error!(
                    %err,
                    "descendant snapshot task stopped; the session stays until the cap"
                );
                Reading::Unknown
            }
        }
    }

    /// One snapshot for every restored lease. `None` means the list could not be read.
    pub(crate) async fn fresh_labels(
        &mut self,
        commands: &[ProcId],
        sessions: &[ProcId],
    ) -> Option<FreshLabels> {
        if commands.is_empty() && sessions.is_empty() {
            return Some(FreshLabels {
                commands: Vec::new(),
                sessions: Vec::new(),
            });
        }
        let source = match self.ensure_processes() {
            Ok(source) => source,
            Err(err) => {
                tracing::warn!(
                    %err,
                    "the process list could not be opened; restored leases keep no label"
                );
                return None;
            }
        };
        let commands = commands.to_vec();
        let sessions = sessions.to_vec();
        match tokio::task::spawn_blocking(move || label_snapshot(source, &commands, &sessions))
            .await
        {
            Ok(labels) => labels,
            Err(err) => {
                tracing::error!(%err, "label snapshot task stopped");
                None
            }
        }
    }

    pub(crate) fn build_watcher(&mut self) -> Result<ProcWatcher, PlatformError> {
        let source = Arc::clone(&self.ensure_processes()?);
        ProcWatcher::new(source, |sink| self.open_exits(sink)).map_err(|err| match err {
            ProcsError::Platform { source } => source,
            ProcsError::Watch { errno } => PlatformError::Kqueue {
                op: "kevent",
                errno,
            },
        })
    }

    fn ensure_processes(&mut self) -> Result<Arc<dyn ProcessSource>, PlatformError> {
        if let Some(open) = &self.open {
            return Ok(Arc::clone(open));
        }
        let open = (self.processes)()?;
        self.open = Some(Arc::clone(&open));
        Ok(open)
    }
}

fn proc_id(entry: &ProcEntry) -> ProcId {
    ProcId {
        pid: entry.pid,
        start: entry.start,
    }
}

fn resolve_open(
    source: Arc<dyn ProcessSource>,
    pid: i32,
    ppid: i32,
    reported: Option<PathBuf>,
) -> Result<(Harness, ProcId, Option<String>), Rejected> {
    let snapshot = Snapshot::take(source).map_err(|err| match err {
        ProcsError::Platform { source } => Rejected::Platform(source),
        ProcsError::Watch { errno } => Rejected::Platform(PlatformError::Kqueue {
            op: "kevent",
            errno,
        }),
    })?;
    let (harness, process) = match harness_of(&snapshot, pid) {
        HarnessLookup::Found { harness, process } => (harness, process),
        HarnessLookup::NotAHarness => return Err(Rejected::NotAHarness),
        HarnessLookup::NotRunning => match harness_of(&snapshot, ppid) {
            HarnessLookup::Found { harness, process } => (harness, process),
            HarnessLookup::NotAHarness => return Err(Rejected::NotAHarness),
            HarnessLookup::NotRunning => return Err(Rejected::NotRunning),
        },
    };
    let label = match reported.filter(|cwd| harness == Harness::Codex && cwd.is_absolute()) {
        Some(cwd) => Some(place(&resolve_location(&cwd))),
        None => location_label(&snapshot, process),
    };
    Ok((harness, process, label))
}

fn classify_descendants(
    source: Arc<dyn ProcessSource>,
    process: ProcId,
    first_hold: SystemTime,
) -> Reading {
    let snapshot = match Snapshot::take(source) {
        Ok(snapshot) => snapshot,
        Err(err) => {
            tracing::warn!(
                %err,
                "the process list could not be read; the session stays until the cap"
            );
            return Reading::Unknown;
        }
    };
    let busy = snapshot
        .descendants(process)
        .iter()
        .any(|descendant| started_after(descendant.id.start, first_hold));
    if busy {
        Reading::NewDescendant
    } else {
        Reading::NoNewDescendant
    }
}

fn started_after(start: StartTime, first_hold: SystemTime) -> bool {
    let Ok(since) = first_hold.duration_since(UNIX_EPOCH) else {
        return false;
    };
    let Ok(hold_us) = u64::try_from(since.as_micros()) else {
        return false;
    };
    start.0 > hold_us
}

#[derive(Debug)]
pub(crate) struct FreshLabels {
    pub commands: Vec<Option<String>>,
    pub sessions: Vec<Option<String>>,
}

fn label_snapshot(
    source: Arc<dyn ProcessSource>,
    commands: &[ProcId],
    sessions: &[ProcId],
) -> Option<FreshLabels> {
    let snapshot = match Snapshot::take(source) {
        Ok(snapshot) => snapshot,
        Err(err) => {
            tracing::warn!(
                %err,
                "the process list could not be read; restored leases keep no label"
            );
            return None;
        }
    };
    Some(FreshLabels {
        commands: commands
            .iter()
            .map(|id| command_label(&snapshot, *id))
            .collect(),
        sessions: sessions
            .iter()
            .map(|id| location_label(&snapshot, *id))
            .collect(),
    })
}

fn command_label(snapshot: &Snapshot, id: ProcId) -> Option<String> {
    let process = snapshot.get(id.pid)?;
    if process.id != id {
        return None;
    }
    let path = process.exe.as_ref().ok()?;
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .filter(|name| !name.is_empty())
}

fn location_label(snapshot: &Snapshot, process: ProcId) -> Option<String> {
    match snapshot.cwd(process) {
        Ok(cwd) => Some(place(&resolve_location(&cwd))),
        Err(_) => {
            tracing::debug!(pid = process.pid, "the working directory could not be read");
            None
        }
    }
}

/// `project/worktree`, or `project/branch` when the checkout is not a linked worktree.
fn place(location: &Location) -> String {
    let second = location
        .worktree
        .clone()
        .unwrap_or_else(|| match &location.branch {
            Some(Branch::Named(name)) => name.clone(),
            Some(Branch::Detached { abbrev }) => abbrev.clone(),
            None => "unknown".to_owned(),
        });
    format!("{}/{}", location.project, second)
}

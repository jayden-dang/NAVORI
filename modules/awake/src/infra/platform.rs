use std::sync::Arc;

use navori_macos::{
    ExitSink, ExitSource, KqueueExits, MacPower, MacProcesses, PlatformError, PowerAssertions,
    ProcessSource, ReadError,
};
use navori_procs::{
    Branch, Harness, HarnessLookup, Location, ProcId, ProcWatcher, ProcsError, Snapshot,
    harness_of, resolve_location,
};

pub type ProcessFactory = Box<dyn FnMut() -> Result<Arc<dyn ProcessSource>, PlatformError> + Send>;
pub type ExitsFactory =
    Box<dyn FnMut(ExitSink) -> Result<Box<dyn ExitSource>, PlatformError> + Send>;

/// Why `pid` is not a command this user can hold.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProcessClass {
    Live(ProcId),
    /// Gone, a zombie, or a record that could not be read.
    Finished,
    OtherUser,
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

pub struct Platform {
    power: Arc<dyn PowerAssertions>,
    processes: ProcessFactory,
    exits: ExitsFactory,
    open: Option<Arc<dyn ProcessSource>>,
}

impl std::fmt::Debug for Platform {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Platform")
            .field("power", &Arc::strong_count(&self.power))
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
    /// when the working directory cannot be read. `Err` from the join means that task stopped.
    pub(crate) async fn resolve_session(
        &mut self,
        pid: i32,
        ppid: i32,
    ) -> Result<Result<(Harness, ProcId, Option<String>), Rejected>, tokio::task::JoinError> {
        let source = match self.ensure_processes() {
            Ok(source) => source,
            Err(err) => return Ok(Err(Rejected::Platform(err))),
        };
        tokio::task::spawn_blocking(move || resolve_open(source, pid, ppid)).await
    }

    /// One `entry`, kept only when it is this user's live process.
    pub fn identify(&mut self, pid: i32) -> Result<Option<ProcId>, PlatformError> {
        Ok(match self.classify(pid)? {
            ProcessClass::Live(id) => Some(id),
            ProcessClass::Finished | ProcessClass::OtherUser => None,
        })
    }

    pub(crate) fn classify(&mut self, pid: i32) -> Result<ProcessClass, PlatformError> {
        let source = self.ensure_processes()?;
        let uid = source.current_uid();
        let class = match source.entry(pid) {
            Ok(entry) if entry.zombie => ProcessClass::Finished,
            Ok(entry) if entry.ruid != uid => ProcessClass::OtherUser,
            Ok(entry) => ProcessClass::Live(ProcId {
                pid: entry.pid,
                start: entry.start,
            }),
            Err(ReadError::Gone | ReadError::Denied | ReadError::Malformed) => {
                ProcessClass::Finished
            }
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

fn resolve_open(
    source: Arc<dyn ProcessSource>,
    pid: i32,
    ppid: i32,
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
    Ok((harness, process, location_label(&snapshot, process)))
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

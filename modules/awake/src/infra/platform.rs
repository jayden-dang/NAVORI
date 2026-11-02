use std::sync::Arc;

use navori_macos::{
    ExitSink, ExitSource, KqueueExits, MacPower, MacProcesses, PlatformError, PowerAssertions,
    ProcessSource, ReadError,
};
use navori_procs::{ProcId, ProcWatcher, ProcsError};

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

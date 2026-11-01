//! In-memory process list.

use std::fmt;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use crate::{PlatformError, ProcEntry, ProcessSource, ReadError, StartTime};

/// One scripted process. Argument, directory, and CPU reads default to denied.
#[derive(Debug)]
pub struct FakeProcess {
    pub pid: i32,
    pub ppid: i32,
    pub ruid: u32,
    pub start: StartTime,
    pub zombie: bool,
    pub exe: Result<PathBuf, ReadError>,
    pub argv: Result<Vec<String>, ReadError>,
    pub cwd: Result<PathBuf, ReadError>,
    pub cpu: Result<Duration, ReadError>,
}

impl FakeProcess {
    /// A live process. `argv`, `cwd`, and `cpu` start denied.
    pub fn new(pid: i32, ppid: i32, ruid: u32, start_us: u64, exe: &str) -> Self {
        Self {
            pid,
            ppid,
            ruid,
            start: StartTime(start_us),
            zombie: false,
            exe: Ok(PathBuf::from(exe)),
            argv: Err(ReadError::Denied),
            cwd: Err(ReadError::Denied),
            cpu: Err(ReadError::Denied),
        }
    }

    /// Successful argument vector.
    pub fn argv(mut self, argv: &[&str]) -> Self {
        self.argv = Ok(argv.iter().copied().map(str::to_owned).collect());
        self
    }

    /// Successful working directory.
    pub fn cwd(mut self, cwd: impl Into<PathBuf>) -> Self {
        self.cwd = Ok(cwd.into());
        self
    }

    /// Successful CPU time.
    pub fn cpu(mut self, cpu: Duration) -> Self {
        self.cpu = Ok(cpu);
        self
    }

    /// Mark the process as a zombie.
    pub fn zombie(mut self) -> Self {
        self.zombie = true;
        self
    }

    /// The executable read fails with `err`.
    pub fn exe_err(mut self, err: ReadError) -> Self {
        self.exe = Err(err);
        self
    }
}

struct PendingReplace {
    pid: i32,
    kind: &'static str,
    start_us: u64,
    ruid: u32,
}

struct State {
    current_uid: u32,
    processes: Vec<FakeProcess>,
    pending: Vec<PendingReplace>,
    reads: Vec<(i32, &'static str)>,
    /// `entry` for these pids fails with this error instead of returning the row.
    entry_errors: Vec<(i32, ReadError)>,
}

/// [`ProcessSource`](crate::ProcessSource) backed by the processes added in this process.
pub struct FakeProcesses {
    state: Mutex<State>,
}

impl fmt::Debug for FakeProcesses {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let state = lock(&self.state);
        let pids: Vec<i32> = state.processes.iter().map(|process| process.pid).collect();
        f.debug_struct("FakeProcesses")
            .field("current_uid", &state.current_uid)
            .field("pids", &pids)
            .finish()
    }
}

impl FakeProcesses {
    /// Empty list. `current_uid` is what [`ProcessSource::current_uid`] reports.
    pub fn new(current_uid: u32) -> Self {
        Self {
            state: Mutex::new(State {
                current_uid,
                processes: Vec::new(),
                pending: Vec::new(),
                reads: Vec::new(),
                entry_errors: Vec::new(),
            }),
        }
    }

    /// The next `entry` reads of `pid` fail with `err`. The row stays in `list`.
    pub fn fail_entry(&self, pid: i32, err: ReadError) {
        lock(&self.state).entry_errors.push((pid, err));
    }

    /// Append a process. A repeated pid is a second row; reads find the first.
    pub fn add(&self, process: FakeProcess) {
        lock(&self.state).processes.push(process);
    }

    /// Remove every row with this pid.
    pub fn exit(&self, pid: i32) {
        lock(&self.state)
            .processes
            .retain(|process| process.pid != pid);
    }

    /// Give the first row with this pid a new start time and real uid.
    pub fn replace(&self, pid: i32, start_us: u64, ruid: u32) {
        if let Some(process) = lock(&self.state)
            .processes
            .iter_mut()
            .find(|process| process.pid == pid)
        {
            process.start = StartTime(start_us);
            process.ruid = ruid;
        }
    }

    /// On the next read of `kind` (`"exe"`, `"argv"`, `"cwd"`, or `"cpu"`), apply [`Self::replace`].
    pub fn replace_on_read(&self, pid: i32, kind: &'static str, start_us: u64, ruid: u32) {
        lock(&self.state).pending.push(PendingReplace {
            pid,
            kind,
            start_us,
            ruid,
        });
    }

    /// Pids whose executable, arguments, working directory, or CPU time were read, in order.
    pub fn reads(&self) -> Vec<(i32, &'static str)> {
        lock(&self.state).reads.clone()
    }

    fn read<T>(
        &self,
        pid: i32,
        kind: &'static str,
        field: impl FnOnce(&FakeProcess) -> Result<T, ReadError>,
    ) -> Result<T, ReadError> {
        let mut state = lock(&self.state);
        state.reads.push((pid, kind));
        apply_pending(&mut state, pid, kind);
        match state.processes.iter().find(|process| process.pid == pid) {
            Some(process) => field(process),
            None => Err(ReadError::Gone),
        }
    }
}

impl ProcessSource for FakeProcesses {
    fn current_uid(&self) -> u32 {
        lock(&self.state).current_uid
    }

    fn list(&self) -> Result<Vec<ProcEntry>, PlatformError> {
        Ok(lock(&self.state).processes.iter().map(entry_of).collect())
    }

    fn entry(&self, pid: i32) -> Result<ProcEntry, ReadError> {
        let state = lock(&self.state);
        if let Some((_, err)) = state.entry_errors.iter().find(|(id, _)| *id == pid) {
            return Err(*err);
        }
        state
            .processes
            .iter()
            .find(|process| process.pid == pid)
            .map(entry_of)
            .ok_or(ReadError::Gone)
    }

    fn exe(&self, pid: i32) -> Result<PathBuf, ReadError> {
        self.read(pid, "exe", |process| process.exe.clone())
    }

    fn argv(&self, pid: i32) -> Result<Vec<String>, ReadError> {
        self.read(pid, "argv", |process| process.argv.clone())
    }

    fn cwd(&self, pid: i32) -> Result<PathBuf, ReadError> {
        self.read(pid, "cwd", |process| process.cwd.clone())
    }

    fn cpu_time(&self, pid: i32) -> Result<Duration, ReadError> {
        self.read(pid, "cpu", |process| process.cpu)
    }
}

fn entry_of(process: &FakeProcess) -> ProcEntry {
    ProcEntry {
        pid: process.pid,
        ppid: process.ppid,
        ruid: process.ruid,
        start: process.start,
        zombie: process.zombie,
    }
}

fn apply_pending(state: &mut State, pid: i32, kind: &str) {
    let pending = std::mem::take(&mut state.pending);
    let mut kept = Vec::new();
    for item in pending {
        if item.pid == pid && item.kind == kind {
            if let Some(process) = state
                .processes
                .iter_mut()
                .find(|process| process.pid == pid)
            {
                process.start = StartTime(item.start_us);
                process.ruid = item.ruid;
            }
        } else {
            kept.push(item);
        }
    }
    state.pending = kept;
}

fn lock(state: &Mutex<State>) -> MutexGuard<'_, State> {
    state.lock().unwrap_or_else(PoisonError::into_inner)
}

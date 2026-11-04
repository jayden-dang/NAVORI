//! In-memory process list.

use std::fmt;
use std::net::IpAddr;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use crate::procargs::Matcher;
use crate::{Listener, Marker, PlatformError, ProcEntry, ProcessSource, ReadError, StartTime};

/// One scripted process. Argument, environment, directory, CPU, and name reads default to denied; the socket read
/// defaults to no listeners.
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
    pub fds: Result<Vec<Listener>, ReadError>,
    pub env: Result<Vec<String>, ReadError>,
    pub name: Result<String, ReadError>,
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
            fds: Ok(Vec::new()),
            env: Err(ReadError::Denied),
            name: Err(ReadError::Denied),
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

    /// Add a listening TCP socket. An `addr` that is not an IP address makes the socket read malformed.
    pub fn listen(mut self, addr: &str, port: u16) -> Self {
        match (addr.parse::<IpAddr>(), self.fds.as_mut()) {
            (Ok(addr), Ok(listeners)) => listeners.push(Listener { addr, port }),
            (Err(_), _) => self.fds = Err(ReadError::Malformed),
            (Ok(_), Err(_)) => {}
        }
        self
    }

    /// The socket read fails with `err`.
    pub fn fds_err(mut self, err: ReadError) -> Self {
        self.fds = Err(err);
        self
    }

    /// Successful environment, as whole `NAME=value` entries.
    pub fn env(mut self, entries: &[&str]) -> Self {
        self.env = Ok(entries.iter().copied().map(str::to_owned).collect());
        self
    }

    /// Successful short name.
    pub fn name(mut self, name: &str) -> Self {
        self.name = Ok(name.to_owned());
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
    /// `entry` for these pids fails with this error. `skip` successful calls happen first.
    entry_errors: Vec<(i32, u32, ReadError)>,
    list_calls: usize,
    fail_list: bool,
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
                list_calls: 0,
                fail_list: false,
            }),
        }
    }

    /// The next `entry` reads of `pid` fail with `err`. The row stays in `list`.
    pub fn fail_entry(&self, pid: i32, err: ReadError) {
        self.fail_entry_after(pid, 0, err);
    }

    /// The first `succeed` `entry` calls for `pid` succeed. Later calls return `err`.
    pub fn fail_entry_after(&self, pid: i32, succeed: u32, err: ReadError) {
        lock(&self.state).entry_errors.push((pid, succeed, err));
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

    /// On the next read of `kind` (`"exe"`, `"argv"`, `"cwd"`, `"cpu"`, `"fds"`, `"env"`, or `"name"`), apply [`Self::replace`].
    pub fn replace_on_read(&self, pid: i32, kind: &'static str, start_us: u64, ruid: u32) {
        lock(&self.state).pending.push(PendingReplace {
            pid,
            kind,
            start_us,
            ruid,
        });
    }

    /// Pids whose executable, arguments, working directory, CPU time, sockets, environment, or name were read, in order.
    pub fn reads(&self) -> Vec<(i32, &'static str)> {
        lock(&self.state).reads.clone()
    }

    /// How many times [`ProcessSource::list`] has been called.
    pub fn list_calls(&self) -> usize {
        lock(&self.state).list_calls
    }

    /// When `fail` is true, later [`ProcessSource::list`] calls return a sysctl error.
    pub fn fail_list(&self, fail: bool) {
        lock(&self.state).fail_list = fail;
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
        let mut state = lock(&self.state);
        state.list_calls += 1;
        if state.fail_list {
            return Err(PlatformError::Sysctl { errno: 1 });
        }
        Ok(state.processes.iter().map(entry_of).collect())
    }

    fn entry(&self, pid: i32) -> Result<ProcEntry, ReadError> {
        let mut state = lock(&self.state);
        if let Some(fault) = state.entry_errors.iter_mut().find(|fault| fault.0 == pid) {
            if fault.1 == 0 {
                return Err(fault.2);
            }
            fault.1 -= 1;
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

    fn listeners(&self, pid: i32) -> Result<Vec<Listener>, ReadError> {
        self.read(pid, "fds", |process| process.fds.clone())
    }

    fn env_marker(&self, pid: i32, names: &[&str]) -> Result<Marker, ReadError> {
        self.read(pid, "env", |process| {
            let entries = process.env.as_ref().map_err(|err| *err)?;
            let mut matcher = Matcher::new(names);
            for entry in entries {
                matcher.offer(entry.as_bytes());
            }
            Ok(matcher.finish())
        })
    }

    fn name(&self, pid: i32) -> Result<String, ReadError> {
        self.read(pid, "name", |process| process.name.clone())
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

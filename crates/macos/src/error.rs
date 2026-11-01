//! Errors from process, exit and power reads.

/// Microseconds since the Unix epoch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StartTime(pub u64);

/// A process-list or watcher setup failure. The message names the call and the errno, never a path.
#[derive(Debug, thiserror::Error)]
pub enum PlatformError {
    /// `sysctl` failed. Retry the read.
    #[error("sysctl failed with errno {errno}; retry the process read")]
    Sysctl { errno: i32 },
    /// A kqueue change failed. `op` is the call that failed (`kqueue` or `kevent`). A failed `close` is logged and not returned.
    #[error("kqueue {op} failed with errno {errno}; the exit watch was not changed")]
    Kqueue { op: &'static str, errno: i32 },
    /// The `kinfo_proc` buffer is not the layout this reader checks at startup.
    #[error(
        "process record layout check failed for a {len}-byte buffer; expected 648-byte records and pid 1 with parent 0 and real uid 0"
    )]
    Layout { len: usize },
}

/// One process read failed. The message names the outcome only — no path, arguments, or environment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ReadError {
    /// The pid is not in the process table.
    #[error("reading the process failed because it is gone")]
    Gone,
    /// The caller is not allowed to read that process.
    #[error("reading the process was denied")]
    Denied,
    /// The kernel returned a record this reader cannot parse.
    #[error("reading the process failed because the record is malformed")]
    Malformed,
}

/// Registering an exit watch failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum WatchError {
    /// The pid is not in the process table, or it is already a zombie.
    #[error("watching the process failed because it is gone")]
    Gone,
    /// `kevent` failed while registering or removing the watch.
    #[error("kqueue failed with errno {errno}; the exit watch was not changed")]
    Kqueue { errno: i32 },
}

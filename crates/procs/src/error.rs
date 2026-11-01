//! Errors for the process model.

use navori_macos::PlatformError;

/// Why one process field could not be read. The snapshot still succeeds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum Unavailable {
    /// The process exited, or the pid now belongs to a different process.
    #[error("the process exited or was replaced; take a new snapshot")]
    Exited,
    /// The caller is not allowed to read that field.
    #[error("reading the process was denied")]
    PermissionDenied,
    /// The kernel record could not be parsed.
    #[error("reading the process failed because the record is malformed")]
    Malformed,
}

/// One process field. `Err` is why it could not be read, not a failed snapshot.
pub type Field<T> = Result<T, Unavailable>;

/// The named process is not in the snapshot, or its live entry no longer matches.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("the process is not running; take a new snapshot")]
pub struct NotRunning;

/// A snapshot or an exit watch could not be set up.
#[derive(Debug, thiserror::Error)]
pub enum ProcsError {
    /// The process list could not be read.
    #[error("reading the process list failed ({source}); retry the snapshot")]
    Platform {
        /// The platform failure. It names the call and the errno, never a path.
        source: PlatformError,
    },
    /// Registering an exit watch failed in kqueue. Removing a watch logs a kqueue failure and does not return it.
    #[error("kqueue failed with errno {errno}; the exit watch was not changed")]
    Watch {
        /// errno from `kevent`.
        errno: i32,
    },
}

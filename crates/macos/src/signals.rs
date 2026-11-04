//! Sending signals to processes and process groups.

use crate::ReadError;
use crate::process::{last_errno, read_error};

/// Who receives a signal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignalTarget {
    /// One process.
    Process(i32),
    /// Every process in a process group.
    Group(i32),
}

/// A signal this crate can send.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Signal {
    /// Ask the process to exit.
    Term,
    /// End the process without asking.
    Kill,
}

/// A signal was not delivered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum SignalError {
    /// The process or group no longer exists.
    #[error("signalling failed because the target is gone")]
    Gone,
    /// The caller may not signal that target.
    #[error("signalling the target was denied")]
    Denied,
    /// The kernel refused the signal for another reason.
    #[error("signalling the target failed")]
    Failed,
}

/// Signals and process-group reads.
pub trait Signals: Send + Sync {
    /// The pid of this process.
    fn own_pid(&self) -> i32;
    /// The process group of this process.
    fn own_group(&self) -> i32;
    /// The process group of `pid`.
    fn group_of(&self, pid: i32) -> Result<i32, ReadError>;
    /// Deliver `signal` to `to`.
    fn send(&self, to: SignalTarget, signal: Signal) -> Result<(), SignalError>;
}

/// [`Signals`] backed by the kernel.
#[derive(Debug, Default, Clone, Copy)]
pub struct MacSignals;

impl MacSignals {
    /// A handle on the real signal calls.
    pub fn new() -> Self {
        Self
    }
}

impl Signals for MacSignals {
    fn own_pid(&self) -> i32 {
        unsafe {
            // SAFETY: getpid takes no argument and cannot fail.
            libc::getpid()
        }
    }

    fn own_group(&self) -> i32 {
        unsafe {
            // SAFETY: getpgrp takes no argument and cannot fail.
            libc::getpgrp()
        }
    }

    fn group_of(&self, pid: i32) -> Result<i32, ReadError> {
        let group = unsafe {
            // SAFETY: getpgid takes a pid by value and touches no caller memory.
            libc::getpgid(pid)
        };
        if group < 0 {
            return Err(read_error(last_errno()));
        }
        Ok(group)
    }

    fn send(&self, to: SignalTarget, signal: Signal) -> Result<(), SignalError> {
        let target = match to {
            SignalTarget::Process(pid) => pid,
            SignalTarget::Group(group) => group.checked_neg().ok_or(SignalError::Failed)?,
        };
        let number = match signal {
            Signal::Term => libc::SIGTERM,
            Signal::Kill => libc::SIGKILL,
        };
        let rc = unsafe {
            // SAFETY: kill takes a pid and a signal number by value and touches no caller memory.
            libc::kill(target, number)
        };
        if rc == 0 {
            return Ok(());
        }
        Err(match last_errno() {
            libc::ESRCH => SignalError::Gone,
            libc::EPERM => SignalError::Denied,
            _ => SignalError::Failed,
        })
    }
}

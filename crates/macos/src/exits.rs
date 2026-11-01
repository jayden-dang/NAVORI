//! Exit notifications for watched processes.

use std::fmt;
use std::ptr;
use std::thread::JoinHandle;

use crate::error::{PlatformError, WatchError};

/// Receives `(pid, token)` when a watched process exits.
pub type ExitSink = Box<dyn Fn(i32, u64) + Send + Sync>;

/// Register and remove process-exit watches. One exit is delivered for each successful watch.
pub trait ExitSource: Send {
    /// Watch `pid` until it exits. `token` comes back with that exit.
    ///
    /// # Errors
    ///
    /// [`WatchError::Gone`] when `pid` is not in the process table or is already a zombie.
    /// [`WatchError::Kqueue`] when the kernel rejects the watch. Nothing is registered then.
    fn watch(&mut self, pid: i32, token: u64) -> Result<(), WatchError>;

    /// Stop watching `pid`. A pid that already exited, or was never watched, is not an error.
    fn unwatch(&mut self, pid: i32);
}

/// One kqueue and the thread blocked in `kevent` that reports process exits.
pub struct KqueueExits {
    kq: i32,
    thread: Option<JoinHandle<()>>,
}

impl fmt::Debug for KqueueExits {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("KqueueExits")
            .field("kq", &self.kq)
            .field("running", &self.thread.is_some())
            .finish()
    }
}

impl KqueueExits {
    /// Start a watcher. `sink` is called on the watcher thread with `(pid, token)`.
    ///
    /// # Errors
    ///
    /// [`PlatformError::Kqueue`] when the queue cannot be created or the thread cannot start.
    /// Nothing is left open in that case.
    pub fn start(sink: ExitSink) -> Result<Self, PlatformError> {
        let kq = open_kqueue()?;
        if let Err(errno) = register_wake(kq) {
            abandon(kq);
            return Err(PlatformError::Kqueue {
                op: "kevent",
                errno,
            });
        }
        let thread = std::thread::Builder::new()
            .name("navori-exits".into())
            .spawn(move || watch_loop(kq, sink));
        let thread = match thread {
            Ok(thread) => thread,
            Err(err) => {
                abandon(kq);
                return Err(PlatformError::Kqueue {
                    op: "kevent",
                    errno: err.raw_os_error().unwrap_or(0),
                });
            }
        };
        Ok(Self {
            kq,
            thread: Some(thread),
        })
    }
}

impl ExitSource for KqueueExits {
    fn watch(&mut self, pid: i32, token: u64) -> Result<(), WatchError> {
        let change = proc_change(
            pid,
            libc::EV_ADD | libc::EV_ONESHOT | libc::EV_RECEIPT,
            libc::NOTE_EXIT,
            token as usize as *mut libc::c_void,
        );
        receipt(self.kq, &change)
    }

    fn unwatch(&mut self, pid: i32) {
        let change = proc_change(pid, libc::EV_DELETE | libc::EV_RECEIPT, 0, ptr::null_mut());
        match receipt(self.kq, &change) {
            Ok(()) => {}
            Err(WatchError::Gone) => {}
            Err(WatchError::Kqueue {
                errno: libc::ENOENT,
            }) => {}
            Err(WatchError::Kqueue { errno }) => {
                tracing::warn!(
                    errno,
                    "kqueue kevent failed; the exit watch was not changed"
                );
            }
        }
    }
}

impl Drop for KqueueExits {
    fn drop(&mut self) {
        let mut closed = false;
        if let Err(errno) = trigger_wake(self.kq) {
            tracing::warn!(
                errno,
                "kqueue kevent failed; the exit watch was not changed"
            );
            // The thread is still blocked in kevent. Closing the queue makes that
            // call return, so the join below can finish.
            if let Err(errno) = close_fd(self.kq) {
                tracing::warn!(errno, "kqueue close failed; the exit watch was not changed");
            }
            closed = true;
        }
        if let Some(thread) = self.thread.take() {
            // The thread may have panicked; joining still finishes it.
            let _ = thread.join();
        }
        if !closed && let Err(errno) = close_fd(self.kq) {
            tracing::warn!(errno, "kqueue close failed; the exit watch was not changed");
        }
    }
}

fn open_kqueue() -> Result<i32, PlatformError> {
    let kq = unsafe {
        // SAFETY: kqueue takes no pointers. It returns a new queue fd, or -1 and sets errno.
        libc::kqueue()
    };
    if kq < 0 {
        return Err(PlatformError::Kqueue {
            op: "kqueue",
            errno: crate::process::last_errno(),
        });
    }
    Ok(kq)
}

fn register_wake(kq: i32) -> Result<(), i32> {
    let change = user_event(libc::EV_ADD | libc::EV_CLEAR, 0);
    kevent_change(kq, &change, None).map(|_| ())
}

fn trigger_wake(kq: i32) -> Result<(), i32> {
    let change = user_event(0, libc::NOTE_TRIGGER);
    kevent_change(kq, &change, None).map(|_| ())
}

fn abandon(kq: i32) {
    if let Err(errno) = close_fd(kq) {
        tracing::warn!(errno, "kqueue close failed; the exit watch was not changed");
    }
}

fn close_fd(fd: i32) -> Result<(), i32> {
    let rc = unsafe {
        // SAFETY: fd is a kqueue descriptor this watcher owns. close releases that descriptor.
        libc::close(fd)
    };
    if rc == 0 {
        Ok(())
    } else {
        Err(crate::process::last_errno())
    }
}

fn watch_loop(kq: i32, sink: ExitSink) {
    let mut events = [blank_event(); 16];
    loop {
        let n = match kevent_wait(kq, &mut events) {
            Ok(n) => n,
            Err(errno) => {
                tracing::warn!(errno, "kqueue kevent failed; the exit watch stopped");
                return;
            }
        };
        let count = usize::try_from(n).unwrap_or(0);
        let mut stop = false;
        for event in events.iter().take(count) {
            if event.filter == libc::EVFILT_USER {
                stop = true;
            } else if event.filter == libc::EVFILT_PROC {
                sink(event.ident as i32, event.udata as usize as u64);
            }
        }
        if stop {
            return;
        }
    }
}

fn receipt(kq: i32, change: &libc::kevent) -> Result<(), WatchError> {
    let mut out = blank_event();
    let n =
        kevent_change(kq, change, Some(&mut out)).map_err(|errno| WatchError::Kqueue { errno })?;
    if n < 1 {
        return Err(WatchError::Kqueue { errno: 0 });
    }
    let flags = out.flags;
    let data = out.data;
    if flags & libc::EV_ERROR != 0 {
        if data == libc::ESRCH as libc::intptr_t {
            return Err(WatchError::Gone);
        }
        if data != 0 {
            return Err(WatchError::Kqueue { errno: data as i32 });
        }
        return Ok(());
    }
    if data != 0 {
        return Err(WatchError::Kqueue { errno: data as i32 });
    }
    Ok(())
}

fn kevent_change(
    kq: i32,
    change: &libc::kevent,
    mut out: Option<&mut libc::kevent>,
) -> Result<i32, i32> {
    loop {
        let (ptr, slots) = match out.as_deref_mut() {
            Some(event) => (ptr::from_mut(event), 1),
            None => (ptr::null_mut(), 0),
        };
        let n = unsafe {
            // SAFETY: `change` is one live kevent. `ptr` is null, or one writable kevent in `out`.
            // A null timeout applies the change and returns. EV_RECEIPT keeps that return from waiting.
            libc::kevent(kq, change, 1, ptr, slots, ptr::null())
        };
        if n < 0 {
            let errno = crate::process::last_errno();
            if errno == libc::EINTR {
                continue;
            }
            return Err(errno);
        }
        return Ok(n);
    }
}

fn kevent_wait(kq: i32, events: &mut [libc::kevent]) -> Result<i32, i32> {
    loop {
        let n = unsafe {
            // SAFETY: `events` is writable for its length. A null changelist and a null timeout
            // block until the kernel posts an event or the call is interrupted. No timer is set.
            libc::kevent(
                kq,
                ptr::null(),
                0,
                events.as_mut_ptr(),
                events.len() as i32,
                ptr::null(),
            )
        };
        if n < 0 {
            let errno = crate::process::last_errno();
            if errno == libc::EINTR {
                continue;
            }
            return Err(errno);
        }
        return Ok(n);
    }
}

fn proc_change(pid: i32, flags: u16, fflags: u32, udata: *mut libc::c_void) -> libc::kevent {
    libc::kevent {
        ident: pid as libc::uintptr_t,
        filter: libc::EVFILT_PROC,
        flags,
        fflags,
        data: 0,
        udata,
    }
}

fn user_event(flags: u16, fflags: u32) -> libc::kevent {
    libc::kevent {
        ident: 0,
        filter: libc::EVFILT_USER,
        flags,
        fflags,
        data: 0,
        udata: ptr::null_mut(),
    }
}

fn blank_event() -> libc::kevent {
    user_event(0, 0)
}

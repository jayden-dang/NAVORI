//! The power source that is providing power, and a watch for it changing.

use std::ffi::{c_char, c_int};
use std::io::{self, Read};
use std::os::fd::{AsRawFd, RawFd};
use std::os::unix::net::UnixStream;
#[cfg(feature = "fake")]
use std::sync::Arc;
#[cfg(feature = "fake")]
use std::sync::atomic::{AtomicUsize, Ordering};

use objc2_io_kit::{
    IOPSCopyPowerSourcesInfo, IOPSGetProvidingPowerSourceType, kIOPSNotifyPowerSource,
};

const NOTIFY_STATUS_OK: u32 = 0;
const TOKEN_LEN: usize = 4;

/// What is powering the Mac.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerKind {
    Adapter,
    Battery,
}

/// Reads the power source and watches it for changes.
pub trait PowerSource: Send + Sync {
    /// The source that is providing power now.
    ///
    /// # Errors
    ///
    /// [`PowerSourceError::Unavailable`] when the system does not name a source.
    fn current(&self) -> Result<PowerKind, PowerSourceError>;

    /// A watch whose descriptor turns readable when the source changes between adapter and
    /// battery. Register it before reading [`Self::current`], so a change in between is not lost.
    ///
    /// # Errors
    ///
    /// [`PowerSourceError::Register`] or [`PowerSourceError::Nonblocking`] when the watch
    /// could not be set up; nothing stays registered.
    fn watch(&self) -> Result<PowerWatch, PowerSourceError>;
}

/// The power source could not be read or watched.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum PowerSourceError {
    /// The system named no providing power source.
    #[error("the system named no providing power source")]
    Unavailable,
    /// `notify_register_file_descriptor` failed with `status`. Nothing was registered.
    #[error("registering the power-source watch failed with status {status}; nothing is watched")]
    Register { status: u32 },
    /// Making the watch descriptor non-blocking failed with `errno`. The watch was cancelled.
    #[error(
        "making the power-source watch non-blocking failed with errno {errno}; it was cancelled"
    )]
    Nonblocking { errno: i32 },
}

#[derive(Debug)]
enum Inner {
    Notify { fd: RawFd, token: c_int },
    Stream(UnixStream),
}

/// A descriptor that turns readable when the power source changes. Dropping it ends the watch.
#[derive(Debug)]
pub struct PowerWatch {
    inner: Inner,
    #[cfg(feature = "fake")]
    reads: Arc<AtomicUsize>,
}

impl PowerWatch {
    /// Watch `stream`, which a test writes 4-byte tokens to.
    #[cfg(feature = "fake")]
    pub fn from_stream(stream: UnixStream, reads: Arc<AtomicUsize>) -> io::Result<Self> {
        stream.set_nonblocking(true)?;
        Ok(Self {
            inner: Inner::Stream(stream),
            reads,
        })
    }

    /// Watch `stream`, which a test writes 4-byte tokens to.
    #[cfg(not(feature = "fake"))]
    pub fn from_stream(stream: UnixStream) -> io::Result<Self> {
        stream.set_nonblocking(true)?;
        Ok(Self {
            inner: Inner::Stream(stream),
        })
    }

    /// Read every pending token.
    ///
    /// Returns the number read after a change, `Ok(0)` when the watch is closed, and
    /// `Err(WouldBlock)` when nothing is pending. Call it through `AsyncFd::try_io` so
    /// readiness is cleared on `WouldBlock`.
    ///
    /// # Errors
    ///
    /// `WouldBlock` when empty, or the read error.
    pub fn read_tokens(&self) -> io::Result<usize> {
        #[cfg(feature = "fake")]
        self.reads.fetch_add(1, Ordering::SeqCst);
        match &self.inner {
            Inner::Notify { fd, .. } => drain(|buf| read_fd(*fd, buf)),
            Inner::Stream(stream) => drain(|buf| (&*stream).read(buf)),
        }
    }
}

impl AsRawFd for PowerWatch {
    fn as_raw_fd(&self) -> RawFd {
        match &self.inner {
            Inner::Notify { fd, .. } => *fd,
            Inner::Stream(stream) => stream.as_raw_fd(),
        }
    }
}

impl Drop for PowerWatch {
    fn drop(&mut self) {
        if let Inner::Notify { token, .. } = self.inner {
            // SAFETY: `token` came from a successful registration and is cancelled once, here.
            // Cancelling also closes the descriptor, so it is never closed separately.
            unsafe { notify_cancel(token) };
        }
    }
}

fn read_fd(fd: RawFd, buf: &mut [u8]) -> io::Result<usize> {
    // SAFETY: `buf` is writable for `buf.len()` bytes and `fd` is open for the watch's life.
    let n = unsafe { libc::read(fd, buf.as_mut_ptr().cast(), buf.len()) };
    usize::try_from(n).map_err(|_| io::Error::last_os_error())
}

fn drain(mut read: impl FnMut(&mut [u8]) -> io::Result<usize>) -> io::Result<usize> {
    let mut buf = [0u8; 64];
    let mut tokens = 0;
    loop {
        match read(&mut buf) {
            Ok(0) => return Ok(tokens),
            Ok(n) => tokens += n / TOKEN_LEN,
            Err(err) if err.kind() == io::ErrorKind::Interrupted => {}
            Err(err) if err.kind() == io::ErrorKind::WouldBlock && tokens > 0 => return Ok(tokens),
            Err(err) => return Err(err),
        }
    }
}

unsafe extern "C" {
    fn notify_register_file_descriptor(
        name: *const c_char,
        notify_fd: *mut c_int,
        flags: c_int,
        out_token: *mut c_int,
    ) -> u32;
    fn notify_cancel(token: c_int) -> u32;
}

/// The power source through IOPS and notify(3).
#[derive(Debug)]
pub struct MacPowerSource;

impl PowerSource for MacPowerSource {
    fn current(&self) -> Result<PowerKind, PowerSourceError> {
        let info = IOPSCopyPowerSourcesInfo().ok_or(PowerSourceError::Unavailable)?;
        // SAFETY: `info` is the blob `IOPSCopyPowerSourcesInfo` returned and is retained for
        // this call.
        let source = unsafe { IOPSGetProvidingPowerSourceType(Some(&info)) }
            .ok_or(PowerSourceError::Unavailable)?;
        Ok(if source.to_string() == "AC Power" {
            PowerKind::Adapter
        } else {
            PowerKind::Battery
        })
    }

    fn watch(&self) -> Result<PowerWatch, PowerSourceError> {
        let mut fd: c_int = -1;
        let mut token: c_int = 0;
        // SAFETY: the key is a NUL-terminated constant, and `fd` and `token` are writable
        // for the results. Flags 0 requests a plain registration.
        let status = unsafe {
            notify_register_file_descriptor(kIOPSNotifyPowerSource.as_ptr(), &mut fd, 0, &mut token)
        };
        if status != NOTIFY_STATUS_OK {
            return Err(PowerSourceError::Register { status });
        }
        // From here the watch owns the registration, so any early return cancels it.
        let watch = PowerWatch {
            inner: Inner::Notify { fd, token },
            #[cfg(feature = "fake")]
            reads: Arc::default(),
        };
        set_nonblocking(fd)?;
        Ok(watch)
    }
}

fn set_nonblocking(fd: RawFd) -> Result<(), PowerSourceError> {
    // SAFETY: `fd` is the open descriptor the registration returned.
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
    // SAFETY: as above; the flags are those just read plus `O_NONBLOCK`.
    let set =
        flags >= 0 && unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } >= 0;
    if set {
        Ok(())
    } else {
        Err(PowerSourceError::Nonblocking {
            errno: io::Error::last_os_error().raw_os_error().unwrap_or(0),
        })
    }
}

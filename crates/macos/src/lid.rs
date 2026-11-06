//! The lid-close sleep setting and the last sleep reason.

use std::ffi::{c_char, c_void};
use std::fmt;
use std::ptr::NonNull;

use objc2_core_foundation::{CFRetained, CFString, CFType};

type Release = Box<dyn FnOnce() -> Result<(), LidError> + Send>;

const SET_CLAMSHELL_SLEEP_STATE: u32 = 12;

/// Keeps the Mac awake with the lid closed, and reads why it last slept.
pub trait LidSetting: Send + Sync {
    /// Set the lid-close setting. The returned guard gives it back.
    ///
    /// # Errors
    ///
    /// [`LidError::Refused`] when the system declines, [`LidError::Unavailable`] when the
    /// system cannot be reached. The setting is unchanged in both cases.
    fn hold(&self) -> Result<LidGuard, LidError>;

    /// Clear the lid-close setting without a guard.
    ///
    /// # Errors
    ///
    /// The same errors as [`Self::hold`]; the setting stays as it was.
    fn give_back(&self) -> Result<(), LidError>;

    /// The system's `Last Sleep Reason`, when it can be read.
    fn sleep_reason(&self) -> Option<String>;
}

/// Owns the give-back of the lid-close setting. Dropping it gives the setting back.
pub struct LidGuard {
    release: Option<Release>,
}

impl fmt::Debug for LidGuard {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LidGuard")
            .field("pending", &self.release.is_some())
            .finish()
    }
}

impl LidGuard {
    /// Keep `release` until [`Self::release`] or drop. It runs at most once.
    pub fn new(release: Box<dyn FnOnce() -> Result<(), LidError> + Send>) -> Self {
        Self {
            release: Some(release),
        }
    }

    /// Give the setting back now.
    ///
    /// # Errors
    ///
    /// The error from the system when it declines; the setting is then still set.
    pub fn release(mut self) -> Result<(), LidError> {
        match self.release.take() {
            Some(release) => release(),
            None => Ok(()),
        }
    }
}

impl Drop for LidGuard {
    fn drop(&mut self) {
        if let Some(release) = self.release.take()
            && let Err(err) = release()
        {
            tracing::warn!(%err, "lid setting give-back failed");
        }
    }
}

/// The lid-close setting could not be changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum LidError {
    /// The system declined `IOConnectCallScalarMethod` with `code`. The setting is unchanged.
    #[error("the system refused the lid setting call (code {code}); the setting is unchanged")]
    Refused { code: i32 },
    /// The power service could not be opened, so the setting is unchanged.
    #[error("the power service could not be opened for the lid setting; the setting is unchanged")]
    Unavailable,
}

#[link(name = "IOKit", kind = "framework")]
unsafe extern "C" {
    static mach_task_self_: u32;
    fn IOServiceMatching(name: *const c_char) -> *mut c_void;
    fn IOServiceGetMatchingService(main_port: u32, matching: *mut c_void) -> u32;
    fn IOServiceOpen(service: u32, owning_task: u32, kind: u32, connect: *mut u32) -> i32;
    fn IOServiceClose(connect: u32) -> i32;
    fn IOObjectRelease(object: u32) -> i32;
    fn IOConnectCallScalarMethod(
        connect: u32,
        selector: u32,
        input: *const u64,
        input_count: u32,
        output: *mut u64,
        output_count: *mut u32,
    ) -> i32;
    fn IORegistryEntryCreateCFProperty(
        entry: u32,
        key: *const CFString,
        allocator: *const c_void,
        options: u32,
    ) -> *const CFType;
}

/// The lid-close setting and sleep reason through IOKit.
#[derive(Debug)]
pub struct MacLid;

impl LidSetting for MacLid {
    fn hold(&self) -> Result<LidGuard, LidError> {
        set_state(true)?;
        Ok(LidGuard::new(Box::new(|| set_state(false))))
    }

    fn give_back(&self) -> Result<(), LidError> {
        set_state(false)
    }

    fn sleep_reason(&self) -> Option<String> {
        let root = open_root_domain_service()?;
        let key = CFString::from_static_str("Last Sleep Reason");
        // SAFETY: `root` is a live service and `key` a retained string for this call. A null
        // allocator selects the default. The result is null or an owned (+1) CF object.
        let raw =
            unsafe { IORegistryEntryCreateCFProperty(root, &raw const *key, std::ptr::null(), 0) };
        // SAFETY: `root` is owned here and not used after this release.
        unsafe { IOObjectRelease(root) };
        // SAFETY: a non-null result was returned owned (+1) by a Create call, so
        // `CFRetained` takes over that one release.
        let value = unsafe { CFRetained::from_raw(NonNull::new(raw.cast_mut())?) };
        value.downcast::<CFString>().ok().map(|s| s.to_string())
    }
}

fn open_root_domain_service() -> Option<u32> {
    // SAFETY: the name is a NUL-terminated literal. The returned dictionary is null or owned.
    let matching = unsafe { IOServiceMatching(c"IOPMrootDomain".as_ptr()) };
    if matching.is_null() {
        return None;
    }
    // SAFETY: port 0 is the default main port. The call consumes `matching` whether or not
    // it finds a service, so it is not used afterwards.
    let service = unsafe { IOServiceGetMatchingService(0, matching) };
    (service != 0).then_some(service)
}

fn set_state(on: bool) -> Result<(), LidError> {
    let service = open_root_domain_service().ok_or(LidError::Unavailable)?;
    let mut conn: u32 = 0;
    // SAFETY: `mach_task_self_` is initialised by the runtime before `main`. `service` is a live
    // service and `conn` is writable for the new connection.
    let code = unsafe { IOServiceOpen(service, mach_task_self_, 0, &mut conn) };
    // SAFETY: `service` is owned here and not used after this release.
    unsafe { IOObjectRelease(service) };
    if code != 0 {
        return Err(LidError::Unavailable);
    }
    let input = [u64::from(on)];
    // SAFETY: `conn` is an open connection. `input` holds the one scalar the call is told of,
    // and no output is requested, so both output pointers are null.
    let code = unsafe {
        IOConnectCallScalarMethod(
            conn,
            SET_CLAMSHELL_SLEEP_STATE,
            input.as_ptr(),
            1,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    // SAFETY: `conn` is owned here and not used after this close.
    unsafe { IOServiceClose(conn) };
    if code == 0 {
        Ok(())
    } else {
        Err(LidError::Refused { code })
    }
}

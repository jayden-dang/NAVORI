//! Power assertions that keep the system or the display awake.

use std::fmt;

use objc2_core_foundation::CFString;
use objc2_io_kit::{
    IOPMAssertionCreateWithName, IOPMAssertionID, IOPMAssertionRelease, kIOPMAssertionLevelOn,
};

/// Which idle sleep a power assertion prevents.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssertionKind {
    /// `PreventUserIdleSystemSleep`.
    System,
    /// `PreventUserIdleDisplaySleep`.
    Display,
}

impl fmt::Display for AssertionKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::System => "system",
            Self::Display => "display",
        })
    }
}

/// Holds one power assertion until [`AssertionGuard::release`] or drop.
pub trait PowerAssertions: Send + Sync {
    /// Create one assertion of `kind`, named `reason`.
    ///
    /// # Errors
    ///
    /// [`PowerError::Refused`] when the system declines, carrying its return code.
    /// Nothing is held in that case.
    fn hold(&self, kind: AssertionKind, reason: &str) -> Result<AssertionGuard, PowerError>;
}

/// Owns the release of one power assertion. Dropping it releases the assertion.
pub struct AssertionGuard {
    release: Option<Box<dyn FnOnce() -> Result<(), PowerError> + Send>>,
}

impl fmt::Debug for AssertionGuard {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AssertionGuard")
            .field("pending", &self.release.is_some())
            .finish()
    }
}

impl AssertionGuard {
    /// Keep `release` until [`Self::release`] or drop. It runs at most once.
    pub fn new(release: impl FnOnce() -> Result<(), PowerError> + Send + 'static) -> Self {
        Self {
            release: Some(Box::new(release)),
        }
    }

    /// Release the assertion now.
    ///
    /// # Errors
    ///
    /// [`PowerError::ReleaseFailed`] when the system declines the release.
    pub fn release(mut self) -> Result<(), PowerError> {
        match self.release.take() {
            Some(release) => release(),
            None => Ok(()),
        }
    }
}

impl Drop for AssertionGuard {
    fn drop(&mut self) {
        if let Some(release) = self.release.take()
            && let Err(err) = release()
        {
            let code = match err {
                PowerError::Refused { code, .. } | PowerError::ReleaseFailed { code } => code,
            };
            tracing::warn!(code, "power assertion release failed");
        }
    }
}

/// A power assertion was refused or could not be released.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum PowerError {
    /// The system declined to create the assertion. Nothing is held.
    #[error("the system refused a {kind} power assertion (code {code}); nothing is held")]
    Refused { kind: AssertionKind, code: i32 },
    /// The system declined to release an assertion that was held.
    #[error("releasing a power assertion failed (code {code})")]
    ReleaseFailed { code: i32 },
}

impl PowerError {
    /// `None` when `code` is success (`0`); otherwise the refusal that `hold` returns.
    #[doc(hidden)]
    pub fn from_create(kind: AssertionKind, code: i32) -> Option<Self> {
        if code == 0 {
            None
        } else {
            Some(Self::Refused { kind, code })
        }
    }
}

/// Power assertions created through IOKit.
#[derive(Debug)]
pub struct MacPower;

impl PowerAssertions for MacPower {
    fn hold(&self, kind: AssertionKind, reason: &str) -> Result<AssertionGuard, PowerError> {
        let id = create_assertion(kind, reason)?;
        Ok(AssertionGuard::new(move || release_assertion(id)))
    }
}

fn assertion_type_name(kind: AssertionKind) -> &'static str {
    match kind {
        AssertionKind::System => "PreventUserIdleSystemSleep",
        AssertionKind::Display => "PreventUserIdleDisplaySleep",
    }
}

fn create_assertion(kind: AssertionKind, reason: &str) -> Result<IOPMAssertionID, PowerError> {
    let ty = CFString::from_static_str(assertion_type_name(kind));
    let name = CFString::from_str(reason);
    let mut id: IOPMAssertionID = 0;
    // SAFETY: `ty` and `name` are non-null retained strings that live for this call,
    // and `id` is a writable assertion id. IOKit copies both strings before returning.
    // A non-zero code means no assertion was created.
    let code = unsafe {
        IOPMAssertionCreateWithName(Some(&ty), kIOPMAssertionLevelOn, Some(&name), &mut id)
    };
    if let Some(err) = PowerError::from_create(kind, code) {
        return Err(err);
    }
    Ok(id)
}

fn release_assertion(id: IOPMAssertionID) -> Result<(), PowerError> {
    let code = IOPMAssertionRelease(id);
    if code == 0 {
        Ok(())
    } else {
        Err(PowerError::ReleaseFailed { code })
    }
}

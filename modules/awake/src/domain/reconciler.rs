use std::sync::Arc;

use navori_macos::{AssertionGuard, AssertionKind, PowerAssertions, PowerError};

use super::Wants;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Held {
    pub system: bool,
    pub display: bool,
    pub refused: Vec<AssertionKind>,
    pub release_failed: Option<i32>,
    pub released: bool,
}

pub struct Reconciler {
    power: Arc<dyn PowerAssertions>,
    name: String,
    system: Option<AssertionGuard>,
    display: Option<AssertionGuard>,
}

impl std::fmt::Debug for Reconciler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Reconciler")
            .field("name", &self.name)
            .field("system", &self.system)
            .field("display", &self.display)
            .finish_non_exhaustive()
    }
}

impl Reconciler {
    pub fn new(power: Arc<dyn PowerAssertions>, name: impl Into<String>) -> Self {
        Self {
            power,
            name: name.into(),
            system: None,
            display: None,
        }
    }

    pub fn apply(&mut self, wants: Wants) -> Held {
        let mut refused = Vec::new();
        let mut release_failed = None;
        let mut released = false;
        if !wants.display {
            released |= release(&mut self.display, &mut release_failed);
        }
        if !wants.system {
            released |= release(&mut self.system, &mut release_failed);
        }
        if wants.system && self.system.is_none() {
            self.hold(AssertionKind::System, &mut refused);
        }
        if wants.display && self.display.is_none() {
            self.hold(AssertionKind::Display, &mut refused);
        }
        Held {
            system: self.system.is_some(),
            display: self.display.is_some(),
            refused,
            release_failed,
            released,
        }
    }

    fn hold(&mut self, kind: AssertionKind, refused: &mut Vec<AssertionKind>) {
        match self.power.hold(kind, &self.name) {
            Ok(guard) => match kind {
                AssertionKind::System => self.system = Some(guard),
                AssertionKind::Display => self.display = Some(guard),
            },
            Err(PowerError::Refused { .. } | PowerError::ReleaseFailed { .. }) => {
                refused.push(kind);
            }
        }
    }
}

fn release(guard: &mut Option<AssertionGuard>, failed: &mut Option<i32>) -> bool {
    let Some(guard) = guard.take() else {
        return false;
    };
    match guard.release() {
        Ok(()) => true,
        Err(err) => {
            let code = match err {
                PowerError::ReleaseFailed { code } | PowerError::Refused { code, .. } => code,
            };
            if failed.is_none() {
                *failed = Some(code);
            }
            false
        }
    }
}

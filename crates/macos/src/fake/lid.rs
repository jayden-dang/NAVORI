//! In-memory lid setting.

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use crate::{LidError, LidGuard, LidSetting};

/// One change the code asked the lid setting to make.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LidCall {
    Set,
    Clear,
}

type Probe = Arc<dyn Fn(LidCall) + Send + Sync>;

struct State {
    set: bool,
    refuse_set: Option<i32>,
    refuse_clear: Option<i32>,
    reason: Option<String>,
    calls: Vec<LidCall>,
    probe: Option<Probe>,
}

/// Scripted [`LidSetting`](crate::LidSetting) that never calls IOKit.
#[derive(Clone)]
pub struct FakeLid {
    state: Arc<Mutex<State>>,
}

impl std::fmt::Debug for FakeLid {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FakeLid").finish_non_exhaustive()
    }
}

impl Default for FakeLid {
    fn default() -> Self {
        Self::new()
    }
}

impl FakeLid {
    /// The setting is clear, calls succeed and no sleep reason is scripted.
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(State {
                set: false,
                refuse_set: None,
                refuse_clear: None,
                reason: None,
                calls: Vec::new(),
                probe: None,
            })),
        }
    }

    /// Later sets fail with `code` and leave the setting clear.
    pub fn refuse_set(&self, code: i32) {
        lock(&self.state).refuse_set = Some(code);
    }

    /// Later clears fail with `code` and leave the setting as it was.
    pub fn refuse_clear(&self, code: i32) {
        lock(&self.state).refuse_clear = Some(code);
    }

    /// Later sets and clears succeed until refused again.
    pub fn allow(&self) {
        let mut state = lock(&self.state);
        state.refuse_set = None;
        state.refuse_clear = None;
    }

    /// The `Last Sleep Reason` that [`LidSetting::sleep_reason`] returns.
    pub fn set_reason(&self, reason: Option<&str>) {
        lock(&self.state).reason = reason.map(str::to_owned);
    }

    /// Every set and clear asked of the setting, in order, refused ones included.
    pub fn calls(&self) -> Vec<LidCall> {
        lock(&self.state).calls.clone()
    }

    /// Whether the setting is currently set.
    pub fn is_set(&self) -> bool {
        lock(&self.state).set
    }

    /// Run `probe` on every call, after the change and outside the fake's lock.
    pub fn on_call(&self, probe: Box<dyn Fn(LidCall) + Send + Sync>) {
        lock(&self.state).probe = Some(Arc::from(probe));
    }
}

impl LidSetting for FakeLid {
    fn hold(&self) -> Result<LidGuard, LidError> {
        apply(&self.state, LidCall::Set)?;
        let state = Arc::clone(&self.state);
        Ok(LidGuard::new(Box::new(move || {
            apply(&state, LidCall::Clear)
        })))
    }

    fn give_back(&self) -> Result<(), LidError> {
        apply(&self.state, LidCall::Clear)
    }

    fn sleep_reason(&self) -> Option<String> {
        lock(&self.state).reason.clone()
    }
}

fn apply(state: &Mutex<State>, call: LidCall) -> Result<(), LidError> {
    let (result, probe) = {
        let mut state = lock(state);
        state.calls.push(call);
        let refused = match call {
            LidCall::Set => state.refuse_set,
            LidCall::Clear => state.refuse_clear,
        };
        let result = match refused {
            Some(code) => Err(LidError::Refused { code }),
            None => {
                state.set = call == LidCall::Set;
                Ok(())
            }
        };
        (result, state.probe.clone())
    };
    if let Some(probe) = probe {
        probe(call);
    }
    result
}

fn lock(state: &Mutex<State>) -> MutexGuard<'_, State> {
    state.lock().unwrap_or_else(PoisonError::into_inner)
}

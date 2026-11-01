//! In-memory power assertions.

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use crate::{AssertionGuard, AssertionKind, PowerAssertions, PowerError};

#[derive(Debug)]
struct Held {
    id: u64,
    kind: AssertionKind,
    reason: String,
}

#[derive(Debug)]
struct State {
    refuse: Option<i32>,
    next_id: u64,
    held: Vec<Held>,
    releases: usize,
}

/// Scripted [`PowerAssertions`](crate::PowerAssertions) that never calls IOKit.
#[derive(Debug)]
pub struct FakePower {
    state: Arc<Mutex<State>>,
}

impl Default for FakePower {
    fn default() -> Self {
        Self::new()
    }
}

impl FakePower {
    /// No assertions held, and creates succeed.
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(State {
                refuse: None,
                next_id: 0,
                held: Vec::new(),
                releases: 0,
            })),
        }
    }

    /// Later [`PowerAssertions::hold`] calls fail with `code` and hold nothing.
    pub fn refuse(&self, code: i32) {
        lock(&self.state).refuse = Some(code);
    }

    /// Assertions whose guards have not yet been released or dropped.
    pub fn held(&self) -> Vec<(AssertionKind, String)> {
        lock(&self.state)
            .held
            .iter()
            .map(|held| (held.kind, held.reason.clone()))
            .collect()
    }

    /// How many guards have run their release, including releases from drop.
    pub fn releases(&self) -> usize {
        lock(&self.state).releases
    }
}

impl PowerAssertions for FakePower {
    fn hold(&self, kind: AssertionKind, reason: &str) -> Result<AssertionGuard, PowerError> {
        let id = {
            let mut state = lock(&self.state);
            if let Some(code) = state.refuse {
                return Err(PowerError::Refused { kind, code });
            }
            let id = state.next_id;
            state.next_id += 1;
            state.held.push(Held {
                id,
                kind,
                reason: reason.to_owned(),
            });
            id
        };
        let state = Arc::clone(&self.state);
        Ok(AssertionGuard::new(move || {
            let mut state = lock(&state);
            if let Some(index) = state.held.iter().position(|held| held.id == id) {
                state.held.remove(index);
                state.releases += 1;
            }
            Ok(())
        }))
    }
}

fn lock(state: &Mutex<State>) -> MutexGuard<'_, State> {
    state.lock().unwrap_or_else(PoisonError::into_inner)
}

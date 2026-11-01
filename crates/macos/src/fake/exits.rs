//! In-memory exit notifications.

use std::collections::HashMap;
use std::fmt;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use crate::{ExitSink, ExitSource, PlatformError, WatchError};

struct State {
    sink: Option<ExitSink>,
    /// Tokens ever passed to a successful watch, oldest first. Kept after `unwatch`.
    tokens: HashMap<i32, Vec<u64>>,
    watched: Vec<i32>,
    fail: HashMap<i32, WatchError>,
}

/// Scripted exit source. [`Debug`] prints watched pids and not the sink.
#[derive(Clone)]
pub struct FakeExitsHandle {
    state: Arc<Mutex<State>>,
}

impl fmt::Debug for FakeExitsHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let state = lock(&self.state);
        f.debug_struct("FakeExitsHandle")
            .field("watched", &state.watched)
            .finish_non_exhaustive()
    }
}

impl FakeExitsHandle {
    /// No watches, no sink, and the next registration succeeds.
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(State {
                sink: None,
                tokens: HashMap::new(),
                watched: Vec::new(),
                fail: HashMap::new(),
            })),
        }
    }

    /// Store `sink` and return the exit source that shares this handle's state.
    pub fn factory(&self) -> impl FnOnce(ExitSink) -> Result<Box<dyn ExitSource>, PlatformError> {
        let state = Arc::clone(&self.state);
        move |sink| {
            lock(&state).sink = Some(sink);
            Ok(Box::new(FakeExits {
                state: Arc::clone(&state),
            }))
        }
    }

    /// Deliver the newest token ever registered for `pid`, including one removed by `unwatch`.
    pub fn fire(&self, pid: i32) {
        let state = lock(&self.state);
        let Some(token) = state
            .tokens
            .get(&pid)
            .and_then(|tokens| tokens.last().copied())
        else {
            return;
        };
        if let Some(sink) = state.sink.as_ref() {
            sink(pid, token);
        }
    }

    /// Deliver `token` for `pid`, whether or not it is the current registration.
    pub fn fire_with(&self, pid: i32, token: u64) {
        let state = lock(&self.state);
        if let Some(sink) = state.sink.as_ref() {
            sink(pid, token);
        }
    }

    /// The next [`ExitSource::watch`] of `pid` fails with `err` and registers nothing.
    pub fn fail_next_watch(&self, pid: i32, err: WatchError) {
        lock(&self.state).fail.insert(pid, err);
    }

    /// Pids with a registration that has not been removed.
    pub fn watched(&self) -> Vec<i32> {
        lock(&self.state).watched.clone()
    }

    /// Tokens successfully registered for `pid`, oldest first.
    pub fn tokens(&self, pid: i32) -> Vec<u64> {
        lock(&self.state)
            .tokens
            .get(&pid)
            .cloned()
            .unwrap_or_default()
    }
}

impl Default for FakeExitsHandle {
    fn default() -> Self {
        Self::new()
    }
}

struct FakeExits {
    state: Arc<Mutex<State>>,
}

impl ExitSource for FakeExits {
    fn watch(&mut self, pid: i32, token: u64) -> Result<(), WatchError> {
        let mut state = lock(&self.state);
        if let Some(err) = state.fail.remove(&pid) {
            return Err(err);
        }
        state.tokens.entry(pid).or_default().push(token);
        if !state.watched.contains(&pid) {
            state.watched.push(pid);
        }
        Ok(())
    }

    fn unwatch(&mut self, pid: i32) {
        lock(&self.state).watched.retain(|watched| *watched != pid);
    }
}

fn lock(state: &Mutex<State>) -> MutexGuard<'_, State> {
    state.lock().unwrap_or_else(PoisonError::into_inner)
}

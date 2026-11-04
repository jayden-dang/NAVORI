//! In-memory signalling.

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use crate::{ProcessSource, ReadError, Signal, SignalError, SignalTarget, Signals};

use super::FakeProcesses;

struct State {
    own_pid: i32,
    own_group: i32,
    groups: HashMap<i32, i32>,
    ignores_term: HashSet<i32>,
    obeys: HashMap<i32, Arc<FakeProcesses>>,
    denied: HashSet<i32>,
    hook: Option<Arc<dyn Fn() + Send + Sync>>,
    sent: Vec<(SignalTarget, Signal)>,
}

/// [`Signals`] that records what it delivers and ends scripted processes in a [`FakeProcesses`].
pub struct FakeSignals {
    state: Mutex<State>,
}

impl fmt::Debug for FakeSignals {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let state = lock(&self.state);
        f.debug_struct("FakeSignals")
            .field("own_pid", &state.own_pid)
            .field("sent", &state.sent)
            .finish()
    }
}

impl FakeSignals {
    /// A seam whose own process is `own_pid` in group `own_group`. Nothing is obedient, denied or ignoring yet.
    pub fn new(own_pid: i32, own_group: i32) -> Self {
        Self {
            state: Mutex::new(State {
                own_pid,
                own_group,
                groups: HashMap::new(),
                ignores_term: HashSet::new(),
                obeys: HashMap::new(),
                denied: HashSet::new(),
                hook: None,
                sent: Vec::new(),
            }),
        }
    }

    /// `group_of(pid)` answers `pgid`.
    pub fn set_group(&self, pid: i32, pgid: i32) {
        lock(&self.state).groups.insert(pid, pgid);
    }

    /// `Term` leaves `pid` running. `Kill` still ends it.
    pub fn ignore_term(&self, pid: i32) {
        lock(&self.state).ignores_term.insert(pid);
    }

    /// A signal that reaches `pid` removes it from `processes`.
    pub fn obeys(&self, pid: i32, processes: &Arc<FakeProcesses>) {
        lock(&self.state).obeys.insert(pid, Arc::clone(processes));
    }

    /// Signals to `pid` fail with [`SignalError::Denied`].
    pub fn deny(&self, pid: i32) {
        lock(&self.state).denied.insert(pid);
    }

    /// Run `hook` at the start of every `send`, before anything is recorded or ended.
    pub fn before_send(&self, hook: Box<dyn Fn() + Send + Sync>) {
        lock(&self.state).hook = Some(Arc::from(hook));
    }

    /// Delivered signals, in order. Denied sends are not listed.
    pub fn sent(&self) -> Vec<(SignalTarget, Signal)> {
        lock(&self.state).sent.clone()
    }
}

impl Signals for FakeSignals {
    fn own_pid(&self) -> i32 {
        lock(&self.state).own_pid
    }

    fn own_group(&self) -> i32 {
        lock(&self.state).own_group
    }

    fn group_of(&self, pid: i32) -> Result<i32, ReadError> {
        lock(&self.state)
            .groups
            .get(&pid)
            .copied()
            .ok_or(ReadError::Gone)
    }

    fn send(&self, to: SignalTarget, signal: Signal) -> Result<(), SignalError> {
        if !crate::signals::names_one_target(to) {
            return Err(SignalError::Failed);
        }
        let hook = lock(&self.state).hook.clone();
        if let Some(hook) = hook {
            hook();
        }
        let mut state = lock(&self.state);
        let reached: Vec<i32> = match to {
            SignalTarget::Process(pid) => vec![pid],
            SignalTarget::Group(group) => state
                .groups
                .iter()
                .filter(|(_, pgid)| **pgid == group)
                .map(|(pid, _)| *pid)
                .collect(),
        };
        if reached.iter().any(|pid| state.denied.contains(pid)) {
            return Err(SignalError::Denied);
        }
        if let SignalTarget::Process(pid) = to
            && state
                .obeys
                .get(&pid)
                .is_some_and(|processes| processes.entry(pid).is_err())
        {
            return Err(SignalError::Gone);
        }
        state.sent.push((to, signal));
        for pid in reached {
            let ends = signal == Signal::Kill || !state.ignores_term.contains(&pid);
            if let (true, Some(processes)) = (ends, state.obeys.get(&pid)) {
                processes.exit(pid);
            }
        }
        Ok(())
    }
}

fn lock(state: &Mutex<State>) -> MutexGuard<'_, State> {
    state.lock().unwrap_or_else(PoisonError::into_inner)
}

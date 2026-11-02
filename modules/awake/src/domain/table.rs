use std::time::{Duration, SystemTime, UNIX_EPOCH};

use navori_procs::{Harness, ProcId};

use crate::state::{AwakeState, LeaseState};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Timing {
    pub grace: Duration,
    pub recheck: Duration,
    pub cap: Duration,
    pub session_ttl: Duration,
}

impl Timing {
    pub const FIXED: Self = Self {
        grace: Duration::from_secs(120),
        recheck: Duration::from_secs(60),
        cap: Duration::from_secs(2 * 60 * 60),
        session_ttl: Duration::from_secs(8 * 60 * 60),
    };
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LeaseKey {
    Manual,
    Command { pid: i32, start_us: u64 },
    Session { session_id: String },
}

impl LeaseKey {
    pub fn text(&self) -> String {
        match self {
            Self::Manual => "manual".to_owned(),
            Self::Command { pid, start_us } => format!("command:{pid}:{start_us}"),
            Self::Session { session_id } => format!("session:{session_id}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LeaseSource {
    Duration {
        started: SystemTime,
    },
    UntilOff,
    Command {
        process: ProcId,
    },
    Session {
        session_id: String,
        harness: Harness,
        process: ProcId,
        first_hold: SystemTime,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lease {
    pub key: LeaseKey,
    pub source: LeaseSource,
    pub deadline: Option<SystemTime>,
    pub label: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndReason {
    Deadline,
    ProcessExited,
    EndSession,
    ReleaseAll,
    Replaced,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ended {
    pub lease: Lease,
    pub reason: EndReason,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settled {
    pub ended: Vec<Ended>,
    pub to_check: Vec<(LeaseKey, ProcId, SystemTime)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Wants {
    pub system: bool,
    pub display: bool,
}

#[derive(Debug, Clone)]
enum Manual {
    Duration {
        started: SystemTime,
        deadline: SystemTime,
    },
    UntilOff {
        started: SystemTime,
    },
}

impl Manual {
    fn started(&self) -> SystemTime {
        match *self {
            Self::Duration { started, .. } | Self::UntilOff { started } => started,
        }
    }

    fn deadline(&self) -> Option<SystemTime> {
        match *self {
            Self::Duration { deadline, .. } => Some(deadline),
            Self::UntilOff { .. } => None,
        }
    }
}

#[derive(Debug, Clone)]
struct CommandLease {
    process: ProcId,
    started: SystemTime,
    label: Option<String>,
}

#[derive(Debug, Clone)]
struct SessionLease {
    session_id: String,
    harness: Harness,
    process: ProcId,
    first_hold: SystemTime,
    deadline: SystemTime,
    label: Option<String>,
}

#[derive(Debug, Clone)]
pub struct LeaseTable {
    timing: Timing,
    manual: Option<Manual>,
    commands: Vec<CommandLease>,
    sessions: Vec<SessionLease>,
}

impl LeaseTable {
    pub fn new(timing: Timing) -> Self {
        Self {
            timing,
            manual: None,
            commands: Vec::new(),
            sessions: Vec::new(),
        }
    }

    pub fn timing(&self) -> Timing {
        self.timing
    }

    pub(crate) fn hold_for(&mut self, started: SystemTime, deadline: SystemTime) -> Option<Ended> {
        self.replace_manual(Manual::Duration { started, deadline })
    }

    pub(crate) fn hold_until_off(&mut self, started: SystemTime) -> Option<Ended> {
        self.replace_manual(Manual::UntilOff { started })
    }

    /// One command key per process. A later session lease may share the process
    /// under a different key; that is not a second command row.
    pub(crate) fn hold_command(
        &mut self,
        process: ProcId,
        started: SystemTime,
        label: Option<String>,
    ) {
        if self.commands.iter().any(|lease| lease.process == process) {
            return;
        }
        self.commands.push(CommandLease {
            process,
            started,
            label,
        });
    }

    /// One session key. A repeated hold keeps First hold and moves the deadline.
    /// Returns whether this created the lease.
    pub(crate) fn hold_session(
        &mut self,
        session_id: String,
        harness: Harness,
        process: ProcId,
        first_hold: SystemTime,
        deadline: SystemTime,
        label: Option<String>,
    ) -> bool {
        if let Some(existing) = self
            .sessions
            .iter_mut()
            .find(|lease| lease.session_id == session_id)
        {
            existing.harness = harness;
            existing.process = process;
            existing.deadline = deadline;
            existing.label = label;
            return false;
        }
        self.sessions.push(SessionLease {
            session_id,
            harness,
            process,
            first_hold,
            deadline,
            label,
        });
        true
    }

    pub(crate) fn session_process(&self, session_id: &str) -> Option<ProcId> {
        self.sessions
            .iter()
            .find(|lease| lease.session_id == session_id)
            .map(|lease| lease.process)
    }

    pub(crate) fn end_session(&mut self, session_id: &str) -> Option<Ended> {
        let index = self
            .sessions
            .iter()
            .position(|lease| lease.session_id == session_id)?;
        let lease = self.sessions.remove(index);
        Some(Ended {
            lease: session_as_lease(&lease),
            reason: EndReason::EndSession,
        })
    }

    pub(crate) fn has_command(&self, process: ProcId) -> bool {
        self.commands.iter().any(|lease| lease.process == process)
    }

    pub fn leases_on(&self, process: ProcId) -> usize {
        let commands = self
            .commands
            .iter()
            .filter(|lease| lease.process == process)
            .count();
        let sessions = self
            .sessions
            .iter()
            .filter(|lease| lease.process == process)
            .count();
        commands + sessions
    }

    pub fn on_exit(&mut self, process: ProcId) -> Vec<Ended> {
        let mut ended = Vec::new();
        let mut kept = Vec::new();
        for lease in self.commands.drain(..) {
            if lease.process == process {
                ended.push(Ended {
                    lease: command_as_lease(&lease),
                    reason: EndReason::ProcessExited,
                });
            } else {
                kept.push(lease);
            }
        }
        self.commands = kept;
        let mut kept_sessions = Vec::new();
        for lease in self.sessions.drain(..) {
            if lease.process == process {
                ended.push(Ended {
                    lease: session_as_lease(&lease),
                    reason: EndReason::ProcessExited,
                });
            } else {
                kept_sessions.push(lease);
            }
        }
        self.sessions = kept_sessions;
        ended
    }

    pub(crate) fn process_ids(&self) -> Vec<ProcId> {
        let mut ids = Vec::new();
        for process in self
            .commands
            .iter()
            .map(|lease| lease.process)
            .chain(self.sessions.iter().map(|lease| lease.process))
        {
            if !ids.contains(&process) {
                ids.push(process);
            }
        }
        ids
    }

    fn replace_manual(&mut self, next: Manual) -> Option<Ended> {
        self.manual.replace(next).map(|lease| Ended {
            lease: to_lease(&lease),
            reason: EndReason::Replaced,
        })
    }

    pub(crate) fn release_all(&mut self) -> Vec<Ended> {
        let mut ended: Vec<Ended> = self
            .manual
            .take()
            .into_iter()
            .map(|lease| Ended {
                lease: to_lease(&lease),
                reason: EndReason::ReleaseAll,
            })
            .collect();
        ended.extend(self.commands.drain(..).map(|lease| Ended {
            lease: command_as_lease(&lease),
            reason: EndReason::ReleaseAll,
        }));
        ended.extend(self.sessions.drain(..).map(|lease| Ended {
            lease: session_as_lease(&lease),
            reason: EndReason::ReleaseAll,
        }));
        ended
    }

    pub fn settle(&mut self, now: SystemTime) -> Settled {
        let mut ended = Vec::new();
        let due = self
            .manual
            .as_ref()
            .and_then(Manual::deadline)
            .is_some_and(|deadline| now >= deadline);
        if due && let Some(lease) = self.manual.take() {
            ended.push(Ended {
                lease: to_lease(&lease),
                reason: EndReason::Deadline,
            });
        }
        let mut kept = Vec::new();
        for lease in self.sessions.drain(..) {
            if now >= lease.deadline {
                ended.push(Ended {
                    lease: session_as_lease(&lease),
                    reason: EndReason::Deadline,
                });
            } else {
                kept.push(lease);
            }
        }
        self.sessions = kept;
        Settled {
            ended,
            to_check: Vec::new(),
        }
    }

    pub fn wants(&self) -> Wants {
        Wants {
            system: self.manual.is_some() || !self.commands.is_empty() || !self.sessions.is_empty(),
            display: false,
        }
    }

    pub(crate) fn next_deadline(&self) -> Option<SystemTime> {
        let manual = self.manual.as_ref().and_then(Manual::deadline);
        let session = self.sessions.iter().map(|lease| lease.deadline).min();
        match (manual, session) {
            (Some(manual), Some(session)) => Some(manual.min(session)),
            (Some(manual), None) => Some(manual),
            (None, Some(session)) => Some(session),
            (None, None) => None,
        }
    }

    pub(crate) fn snapshot(&self, system_held: bool, display_held: bool) -> AwakeState {
        let mut leases = Vec::new();
        if let Some(lease) = &self.manual {
            leases.push(manual_state(lease));
        }
        for lease in &self.commands {
            leases.push(command_state(lease));
        }
        for lease in &self.sessions {
            leases.push(session_state(lease));
        }
        AwakeState {
            leases,
            system_held,
            display_held,
            since_ms: self.earliest_start().map(wall_ms),
            problem: None,
        }
    }

    fn earliest_start(&self) -> Option<SystemTime> {
        let manual = self.manual.as_ref().map(Manual::started);
        let command = self.commands.iter().map(|lease| lease.started).min();
        let session = self.sessions.iter().map(|lease| lease.first_hold).min();
        [manual, command, session].into_iter().flatten().min()
    }
}

pub(crate) fn harness_name(harness: Harness) -> &'static str {
    match harness {
        Harness::ClaudeCode => "Claude Code",
        Harness::Codex => "Codex",
        Harness::Grok => "Grok",
    }
}

fn manual_label(lease: &Manual) -> Option<String> {
    match lease {
        Manual::Duration { started, deadline } => match deadline.duration_since(*started) {
            Ok(span) if span == Duration::from_secs(3_600) => Some("1 hour".to_owned()),
            _ => None,
        },
        Manual::UntilOff { .. } => Some("Until off".to_owned()),
    }
}

fn manual_state(lease: &Manual) -> LeaseState {
    match lease {
        Manual::Duration { deadline, .. } => LeaseState {
            key: LeaseKey::Manual.text(),
            kind: "duration".to_owned(),
            harness: None,
            label: manual_label(lease),
            pid: None,
            stopped: false,
            ends_at_ms: Some(wall_ms(*deadline)),
        },
        Manual::UntilOff { .. } => LeaseState {
            key: LeaseKey::Manual.text(),
            kind: "until_off".to_owned(),
            harness: None,
            label: manual_label(lease),
            pid: None,
            stopped: false,
            ends_at_ms: None,
        },
    }
}

fn command_state(lease: &CommandLease) -> LeaseState {
    LeaseState {
        key: command_key(lease.process),
        kind: "command".to_owned(),
        harness: None,
        label: lease.label.clone(),
        pid: Some(lease.process.pid),
        stopped: false,
        ends_at_ms: None,
    }
}

fn command_key(process: ProcId) -> String {
    LeaseKey::Command {
        pid: process.pid,
        start_us: process.start.0,
    }
    .text()
}

fn session_state(lease: &SessionLease) -> LeaseState {
    LeaseState {
        key: LeaseKey::Session {
            session_id: lease.session_id.clone(),
        }
        .text(),
        kind: "session".to_owned(),
        harness: Some(harness_name(lease.harness).to_owned()),
        label: lease.label.clone(),
        pid: Some(lease.process.pid),
        stopped: false,
        ends_at_ms: Some(wall_ms(lease.deadline)),
    }
}

fn session_as_lease(lease: &SessionLease) -> Lease {
    Lease {
        key: LeaseKey::Session {
            session_id: lease.session_id.clone(),
        },
        source: LeaseSource::Session {
            session_id: lease.session_id.clone(),
            harness: lease.harness,
            process: lease.process,
            first_hold: lease.first_hold,
        },
        deadline: Some(lease.deadline),
        label: lease.label.clone(),
    }
}

fn command_as_lease(lease: &CommandLease) -> Lease {
    Lease {
        key: LeaseKey::Command {
            pid: lease.process.pid,
            start_us: lease.process.start.0,
        },
        source: LeaseSource::Command {
            process: lease.process,
        },
        deadline: None,
        label: lease.label.clone(),
    }
}

fn to_lease(lease: &Manual) -> Lease {
    match lease {
        Manual::Duration { started, deadline } => Lease {
            key: LeaseKey::Manual,
            source: LeaseSource::Duration { started: *started },
            deadline: Some(*deadline),
            label: manual_label(lease),
        },
        Manual::UntilOff { .. } => Lease {
            key: LeaseKey::Manual,
            source: LeaseSource::UntilOff,
            deadline: None,
            label: manual_label(lease),
        },
    }
}

fn wall_ms(time: SystemTime) -> i64 {
    let Ok(duration) = time.duration_since(UNIX_EPOCH) else {
        return 0;
    };
    i64::try_from(duration.as_millis()).unwrap_or(i64::MAX)
}

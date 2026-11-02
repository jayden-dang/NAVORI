use std::time::{Duration, SystemTime, UNIX_EPOCH};

use navori_procs::ProcId;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LeaseKey {
    Manual,
    Command { pid: i32, start_us: u64 },
}

impl LeaseKey {
    pub fn text(&self) -> String {
        match self {
            Self::Manual => "manual".to_owned(),
            Self::Command { pid, start_us } => format!("command:{pid}:{start_us}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LeaseSource {
    Duration { started: SystemTime },
    UntilOff,
    Command { process: ProcId },
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
pub struct LeaseTable {
    timing: Timing,
    manual: Option<Manual>,
    commands: Vec<CommandLease>,
}

impl LeaseTable {
    pub fn new(timing: Timing) -> Self {
        Self {
            timing,
            manual: None,
            commands: Vec::new(),
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

    pub fn leases_on(&self, process: ProcId) -> usize {
        self.commands
            .iter()
            .filter(|lease| lease.process == process)
            .count()
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
        ended
    }

    pub(crate) fn process_ids(&self) -> Vec<ProcId> {
        let mut ids = Vec::new();
        for lease in &self.commands {
            if !ids.contains(&lease.process) {
                ids.push(lease.process);
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
        Settled {
            ended,
            to_check: Vec::new(),
        }
    }

    pub fn wants(&self) -> Wants {
        Wants {
            system: self.manual.is_some() || !self.commands.is_empty(),
            display: false,
        }
    }

    pub(crate) fn next_deadline(&self) -> Option<SystemTime> {
        self.manual.as_ref().and_then(Manual::deadline)
    }

    pub(crate) fn snapshot(&self, system_held: bool, display_held: bool) -> AwakeState {
        let mut leases = Vec::new();
        if let Some(lease) = &self.manual {
            leases.push(manual_state(lease));
        }
        for lease in &self.commands {
            leases.push(command_state(lease));
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
        match (manual, command) {
            (Some(manual), Some(command)) => Some(manual.min(command)),
            (Some(manual), None) => Some(manual),
            (None, Some(command)) => Some(command),
            (None, None) => None,
        }
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

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
}

impl LeaseKey {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Manual => "manual",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LeaseSource {
    Duration { started: SystemTime },
    UntilOff,
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
pub struct LeaseTable {
    timing: Timing,
    manual: Option<Manual>,
}

impl LeaseTable {
    pub fn new(timing: Timing) -> Self {
        Self {
            timing,
            manual: None,
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

    fn replace_manual(&mut self, next: Manual) -> Option<Ended> {
        self.manual.replace(next).map(|lease| Ended {
            lease: to_lease(&lease),
            reason: EndReason::Replaced,
        })
    }

    pub(crate) fn release_all(&mut self) -> Vec<Ended> {
        self.manual
            .take()
            .into_iter()
            .map(|lease| Ended {
                lease: to_lease(&lease),
                reason: EndReason::ReleaseAll,
            })
            .collect()
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
            system: self.manual.is_some(),
            display: false,
        }
    }

    pub(crate) fn next_deadline(&self) -> Option<SystemTime> {
        self.manual.as_ref().and_then(Manual::deadline)
    }

    pub(crate) fn snapshot(&self, system_held: bool, display_held: bool) -> AwakeState {
        let leases = self
            .manual
            .as_ref()
            .map(|lease| match lease {
                Manual::Duration { deadline, .. } => LeaseState {
                    key: LeaseKey::Manual.as_str().to_owned(),
                    kind: "duration".to_owned(),
                    harness: None,
                    label: manual_label(lease),
                    pid: None,
                    stopped: false,
                    ends_at_ms: Some(wall_ms(*deadline)),
                },
                Manual::UntilOff { .. } => LeaseState {
                    key: LeaseKey::Manual.as_str().to_owned(),
                    kind: "until_off".to_owned(),
                    harness: None,
                    label: manual_label(lease),
                    pid: None,
                    stopped: false,
                    ends_at_ms: None,
                },
            })
            .into_iter()
            .collect();
        AwakeState {
            leases,
            system_held,
            display_held,
            since_ms: self.manual.as_ref().map(|lease| wall_ms(lease.started())),
            problem: None,
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

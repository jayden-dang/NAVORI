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
struct DurationLease {
    started: SystemTime,
    deadline: SystemTime,
}

#[derive(Debug, Clone)]
pub struct LeaseTable {
    timing: Timing,
    duration: Option<DurationLease>,
}

impl LeaseTable {
    pub fn new(timing: Timing) -> Self {
        Self {
            timing,
            duration: None,
        }
    }

    pub fn timing(&self) -> Timing {
        self.timing
    }

    pub(crate) fn hold_for(&mut self, started: SystemTime, deadline: SystemTime) -> Option<Ended> {
        let previous = self.duration.replace(DurationLease { started, deadline });
        previous.map(|lease| Ended {
            lease: to_lease(&lease),
            reason: EndReason::Replaced,
        })
    }

    pub(crate) fn release_all(&mut self) -> Vec<Ended> {
        self.duration
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
            .duration
            .as_ref()
            .is_some_and(|lease| now >= lease.deadline);
        if due && let Some(lease) = self.duration.take() {
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
            system: self.duration.is_some(),
            display: false,
        }
    }

    pub(crate) fn next_deadline(&self) -> Option<SystemTime> {
        self.duration.as_ref().map(|lease| lease.deadline)
    }

    pub(crate) fn snapshot(&self, system_held: bool, display_held: bool) -> AwakeState {
        let leases = self
            .duration
            .iter()
            .map(|lease| LeaseState {
                key: LeaseKey::Manual.as_str().to_owned(),
                kind: "duration".to_owned(),
                harness: None,
                label: hour_label(lease),
                pid: None,
                stopped: false,
                ends_at_ms: Some(wall_ms(lease.deadline)),
            })
            .collect();
        AwakeState {
            leases,
            system_held,
            display_held,
            since_ms: self.duration.as_ref().map(|lease| wall_ms(lease.started)),
            problem: None,
        }
    }
}

fn hour_label(lease: &DurationLease) -> Option<String> {
    match lease.deadline.duration_since(lease.started) {
        Ok(span) if span == Duration::from_secs(3_600) => Some("1 hour".to_owned()),
        _ => None,
    }
}

fn to_lease(lease: &DurationLease) -> Lease {
    Lease {
        key: LeaseKey::Manual,
        source: LeaseSource::Duration {
            started: lease.started,
        },
        deadline: Some(lease.deadline),
        label: hour_label(lease),
    }
}

fn wall_ms(time: SystemTime) -> i64 {
    let Ok(duration) = time.duration_since(UNIX_EPOCH) else {
        return 0;
    };
    i64::try_from(duration.as_millis()).unwrap_or(i64::MAX)
}

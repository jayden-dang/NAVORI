use std::collections::BTreeMap;
use std::time::Duration;

use navori_macos::{ProcessSource, ReadError, Signal, SignalError, SignalTarget, Signals};

use super::reason;
use super::resolve::{FreshRead, Guard, ProcessPlan, Reach, never};
use crate::action::Outcome;

const TERM_WAIT: Duration = Duration::from_secs(3);
const KILL_WAIT: Duration = Duration::from_secs(1);

/// Decides, before any signal, whether a stop may reach the leader's whole process group.
///
/// `confirmed` lists the pid and start time of every target of the request that passed its checks.
/// Any doubt, including a group that cannot be read, means the pid alone.
pub fn reach_for(
    plan: &ProcessPlan,
    fresh: &FreshRead,
    signals: &dyn Signals,
    guard: &Guard,
    confirmed: &[(i32, u64)],
) -> Reach {
    let pid = plan.pid;
    if !guard.complete || pid <= 1 || signals.group_of(pid) != Ok(pid) || signals.own_group() == pid
    {
        return Reach::Alone;
    }
    for ancestor in &guard.ancestors {
        match signals.group_of(*ancestor) {
            Ok(group) if group != pid => {}
            _ => return Reach::Alone,
        }
    }
    let mut mates = BTreeMap::new();
    for listener in fresh.entries.iter().filter(|entry| entry.pid != pid) {
        let Ok(group) = signals.group_of(listener.pid) else {
            return Reach::Alone;
        };
        if group != pid {
            continue;
        }
        let known = confirmed.contains(&(listener.pid, listener.start_us));
        if !known || guard.ancestors.contains(&listener.pid) || never(listener, guard) {
            return Reach::Alone;
        }
        mates.insert(listener.pid, listener.start_us);
    }
    let mut members = vec![(pid, plan.start_us)];
    members.extend(mates);
    Reach::Group(members)
}

enum Presence {
    Gone,
    Same,
    Unreadable,
}

fn presence(source: &dyn ProcessSource, pid: i32, start_us: u64) -> Presence {
    match source.entry(pid) {
        Ok(current) => {
            if current.zombie || current.start.0 != start_us || current.ruid != source.current_uid()
            {
                Presence::Gone
            } else {
                Presence::Same
            }
        }
        Err(ReadError::Gone) => Presence::Gone,
        Err(_) => Presence::Unreadable,
    }
}

/// How one member of a signalled group fared, from its own check.
pub fn member_outcome(
    pid: i32,
    start_us: u64,
    source: &dyn ProcessSource,
) -> (Outcome, Option<&'static str>) {
    match presence(source, pid, start_us) {
        Presence::Gone => (Outcome::Stopped, None),
        Presence::Same => (Outcome::Failed, Some(reason::STILL_RUNNING)),
        Presence::Unreadable => (Outcome::Failed, Some(reason::PROCESS_LIST_TIMEOUT)),
    }
}

/// Asks the process to exit, waits, and ends it if it is still the same process.
///
/// The reach in `plan` is used as given; a group member other than the leader is answered
/// separately with [`member_outcome`].
pub async fn stop_process(
    plan: ProcessPlan,
    source: &dyn ProcessSource,
    signals: &dyn Signals,
) -> (Outcome, Option<&'static str>) {
    let (to, done) = match plan.reach {
        Reach::Group(_) => (SignalTarget::Group(plan.pid), Outcome::Stopped),
        Reach::Alone => (SignalTarget::Process(plan.pid), Outcome::StoppedAlone),
    };
    match signals.send(to, Signal::Term) {
        Ok(()) => {}
        Err(SignalError::Gone) => return (Outcome::AlreadyGone, None),
        Err(_) => return (Outcome::Failed, Some(reason::SIGNAL_REFUSED)),
    }
    tokio::time::sleep(TERM_WAIT).await;
    match presence(source, plan.pid, plan.start_us) {
        Presence::Gone => return (done, None),
        Presence::Unreadable => return (Outcome::Failed, Some(reason::PROCESS_LIST_TIMEOUT)),
        Presence::Same => {}
    }
    match signals.send(to, Signal::Kill) {
        Ok(()) => {}
        Err(SignalError::Gone) => return (done, None),
        Err(_) => return (Outcome::Failed, Some(reason::SIGNAL_REFUSED)),
    }
    tokio::time::sleep(KILL_WAIT).await;
    match presence(source, plan.pid, plan.start_us) {
        Presence::Gone => (done, None),
        Presence::Same => (Outcome::Failed, Some(reason::STILL_RUNNING)),
        Presence::Unreadable => (Outcome::Failed, Some(reason::PROCESS_LIST_TIMEOUT)),
    }
}

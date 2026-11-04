use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use futures::future::BoxFuture;
use futures::stream::{FuturesUnordered, StreamExt};
use navori_kernel::{Request, Severity};
use navori_macos::{ProcessSource, Signals};
use serde_json::{Value, json};
use tokio::task::{Id, JoinSet};
use tokio::time::Instant;

use super::process::{member_outcome, reach_for, stop_process};
use super::reason;
use super::resolve::{
    ContainerPlan, FreshRead, Guard, Plan, ProcessPlan, Reach, Resolved, resolve,
};
use crate::action::{KillReply, Outcome, PortsAction, Selector, Target, TargetResult};
use crate::infra::docker::{Containers, DockerError, Stopped};

const DEADLINE: Duration = Duration::from_secs(25);

const OUTCOMES: [(&str, &str); 8] = [
    ("stopped", "stopped"),
    ("stopped_alone", "stopped alone"),
    ("already_gone", "already gone"),
    ("still_stopping", "still stopping"),
    ("stale", "stale"),
    ("protected", "protected"),
    ("outside_selector", "outside the selector"),
    ("failed", "failed"),
];
const FAILED: usize = 7;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) enum Claim {
    Process(i32, u64),
    Container(String),
}

impl Claim {
    fn of(target: &Target) -> Self {
        match target {
            Target::Process { pid, start_us } => Self::Process(*pid, *start_us),
            Target::Container { id } => Self::Container(id.clone()),
        }
    }
}

pub(crate) struct StopDone {
    counts: [u32; 8],
}

impl StopDone {
    pub(crate) fn severity(&self) -> Severity {
        if self.counts[FAILED] > 0 {
            Severity::Warn
        } else {
            Severity::Info
        }
    }

    pub(crate) fn summary(&self) -> String {
        let parts: Vec<String> = OUTCOMES
            .iter()
            .zip(self.counts)
            .filter(|(_, count)| *count > 0)
            .map(|((_, words), count)| format!("{count} {words}"))
            .collect();
        format!("{}.", parts.join(", "))
    }

    pub(crate) fn payload(&self) -> Value {
        let counts: serde_json::Map<String, Value> = OUTCOMES
            .iter()
            .zip(self.counts)
            .map(|((key, _), count)| ((*key).to_owned(), json!(count)))
            .collect();
        Value::Object(counts)
    }
}

#[derive(Default)]
pub(crate) struct Stops {
    running: JoinSet<StopDone>,
    claims: HashMap<Id, Vec<Claim>>,
}

impl Stops {
    pub(crate) fn is_empty(&self) -> bool {
        self.running.is_empty()
    }

    pub(crate) async fn next(&mut self) -> Option<Result<StopDone, tokio::task::JoinError>> {
        let joined = self.running.join_next_with_id().await?;
        let id = match &joined {
            Ok((id, _)) => *id,
            Err(err) => err.id(),
        };
        self.claims.remove(&id);
        Some(joined.map(|(_, done)| done))
    }

    pub(crate) fn spawn(&mut self, mut task: StopTask) {
        let held: HashSet<&Claim> = self.claims.values().flatten().collect();
        let overlapping: Vec<bool> = task
            .targets
            .iter()
            .map(|target| held.contains(&Claim::of(target)))
            .collect();
        let mine: Vec<Claim> = task
            .targets
            .iter()
            .zip(&overlapping)
            .filter(|(_, overlaps)| !**overlaps)
            .map(|(target, _)| Claim::of(target))
            .collect();
        task.overlapping = overlapping;
        let id = self.running.spawn(task.run()).id();
        self.claims.insert(id, mine);
    }
}

pub(crate) struct StopEnv {
    pub source: Arc<dyn ProcessSource>,
    pub home: PathBuf,
    pub containers: Arc<dyn Containers>,
    pub signals: Arc<dyn Signals>,
    pub helper: Option<PathBuf>,
}

pub(crate) struct StopTask {
    request: Request<PortsAction>,
    env: StopEnv,
    selector: Selector,
    targets: Vec<Target>,
    arrival: Instant,
    overlapping: Vec<bool>,
}

impl StopTask {
    pub(crate) fn new(
        request: Request<PortsAction>,
        env: StopEnv,
        selector: Selector,
        targets: Vec<Target>,
        arrival: Instant,
    ) -> Self {
        Self {
            request,
            env,
            selector,
            targets,
            arrival,
            overlapping: Vec::new(),
        }
    }

    async fn run(self) -> StopDone {
        let Self {
            request,
            env,
            selector,
            targets,
            arrival,
            overlapping,
        } = self;
        let mut slots = open_slots(&targets, &overlapping);
        let _ = tokio::time::timeout_at(
            arrival + DEADLINE,
            drive(&env, &selector, &targets, &mut slots),
        )
        .await;
        let mut counts = [0; 8];
        let results = targets
            .into_iter()
            .enumerate()
            .map(|(at, target)| {
                let (result, why) = answer(&slots, at);
                counts[result as usize] += 1;
                TargetResult {
                    target,
                    result,
                    reason: why.map(str::to_owned),
                }
            })
            .collect();
        request.respond(Ok(KillReply { results }));
        StopDone { counts }
    }
}

type Answer = (Outcome, Option<&'static str>);

enum Slot {
    Pending,
    Running,
    Done(Answer),
    Follows(usize),
}

fn open_slots(targets: &[Target], overlapping: &[bool]) -> Vec<Slot> {
    let mut first: HashMap<Claim, usize> = HashMap::new();
    targets
        .iter()
        .enumerate()
        .map(|(at, target)| {
            if overlapping.get(at).copied().unwrap_or(false) {
                return Slot::Done((Outcome::StillStopping, Some(reason::STILL_STOPPING)));
            }
            match first.entry(Claim::of(target)) {
                std::collections::hash_map::Entry::Occupied(seen) => Slot::Follows(*seen.get()),
                std::collections::hash_map::Entry::Vacant(free) => {
                    free.insert(at);
                    Slot::Pending
                }
            }
        })
        .collect()
}

fn answer(slots: &[Slot], at: usize) -> Answer {
    match slots.get(at) {
        Some(Slot::Done(done)) => *done,
        Some(Slot::Running) => (Outcome::StillStopping, Some(reason::STILL_STOPPING)),
        Some(Slot::Follows(first)) => answer(slots, *first),
        Some(Slot::Pending) | None => (Outcome::Failed, Some(reason::PROCESS_LIST_TIMEOUT)),
    }
}

struct Finished {
    answered: Vec<(usize, Answer)>,
}

async fn drive(env: &StopEnv, selector: &Selector, targets: &[Target], slots: &mut [Slot]) {
    let asked: Vec<usize> = (0..slots.len())
        .filter(|at| matches!(slots[*at], Slot::Pending))
        .collect();
    if asked.is_empty() {
        return;
    }
    let read = FreshRead::take(
        Arc::clone(&env.source),
        env.home.clone(),
        Arc::clone(&env.containers),
    )
    .await;
    let fresh = match read {
        Ok(fresh) => fresh,
        Err(why) => {
            for at in asked {
                slots[at] = Slot::Done((Outcome::Failed, Some(why)));
            }
            return;
        }
    };
    let guard = Guard::new(
        Arc::clone(&env.signals),
        Arc::clone(&env.source),
        env.helper.clone(),
    );
    let asked_targets: Vec<Target> = asked.iter().map(|at| targets[*at].clone()).collect();
    let verdicts = resolve(&fresh, &guard, selector, &asked_targets);

    let mut processes: Vec<(usize, ProcessPlan)> = Vec::new();
    let mut containers: Vec<(usize, ContainerPlan)> = Vec::new();
    for (at, verdict) in asked.into_iter().zip(verdicts) {
        match verdict {
            Resolved::Refused(outcome, why) => slots[at] = Slot::Done((outcome, why)),
            Resolved::Go(Plan::Process(plan)) => {
                slots[at] = Slot::Running;
                processes.push((at, plan));
            }
            Resolved::Go(Plan::Container(plan)) => {
                slots[at] = Slot::Running;
                containers.push((at, plan));
            }
        }
    }

    let confirmed: Vec<(i32, u64)> = processes
        .iter()
        .map(|(_, plan)| (plan.pid, plan.start_us))
        .collect();
    let mut leaders: Vec<(usize, ProcessPlan)> = Vec::new();
    let mut mates: HashSet<(i32, u64)> = HashSet::new();
    for (at, mut plan) in processes.iter().cloned() {
        plan.reach = reach_for(&plan, &fresh, env.signals.as_ref(), &guard, &confirmed);
        if let Reach::Group(members) = &plan.reach {
            mates.extend(members.iter().skip(1).copied());
        }
        leaders.push((at, plan));
    }

    let mut work: FuturesUnordered<BoxFuture<'_, Finished>> = FuturesUnordered::new();
    for (at, plan) in leaders {
        if mates.contains(&(plan.pid, plan.start_us)) {
            continue;
        }
        let followers: Vec<(usize, i32, u64)> = match &plan.reach {
            Reach::Group(members) => members
                .iter()
                .skip(1)
                .filter_map(|member| {
                    processes
                        .iter()
                        .find(|(_, other)| (other.pid, other.start_us) == *member)
                        .map(|(other_at, other)| (*other_at, other.pid, other.start_us))
                })
                .collect(),
            Reach::Alone => Vec::new(),
        };
        work.push(Box::pin(async move {
            let mut answered = Vec::new();
            if plan.pid <= 1 {
                answered.push((at, (Outcome::Protected, Some(reason::PROCESS_PROTECTED))));
                return Finished { answered };
            }
            let led = stop_process(plan, env.source.as_ref(), env.signals.as_ref()).await;
            answered.push((at, led));
            for (mate_at, pid, start_us) in followers {
                answered.push((mate_at, member_outcome(pid, start_us, env.source.as_ref())));
            }
            Finished { answered }
        }));
    }
    let mut by_id: HashMap<String, Vec<usize>> = HashMap::new();
    let mut order: Vec<String> = Vec::new();
    for (at, plan) in containers {
        by_id.entry(plan.id.clone()).or_default().push(at);
        if !order.contains(&plan.id) {
            order.push(plan.id);
        }
    }
    for id in order {
        let slots_for = by_id.remove(&id).unwrap_or_default();
        work.push(Box::pin(async move {
            let got = stop_container(env.containers.as_ref(), &id).await;
            Finished {
                answered: slots_for.into_iter().map(|at| (at, got)).collect(),
            }
        }));
    }

    while let Some(finished) = work.next().await {
        for (at, got) in finished.answered {
            slots[at] = Slot::Done(got);
        }
    }
}

async fn stop_container(containers: &dyn Containers, id: &str) -> Answer {
    match containers.stop(id).await {
        Ok(Stopped::Stopped) => (Outcome::Stopped, None),
        Ok(Stopped::AlreadyGone) => (Outcome::AlreadyGone, None),
        Err(err) => {
            tracing::warn!(error = %err, "a container stop failed");
            let why = match err {
                DockerError::Absent | DockerError::Refused | DockerError::Timeout => {
                    reason::DOCKER_UNREACHABLE
                }
                _ => reason::DOCKER_REFUSED,
            };
            (Outcome::Failed, Some(why))
        }
    }
}

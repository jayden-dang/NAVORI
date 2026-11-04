use std::net::IpAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use navori_kernel::{ActionError, ModuleCtx, Request, Wake};
use navori_macos::ProcessSource;
use tokio::time::Instant;

use crate::action::{KillReply, Outcome, PortsAction, TargetResult};
use crate::docker::attribute;
use crate::infra::docker::Containers;
use crate::place::place;
use crate::scan::{HarnessMemo, Scan, Scanner};
use crate::state::{DockerStatus, PortsState, group};
use crate::stop::{StopEnv, StopTask, Stops, reason};
use crate::{Ports, PortsEnv};

const FIRST_SCAN: Duration = Duration::from_secs(5);
const OPEN_PROBLEM: &str = "Ports could not read the process list.";
const SCAN_FAILED: &str = "a port scan could not read the process list";
const OPEN_FAILED: &str = "the process source could not be opened";
const RECOVERED: &str = "port scans work again";
const DOCKER_FAILED: &str = "Docker could not be read";
const DOCKER_RECOVERED: &str = "Docker reads work again";
const STOPPED: &str = "ports.stopped";
const OPEN_CADENCE: Duration = Duration::from_secs(3);
const CLOSED_CADENCE: Duration = Duration::from_secs(30);

struct Finished {
    memo: HarnessMemo,
    result: Result<(Scan, DockerStatus), String>,
}

struct InFlight {
    started: SystemTime,
    task: tokio::task::JoinHandle<Result<Finished, tokio::task::JoinError>>,
}

impl Drop for InFlight {
    fn drop(&mut self) {
        self.task.abort();
    }
}

fn note(failing: &mut bool, failure: Option<&'static str>, recovered: &'static str) {
    match (failure, *failing) {
        (Some(text), false) => tracing::warn!("{text}"),
        (None, true) => tracing::info!("{recovered}"),
        _ => {}
    }
    *failing = failure.is_some();
}

async fn finished(in_flight: &mut Option<InFlight>) -> anyhow::Result<(SystemTime, Finished)> {
    match in_flight {
        Some(flight) => {
            let outcome = (&mut flight.task).await??;
            Ok((flight.started, outcome))
        }
        None => std::future::pending().await,
    }
}

fn cadence(panel_visible: bool) -> Duration {
    if panel_visible {
        OPEN_CADENCE
    } else {
        CLOSED_CADENCE
    }
}

fn publish(
    ctx: &ModuleCtx<Ports>,
    started: SystemTime,
    result: Result<(Scan, DockerStatus), String>,
) {
    ctx.update_state(|state| match result {
        Ok((scan, docker)) => {
            state.scanned_at_ms = Some(millis(started));
            state.docker = docker;
            state.groups = group(scan.entries);
            state.skipped = scan.skipped;
            state.problem = None;
        }
        Err(problem) => state.problem = Some(problem),
    });
}

impl Ports {
    pub fn new(env: PortsEnv) -> Self {
        Self {
            env,
            source: None,
            memo: HarnessMemo::new(),
        }
    }

    pub(crate) async fn run_loop(&mut self, ctx: &mut ModuleCtx<Self>) -> anyhow::Result<()> {
        ctx.update_state(|state| *state = PortsState::default());
        let mut wait = ctx.wait_handle();
        let mut deadline = Some(wait.now() + FIRST_SCAN);
        let mut in_flight: Option<InFlight> = None;
        let mut failing = false;
        let mut docker_failing = false;
        let mut stops = Stops::default();
        let mut rescan_after = false;
        loop {
            tokio::select! {
                biased;
                done = finished(&mut in_flight) => {
                    in_flight = None;
                    let (started, outcome) = done?;
                    self.memo = outcome.memo;
                    note(
                        &mut failing,
                        outcome.result.as_ref().err().map(|_| SCAN_FAILED),
                        RECOVERED,
                    );
                    if let Ok((_, docker)) = &outcome.result {
                        match docker {
                            DockerStatus::Unreadable(_) => {
                                note(&mut docker_failing, Some(DOCKER_FAILED), DOCKER_RECOVERED);
                            }
                            DockerStatus::Read => {
                                note(&mut docker_failing, None, DOCKER_RECOVERED);
                            }
                            DockerStatus::NotSeen => {}
                        }
                    }
                    publish(ctx, started, outcome.result);
                    let now = wait.now();
                    deadline = Some(now + cadence(wait.panel_visible()));
                    if std::mem::take(&mut rescan_after) {
                        match self.begin_scan(ctx, now, &mut failing, wait.panel_visible()) {
                            Ok(task) => {
                                in_flight = Some(task);
                                deadline = None;
                            }
                            Err(next) => deadline = Some(next),
                        }
                    }
                }
                Some(joined) = stops.next(), if !stops.is_empty() => {
                    match joined {
                        Ok(done) => {
                            ctx.emit(STOPPED, done.severity(), done.summary(), done.payload());
                        }
                        Err(err) => tracing::warn!(error = %err, "a stop ended before it could answer"),
                    }
                    if in_flight.is_some() {
                        rescan_after = true;
                    } else {
                        match self.begin_scan(ctx, wait.now(), &mut failing, wait.panel_visible()) {
                            Ok(task) => {
                                in_flight = Some(task);
                                deadline = None;
                            }
                            Err(next) => deadline = Some(next),
                        }
                    }
                }
                req = ctx.recv() => {
                    let Some(req) = req else {
                        return Ok(());
                    };
                    self.on_request(ctx, req, &mut stops);
                }
                wake = wait.wait_until(deadline) => {
                    let now = wait.now();
                    match wake {
                        Wake::Due => {
                            deadline = None;
                            match self.begin_scan(ctx, now, &mut failing, wait.panel_visible()) {
                                Ok(task) => in_flight = Some(task),
                                Err(next) => deadline = Some(next),
                            }
                        }
                        Wake::PanelChanged => {
                            if wait.panel_visible() && in_flight.is_none() {
                                let soonest = now + OPEN_CADENCE;
                                deadline = Some(deadline.map_or(soonest, |due| due.min(soonest)));
                            }
                        }
                    }
                }
            }
        }
    }

    fn begin_scan(
        &mut self,
        ctx: &ModuleCtx<Self>,
        now: SystemTime,
        failing: &mut bool,
        panel_visible: bool,
    ) -> Result<InFlight, SystemTime> {
        self.start_scan(now).map_err(|problem| {
            note(failing, Some(OPEN_FAILED), RECOVERED);
            publish(ctx, now, Err(problem));
            now + cadence(panel_visible)
        })
    }

    fn on_request(&mut self, ctx: &ModuleCtx<Self>, req: Request<PortsAction>, stops: &mut Stops) {
        if let Err(message) = req.action.validate() {
            req.respond::<()>(Err(ActionError::Invalid(message)));
            return;
        }
        match req.action {
            PortsAction::List => {
                let state = ctx.state();
                req.respond(Ok(state));
            }
            PortsAction::Open { port, address } => {
                let reply = open_url(&ctx.state(), port, address);
                req.respond(reply);
            }
            PortsAction::Kill {
                ref selector,
                ref targets,
            } => {
                let (selector, targets) = (selector.clone(), targets.clone());
                let arrival = Instant::now();
                let Ok(source) = self.open() else {
                    let results = targets
                        .into_iter()
                        .map(|target| TargetResult {
                            target,
                            result: Outcome::Failed,
                            reason: Some(reason::PROCESS_LIST_TIMEOUT.to_owned()),
                        })
                        .collect();
                    req.respond(Ok(KillReply { results }));
                    return;
                };
                let env = StopEnv {
                    source,
                    home: self.env.home.clone(),
                    containers: Arc::clone(&self.env.containers),
                    signals: Arc::clone(&self.env.signals),
                    helper: self.env.helper.clone(),
                };
                stops.spawn(StopTask::new(req, env, selector, targets, arrival));
            }
        }
    }

    fn start_scan(&mut self, started: SystemTime) -> Result<InFlight, String> {
        let source = self.open()?;
        let scanner = Scanner::new(source, self.env.home.clone());
        let memo = std::mem::take(&mut self.memo);
        let containers = Arc::clone(&self.env.containers);
        let home = self.env.home.clone();
        let task = tokio::spawn(async move {
            match tokio::task::spawn_blocking(move || scanner.scan(memo)).await {
                Ok((Ok(scan), memo)) => Ok(Finished {
                    memo,
                    result: Ok(with_docker(containers, home, scan).await),
                }),
                Ok((Err(err), memo)) => Ok(Finished {
                    memo,
                    result: Err(err.to_string()),
                }),
                Err(err) => Err(err),
            }
        });
        Ok(InFlight { started, task })
    }

    fn open(&mut self) -> Result<Arc<dyn ProcessSource>, String> {
        if let Some(source) = &self.source {
            return Ok(Arc::clone(source));
        }
        match (self.env.processes)() {
            Ok(source) => Ok(Arc::clone(self.source.insert(source))),
            Err(_) => Err(OPEN_PROBLEM.to_owned()),
        }
    }
}

pub(crate) async fn with_docker(
    containers: Arc<dyn Containers>,
    home: PathBuf,
    mut scan: Scan,
) -> (Scan, DockerStatus) {
    if scan.backend.is_empty() {
        return (scan, DockerStatus::NotSeen);
    }
    let (running, status) = match containers.running().await {
        Ok(containers) => (containers, DockerStatus::Read),
        Err(err) => (Vec::new(), DockerStatus::Unreadable(err.to_string())),
    };
    let docker = attribute(&scan.backend, &running, |path| place(path, &home));
    scan.entries.extend(docker);
    (scan, status)
}

fn open_url(
    state: &PortsState,
    port: u16,
    address: Option<IpAddr>,
) -> Result<serde_json::Value, ActionError> {
    let mut bound: Vec<IpAddr> = state
        .entries()
        .filter(|entry| entry.port == port)
        .flat_map(|entry| entry.addresses.iter().copied())
        .collect();
    bound.sort();
    bound.dedup();
    if bound.is_empty() {
        return Err(ActionError::Invalid(format!(
            "port {port} is not in the last scan"
        )));
    }
    let chosen: Vec<IpAddr> = match address {
        Some(wanted) => bound.iter().copied().filter(|a| *a == wanted).collect(),
        None => bound.clone(),
    };
    let mut urls: Vec<String> = chosen.iter().map(|a| url(*a, port)).collect();
    urls.dedup();
    if let [only] = urls.as_slice() {
        return Ok(serde_json::json!({ "url": only }));
    }
    let listed: Vec<String> = bound.iter().map(IpAddr::to_string).collect();
    let listed = listed.join(", ");
    Err(ActionError::Invalid(match address {
        Some(wanted) => format!("port {port} is not bound to {wanted}; it is bound to {listed}"),
        None => format!("port {port} is bound to {listed}; name one with address"),
    }))
}

fn url(address: IpAddr, port: u16) -> String {
    if address.is_loopback() || address.is_unspecified() {
        return format!("http://localhost:{port}");
    }
    match address {
        IpAddr::V4(v4) => format!("http://{v4}:{port}"),
        IpAddr::V6(v6) => format!("http://[{v6}]:{port}"),
    }
}

fn millis(time: SystemTime) -> u64 {
    time.duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|elapsed| u64::try_from(elapsed.as_millis()).ok())
        .unwrap_or(0)
}

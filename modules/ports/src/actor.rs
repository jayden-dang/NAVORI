use std::net::IpAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use navori_kernel::{ActionError, ModuleCtx, Request, Wake};
use navori_macos::ProcessSource;

use crate::action::PortsAction;
use crate::docker::attribute;
use crate::infra::docker::Containers;
use crate::place::place;
use crate::scan::{HarnessMemo, Scan, Scanner};
use crate::state::{DockerStatus, PortsState, group};
use crate::{Ports, PortsEnv};

const FIRST_SCAN: Duration = Duration::from_secs(5);
const OPEN_PROBLEM: &str = "Ports could not read the process list.";
const SCAN_PROBLEM: &str = "A port scan stopped before it finished.";

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
        loop {
            tokio::select! {
                biased;
                req = ctx.recv() => {
                    let Some(req) = req else {
                        return Ok(());
                    };
                    Self::on_request(ctx, req);
                }
                wake = wait.wait_until(deadline) => {
                    if wake == Wake::Due {
                        deadline = None;
                        self.scan(ctx, wait.now()).await;
                    }
                }
            }
        }
    }

    fn on_request(ctx: &ModuleCtx<Self>, req: Request<PortsAction>) {
        match req.action {
            PortsAction::List => {
                let state = ctx.state();
                req.respond(Ok(state));
            }
            PortsAction::Open { port, address } => {
                let reply = open_url(&ctx.state(), port, address);
                req.respond(reply);
            }
        }
    }

    async fn scan(&mut self, ctx: &ModuleCtx<Self>, now: SystemTime) {
        let outcome = match self.open() {
            Ok(source) => {
                let scanner = Scanner::new(source, self.env.home.clone());
                let memo = std::mem::take(&mut self.memo);
                match tokio::task::spawn_blocking(move || scanner.scan(memo)).await {
                    Ok((Ok(scan), memo)) => {
                        self.memo = memo;
                        Ok(with_docker(
                            Arc::clone(&self.env.containers),
                            self.env.home.clone(),
                            scan,
                        )
                        .await)
                    }
                    Ok((Err(err), memo)) => {
                        self.memo = memo;
                        tracing::warn!(error = %err, "a port scan could not read the process list");
                        Err(err.to_string())
                    }
                    Err(_) => {
                        tracing::warn!("a port scan stopped before it finished");
                        Err(SCAN_PROBLEM.to_owned())
                    }
                }
            }
            Err(problem) => Err(problem),
        };
        ctx.update_state(|state| match outcome {
            Ok((scan, docker)) => {
                state.scanned_at_ms = Some(millis(now));
                state.docker = docker;
                state.groups = group(scan.entries);
                state.skipped = scan.skipped;
                state.problem = None;
            }
            Err(problem) => state.problem = Some(problem),
        });
    }

    fn open(&mut self) -> Result<Arc<dyn ProcessSource>, String> {
        if let Some(source) = &self.source {
            return Ok(Arc::clone(source));
        }
        match (self.env.processes)() {
            Ok(source) => Ok(Arc::clone(self.source.insert(source))),
            Err(err) => {
                tracing::warn!(error = %err, "the process source could not be opened");
                Err(OPEN_PROBLEM.to_owned())
            }
        }
    }
}

async fn with_docker(
    containers: Arc<dyn Containers>,
    home: PathBuf,
    mut scan: Scan,
) -> (Scan, DockerStatus) {
    if scan.backend.is_empty() {
        return (scan, DockerStatus::NotSeen);
    }
    let (running, status) = match containers.running().await {
        Ok(containers) => (containers, DockerStatus::Read),
        Err(err) => {
            tracing::warn!(error = %err, "Docker could not be read");
            (Vec::new(), DockerStatus::Unreadable(err.to_string()))
        }
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
        .groups
        .iter()
        .flat_map(|project| &project.worktrees)
        .flat_map(|worktree| &worktree.processes)
        .flat_map(|process| &process.entries)
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

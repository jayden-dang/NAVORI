use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use navori_kernel::{ModuleCtx, Request, Wake};
use navori_macos::ProcessSource;

use crate::action::PortsAction;
use crate::scan::Scanner;
use crate::state::{PortsState, group};
use crate::{Ports, PortsEnv};

const FIRST_SCAN: Duration = Duration::from_secs(5);
const OPEN_PROBLEM: &str = "Ports could not read the process list.";
const SCAN_PROBLEM: &str = "A port scan stopped before it finished.";

impl Ports {
    pub fn new(env: PortsEnv) -> Self {
        Self { env, source: None }
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
        }
    }

    async fn scan(&mut self, ctx: &ModuleCtx<Self>, now: SystemTime) {
        let outcome = match self.open() {
            Ok(source) => {
                let scanner = Scanner::new(source, self.env.home.clone());
                match tokio::task::spawn_blocking(move || scanner.scan()).await {
                    Ok(Ok(scan)) => Ok(scan),
                    Ok(Err(err)) => {
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
            Ok(scan) => {
                state.scanned_at_ms = Some(millis(now));
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

fn millis(time: SystemTime) -> u64 {
    time.duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|elapsed| u64::try_from(elapsed.as_millis()).ok())
        .unwrap_or(0)
}

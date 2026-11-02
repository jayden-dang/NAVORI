//! Built-in `system` module: liveness and journal retention.

use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use navori_protocol::Risk;
use serde::{Deserialize, Serialize};

use crate::module::{ActionDoc, ActionError, Module, ModuleAction, ModuleCtx, NoConfig};
use crate::scheduler::Wake;

const DAY_MS: i64 = 24 * 60 * 60 * 1000;
const RETENTION_SWEEP: Duration = Duration::from_secs(6 * 60 * 60);

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SystemAction {
    /// Liveness check. Answers "pong".
    Ping,
    /// Delete journal events older than `older_than_days` days. Returns how many were removed.
    PruneJournal { older_than_days: u32 },
}

impl ModuleAction for SystemAction {
    fn risk(&self) -> Risk {
        match self {
            Self::Ping => Risk::Read,
            Self::PruneJournal { .. } => Risk::Write,
        }
    }

    fn catalog() -> Vec<ActionDoc> {
        vec![
            ActionDoc {
                name: "ping",
                description: "Liveness check; answers \"pong\".",
                risk: Risk::Read,
            },
            ActionDoc {
                name: "prune_journal",
                description: "Delete journal events older than `older_than_days` days.",
                risk: Risk::Write,
            },
        ]
    }
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct SystemState {
    pub last_prune_removed: Option<usize>,
    pub sweep_paused: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SweepPolicy {
    KeepDays(u32),
    Paused,
}

/// Journal retention runs at start and every six hours, unless the sweep is paused.
#[derive(Debug, Clone)]
pub struct System {
    /// Shared with the kernel builder so it can be configured after registration.
    pub(crate) sweep_policy: Arc<Mutex<SweepPolicy>>,
}

impl Default for System {
    fn default() -> Self {
        Self {
            sweep_policy: Arc::new(Mutex::new(SweepPolicy::KeepDays(30))),
        }
    }
}

impl System {
    fn current_policy(&self) -> SweepPolicy {
        *self
            .sweep_policy
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    fn prune(ctx: &ModuleCtx<Self>, days: u32) -> Result<usize, ActionError> {
        let cutoff = crate::bus::now_ms() - i64::from(days) * DAY_MS;
        let removed = ctx
            .store
            .journal_prune(cutoff)
            .map_err(|e| ActionError::Failed(e.to_string()))?;
        ctx.update_state(|s| s.last_prune_removed = Some(removed));
        Ok(removed)
    }
}

impl Module for System {
    const ID: &'static str = "system";
    type Action = SystemAction;
    type Config = NoConfig;
    type State = SystemState;

    fn initial_state(&self) -> SystemState {
        SystemState {
            last_prune_removed: None,
            sweep_paused: self.current_policy() == SweepPolicy::Paused,
        }
    }

    async fn run(&mut self, ctx: &mut ModuleCtx<Self>) -> anyhow::Result<()> {
        let mut wait = ctx.wait_handle();
        let mut next_sweep = match self.current_policy() {
            SweepPolicy::KeepDays(_) => Some(wait.now()),
            SweepPolicy::Paused => None,
        };
        loop {
            tokio::select! {
                req = ctx.recv() => {
                    let Some(req) = req else { return Ok(()) };
                    match req.action {
                        SystemAction::Ping => req.respond(Ok("pong")),
                        SystemAction::PruneJournal { older_than_days } => {
                            let result = Self::prune(ctx, older_than_days);
                            req.respond(result);
                        }
                    }
                }
                wake = wait.wait_until(next_sweep) => {
                    let Wake::Due = wake else {
                        continue;
                    };
                    if let SweepPolicy::KeepDays(days) = self.current_policy() {
                        if let Err(e) = Self::prune(ctx, days) {
                            tracing::warn!(error = %e, "journal retention sweep failed");
                        }
                        next_sweep = Some(wait.now() + RETENTION_SWEEP);
                    }
                }
            }
        }
    }
}

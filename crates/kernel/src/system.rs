//! Built-in `system` module: liveness and journal retention.

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use navori_protocol::Risk;
use serde::{Deserialize, Serialize};

use crate::module::{ActionDoc, ActionError, Module, ModuleAction, ModuleCtx, NoConfig};

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
}

/// Journal retention is applied at start and then every few hours.
#[derive(Debug, Clone)]
pub struct System {
    /// Shared with the kernel builder so it can be configured after registration.
    pub(crate) retention_days: Arc<AtomicU32>,
}

impl Default for System {
    fn default() -> Self {
        Self {
            retention_days: Arc::new(AtomicU32::new(30)),
        }
    }
}

impl System {
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
        SystemState::default()
    }

    async fn run(&mut self, ctx: &mut ModuleCtx<Self>) -> anyhow::Result<()> {
        let mut sweep = tokio::time::interval(RETENTION_SWEEP);
        sweep.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
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
                _ = sweep.tick() => {
                    if let Err(e) = Self::prune(ctx, self.retention_days.load(Ordering::Relaxed)) {
                        tracing::warn!(error = %e, "journal retention sweep failed");
                    }
                }
            }
        }
    }
}

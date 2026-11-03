//! Reports and manages the hooks Navori keeps in each Harness's config.

mod action;
mod domain;
mod event;
mod infra;
mod state;

pub use action::{Harness, IntegrationsAction};
pub use domain::{Classification, classify};
pub use event::{HOOK_FAILED, HOOK_INSTALLED, HOOK_REMOVED, HOOK_REPAIRED};
pub use state::{HarnessStatus, IntegrationsState, Offer, State};

use std::path::PathBuf;

use navori_kernel::{ActionError, Module, ModuleCtx, NoConfig};

/// Where the Harness config lives and which Helper the hooks must point at.
#[derive(Debug, Clone)]
pub struct IntegrationsEnv {
    pub home: PathBuf,
    pub dev: bool,
    pub helper: PathBuf,
}

/// Reads the three Hook files and says what each one holds. Touches nothing.
pub fn inspect(env: &IntegrationsEnv) -> IntegrationsState {
    IntegrationsState {
        harnesses: Harness::ALL
            .into_iter()
            .map(|harness| state::row(env, harness))
            .collect(),
    }
}

#[derive(Debug)]
pub struct Integrations {
    env: IntegrationsEnv,
}

impl Integrations {
    pub fn new(env: IntegrationsEnv) -> Self {
        Self { env }
    }
}

impl Module for Integrations {
    const ID: &'static str = "integrations";
    type Action = IntegrationsAction;
    type Config = NoConfig;
    type State = IntegrationsState;

    fn initial_state(&self) -> IntegrationsState {
        IntegrationsState::default()
    }

    async fn run(&mut self, ctx: &mut ModuleCtx<Self>) -> anyhow::Result<()> {
        while let Some(req) = ctx.recv().await {
            match &req.action {
                IntegrationsAction::Status => {
                    let env = self.env.clone();
                    let scan = tokio::task::spawn_blocking(move || inspect(&env)).await;
                    req.respond(scan.map_err(|e| {
                        ActionError::Failed(format!("the hook scan did not finish: {e}"))
                    }));
                }
                IntegrationsAction::Install { .. }
                | IntegrationsAction::Repair { .. }
                | IntegrationsAction::Remove { .. } => {
                    req.respond::<IntegrationsState>(Err(ActionError::Invalid(
                        "this action is not available yet".to_owned(),
                    )));
                }
            }
        }
        Ok(())
    }
}

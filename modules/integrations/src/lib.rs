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

use infra::files::Plan;

use navori_kernel::{ActionError, Module, ModuleCtx, NoConfig, Severity};
use serde_json::json;

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

impl Integrations {
    async fn change(
        &self,
        ctx: &ModuleCtx<Self>,
        harness: Harness,
        plan: Plan,
    ) -> Result<IntegrationsState, ActionError> {
        let words = Words::of(plan);
        let env = self.env.clone();
        let payload = json!({ "harness": harness });
        let done = tokio::task::spawn_blocking(move || {
            let outcome = infra::files::apply(&env, harness, plan);
            (outcome, inspect(&env))
        })
        .await;
        let done = match done {
            Ok(done) => done,
            Err(error) => {
                tracing::warn!(?harness, %error, "the {} did not finish", words.verb);
                ctx.emit(
                    HOOK_FAILED,
                    Severity::Error,
                    event::failed(harness, words.verb),
                    payload,
                );
                return Err(ActionError::Failed(format!(
                    "the {} did not finish: {error}",
                    words.verb
                )));
            }
        };
        match done {
            (Ok(outcome), state) => {
                let summary = match outcome {
                    infra::files::Outcome::Written => (words.done)(harness),
                    infra::files::Outcome::Nothing => (words.already)(harness),
                };
                ctx.emit(words.kind, Severity::Info, summary, payload);
                Ok(state)
            }
            (Err(error), _) => {
                tracing::warn!(?harness, %error, "{} the hooks failed", words.verb);
                ctx.emit(
                    HOOK_FAILED,
                    Severity::Error,
                    event::failed(harness, words.verb),
                    payload,
                );
                Err(ActionError::Failed(error.to_string()))
            }
        }
    }
}

/// What differs between the actions that share one change path.
struct Words {
    verb: &'static str,
    kind: &'static str,
    done: fn(Harness) -> String,
    already: fn(Harness) -> String,
}

impl Words {
    fn of(plan: Plan) -> Self {
        match plan {
            Plan::Install => Self {
                verb: "install",
                kind: HOOK_INSTALLED,
                done: event::installed,
                already: event::already_installed,
            },
            Plan::Repair => Self {
                verb: "repair",
                kind: HOOK_REPAIRED,
                done: event::repaired,
                already: event::already_correct,
            },
        }
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
                IntegrationsAction::Install { harness } => {
                    let harness = *harness;
                    let result = self.change(ctx, harness, Plan::Install).await;
                    req.respond(result);
                }
                IntegrationsAction::Repair { harness } => {
                    let harness = *harness;
                    let result = self.change(ctx, harness, Plan::Repair).await;
                    req.respond(result);
                }
                IntegrationsAction::Remove { .. } => {
                    req.respond::<IntegrationsState>(Err(ActionError::Invalid(
                        "this action is not available yet".to_owned(),
                    )));
                }
            }
        }
        Ok(())
    }
}

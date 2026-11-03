//! Reports and manages the hooks Navori keeps in each Harness's config.

mod action;
mod domain;
mod event;
mod infra;
mod state;

pub use action::{Harness, IntegrationsAction};
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
        let message = match done {
            Ok((Ok(outcome), state)) => {
                let summary = match outcome {
                    infra::files::Outcome::Written => (words.done)(harness),
                    infra::files::Outcome::Nothing => (words.already)(harness),
                };
                ctx.emit(words.kind, Severity::Info, summary, payload);
                return Ok(state);
            }
            Ok((Err(error), _)) => error.to_string(),
            Err(error) => format!("the {} did not finish: {error}", words.verb),
        };
        tracing::warn!(?harness, %message, "could not {} the hooks", words.verb);
        ctx.emit(
            HOOK_FAILED,
            Severity::Error,
            event::failed(harness, words.verb),
            payload,
        );
        Err(ActionError::Failed(message))
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
            Plan::Remove => Self {
                verb: "remove",
                kind: HOOK_REMOVED,
                done: event::removed,
                already: event::nothing_to_remove,
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
            let (harness, plan) = match &req.action {
                IntegrationsAction::Status => {
                    let env = self.env.clone();
                    let scan = tokio::task::spawn_blocking(move || inspect(&env)).await;
                    req.respond(scan.map_err(|e| {
                        ActionError::Failed(format!("the hook scan did not finish: {e}"))
                    }));
                    continue;
                }
                IntegrationsAction::Install { harness } => (*harness, Plan::Install),
                IntegrationsAction::Repair { harness } => (*harness, Plan::Repair),
                IntegrationsAction::Remove { harness } => (*harness, Plan::Remove),
            };
            let result = self.change(ctx, harness, plan).await;
            req.respond(result);
        }
        Ok(())
    }
}

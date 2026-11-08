//! Decisions and Updates that need the user.

mod action;
mod raise;
mod state;
pub mod stored;

pub use action::{Choice, NeedsYouAction};
pub use state::{ChoiceState, DecisionState, NeedsYouState, OpenDecision, UnviewedUpdate};

use navori_config::JournalSweep;
use navori_kernel::{ActionError, Module, ModuleCtx, NoConfig, Request};

#[derive(Debug)]
pub struct NeedsYou {
    retention: JournalSweep,
}

impl NeedsYou {
    pub fn new(retention: JournalSweep) -> Self {
        Self { retention }
    }
}

impl Module for NeedsYou {
    const ID: &'static str = "needs_you";
    type Action = NeedsYouAction;
    type Config = NoConfig;
    type State = NeedsYouState;

    fn initial_state(&self) -> NeedsYouState {
        NeedsYouState::default()
    }

    async fn run(&mut self, ctx: &mut ModuleCtx<Self>) -> anyhow::Result<()> {
        let _ = self.retention;
        stored::migrate(&ctx.store)?;
        reload_state(ctx)?;
        while let Some(req) = ctx.recv().await {
            handle(ctx, req);
        }
        Ok(())
    }
}

fn reload_state(ctx: &ModuleCtx<NeedsYou>) -> anyhow::Result<()> {
    let decisions = stored::load_open_decisions(&ctx.store)
        .map_err(|err| anyhow::anyhow!("needs_you reload failed: {err}"))?;
    ctx.update_state(|state| {
        state.decisions = decisions;
        state.updates = Vec::new();
    });
    Ok(())
}

fn handle(ctx: &ModuleCtx<NeedsYou>, req: Request<NeedsYouAction>) {
    let result = match &req.action {
        NeedsYouAction::Raise {
            key,
            title,
            reason,
            expires_at_ms,
            primary,
            secondary,
        } => raise::raise(
            ctx,
            key.clone(),
            title.clone(),
            reason.clone(),
            *expires_at_ms,
            primary.clone(),
            secondary.clone(),
        ),
    };
    match result {
        Ok(reply) => {
            if let Err(err) = reload_state(ctx) {
                req.respond::<raise::RaiseReply>(Err(ActionError::Failed(err.to_string())));
                return;
            }
            req.respond(Ok(reply));
        }
        Err(err) => req.respond::<raise::RaiseReply>(Err(err)),
    }
}

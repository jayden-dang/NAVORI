//! Decisions and Updates that need the user.

mod action;
mod answer;
mod expiry;
mod raise;
mod state;
pub mod stored;

pub use action::{Choice, NeedsYouAction, Pick};
pub use state::{ChoiceState, DecisionState, NeedsYouState, OpenDecision, UnviewedUpdate};

use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use futures::stream::{FuturesUnordered, StreamExt};
use navori_config::JournalSweep;
use navori_kernel::{
    ActionError, Module, ModuleCtx, NoConfig, Pending, Request, TrayContribution, Wake,
};

use crate::answer::AnswerOutcome;

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

    fn tray(state: &NeedsYouState) -> TrayContribution {
        let n = state.decisions.len();
        if n == 0 {
            return TrayContribution::default();
        }
        TrayContribution {
            attention: Some(format!("{n} decisions waiting")),
            ..TrayContribution::default()
        }
    }

    async fn run(&mut self, ctx: &mut ModuleCtx<Self>) -> anyhow::Result<()> {
        let _ = self.retention;
        stored::migrate(&ctx.store)?;
        answer::fail_answering_on_restart(ctx)
            .map_err(|err| anyhow::anyhow!("needs_you restart repair failed: {err}"))?;
        answer::refresh_choice_risks(ctx)
            .map_err(|err| anyhow::anyhow!("needs_you risk refresh failed: {err}"))?;
        expiry::expire_due(ctx)
            .map_err(|err| anyhow::anyhow!("needs_you expiry at start failed: {err}"))?;
        reload_state(ctx)?;

        let mut wait = ctx.wait_handle();
        let mut pending: FuturesUnordered<AnswerFlight> = FuturesUnordered::new();
        loop {
            let deadline = expiry::soonest_deadline(ctx);
            tokio::select! {
                req = ctx.recv() => {
                    let Some(req) = req else {
                        return Ok(());
                    };
                    if let Some(flight) = handle(ctx, req) {
                        pending.push(flight);
                    }
                }
                Some(done) = pending.next(), if !pending.is_empty() => {
                    match done.result {
                        Ok(()) => {
                            if let Err(err) = answer::finish_ok(ctx, done.id) {
                                tracing::warn!(error = %err, id = done.id, "needs_you answer finish failed");
                            }
                        }
                        Err(err) => {
                            if let Err(mark) = answer::finish_err(ctx, done.id, &done.choice, err) {
                                tracing::warn!(error = %mark, id = done.id, "needs_you answer fail mark failed");
                            }
                        }
                    }
                    if let Err(err) = reload_state(ctx) {
                        tracing::warn!(error = %err, "needs_you reload after answer failed");
                    }
                }
                wake = wait.wait_until(deadline) => {
                    if matches!(wake, Wake::Due) {
                        if let Err(err) = expiry::expire_due(ctx) {
                            tracing::warn!(error = %err, "needs_you expiry failed");
                        }
                        if let Err(err) = reload_state(ctx) {
                            tracing::warn!(error = %err, "needs_you reload after expiry failed");
                        }
                    }
                }
            }
        }
    }
}

fn reload_state(ctx: &ModuleCtx<NeedsYou>) -> anyhow::Result<()> {
    let decisions = stored::load_open_decisions(&ctx.store)
        .map_err(|err| anyhow::anyhow!("needs_you reload failed: {err}"))?;
    let updates = stored::load_unviewed_updates(&ctx.store)
        .map_err(|err| anyhow::anyhow!("needs_you reload updates failed: {err}"))?;
    ctx.update_state(|state| {
        state.decisions = decisions;
        state.updates = updates;
    });
    Ok(())
}

fn handle(ctx: &ModuleCtx<NeedsYou>, req: Request<NeedsYouAction>) -> Option<AnswerFlight> {
    match &req.action {
        NeedsYouAction::Raise {
            key,
            title,
            reason,
            expires_at_ms,
            primary,
            secondary,
        } => {
            let result = raise::raise(
                ctx,
                key.clone(),
                title.clone(),
                reason.clone(),
                *expires_at_ms,
                primary.clone(),
                secondary.clone(),
            );
            match result {
                Ok(reply) => {
                    if let Err(err) = reload_state(ctx) {
                        req.respond::<raise::RaiseReply>(Err(ActionError::Failed(err.to_string())));
                        return None;
                    }
                    req.respond(Ok(reply));
                }
                Err(err) => req.respond::<raise::RaiseReply>(Err(err)),
            }
            None
        }
        NeedsYouAction::Answer {
            id,
            revision,
            choice,
        } => {
            let id = *id;
            let revision = *revision;
            let choice = *choice;
            match answer::answer(ctx, id, revision, choice) {
                Ok(AnswerOutcome::Closed) => {
                    if let Err(err) = reload_state(ctx) {
                        req.respond::<()>(Err(ActionError::Failed(err.to_string())));
                        return None;
                    }
                    req.respond(Ok(()));
                    None
                }
                Ok(AnswerOutcome::Dispatch(envelope)) => {
                    if let Err(err) = reload_state(ctx) {
                        req.respond::<()>(Err(ActionError::Failed(err.to_string())));
                        return None;
                    }
                    // Answering is committed; reply before the Envelope runs.
                    req.respond(Ok(()));
                    match ctx.dispatcher().send(envelope) {
                        Ok(wait) => Some(AnswerFlight {
                            id,
                            choice: choice.as_str().to_owned(),
                            wait,
                        }),
                        Err(err) => {
                            if let Err(mark) =
                                answer::finish_send_err(ctx, id, choice.as_str(), err)
                            {
                                tracing::warn!(error = %mark, id, "needs_you send-fail mark failed");
                            }
                            if let Err(err) = reload_state(ctx) {
                                tracing::warn!(error = %err, "needs_you reload after send-fail failed");
                            }
                            None
                        }
                    }
                }
                Err(err) => {
                    // Expired path may have written; refresh State either way.
                    let _ = reload_state(ctx);
                    req.respond::<()>(Err(err));
                    None
                }
            }
        }
    }
}

struct AnswerFlight {
    id: i64,
    choice: String,
    wait: Pending,
}

struct AnswerDone {
    id: i64,
    choice: String,
    result: Result<(), navori_kernel::DispatchError>,
}

impl Future for AnswerFlight {
    type Output = AnswerDone;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        match Pin::new(&mut this.wait).poll(cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(result) => Poll::Ready(AnswerDone {
                id: this.id,
                choice: std::mem::take(&mut this.choice),
                result: result.map(|_| ()),
            }),
        }
    }
}

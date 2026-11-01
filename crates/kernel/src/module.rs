//! The contract every feature module implements.

use std::future::Future;
use std::sync::Arc;

use navori_protocol::{Risk, Severity};
use navori_store::Store;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
use tokio::sync::{mpsc, oneshot, watch};
use tokio_util::sync::CancellationToken;

use crate::bus::EventBus;

/// Documentation for one variant of a module's action enum.
#[derive(Debug, Clone, Copy)]
pub struct ActionDoc {
    /// Externally tagged serde name of the variant (`"add"` for `Add { .. }`).
    pub name: &'static str,
    /// Written for humans *and* LLMs: what it does, when to use it.
    pub description: &'static str,
    pub risk: Risk,
}

/// The action enum of a module. Deserialized from the externally tagged JSON form.
pub trait ModuleAction: DeserializeOwned + schemars::JsonSchema + Send + 'static {
    fn risk(&self) -> Risk;
    /// One entry per variant, in display order.
    fn catalog() -> Vec<ActionDoc>;
}

/// A feature module: an actor owning its state, driven by [`ModuleCtx`].
pub trait Module: Send + Sized + 'static {
    const ID: &'static str;
    type Action: ModuleAction;
    /// Snapshot published to the UI / AI. Must be cheap to clone.
    type State: Serialize + Clone + Send + Sync + 'static;

    fn initial_state(&self) -> Self::State;

    /// The actor loop. Return `Ok(())` when [`ModuleCtx::recv`] yields `None` (shutdown).
    /// Returning an error or panicking makes the supervisor restart the module with the
    /// same `ctx` — queued requests are not lost.
    fn run(&mut self, ctx: &mut ModuleCtx<Self>)
    -> impl Future<Output = anyhow::Result<()>> + Send;
}

/// Error a module returns for one action.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ActionError {
    #[error("invalid input: {0}")]
    Invalid(String),
    #[error("{0}")]
    Failed(String),
}

pub(crate) type Reply = Result<Value, ActionError>;

/// One incoming action plus the channel to answer it. Dropping it unanswered makes the
/// caller see the module as unavailable.
#[derive(Debug)]
pub struct Request<A> {
    pub action: A,
    reply: oneshot::Sender<Reply>,
}

impl<A> Request<A> {
    pub(crate) fn new(action: A) -> (Self, oneshot::Receiver<Reply>) {
        let (reply, rx) = oneshot::channel();
        (Self { action, reply }, rx)
    }

    pub fn respond<T: Serialize>(self, result: Result<T, ActionError>) {
        let value = result.and_then(|v| {
            serde_json::to_value(v)
                .map_err(|e| ActionError::Failed(format!("cannot serialize result: {e}")))
        });
        // The caller may have timed out and gone away; that is not the module's problem.
        let _ = self.reply.send(value);
    }
}

/// Everything a module needs from the kernel.
pub struct ModuleCtx<M: Module> {
    pub(crate) inbox: mpsc::Receiver<Request<M::Action>>,
    pub(crate) state: watch::Sender<M::State>,
    pub(crate) revision: Arc<watch::Sender<u64>>,
    pub bus: EventBus,
    pub store: Store,
    pub shutdown: CancellationToken,
}

impl<M: Module> std::fmt::Debug for ModuleCtx<M> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ModuleCtx")
            .field("module", &M::ID)
            .finish_non_exhaustive()
    }
}

impl<M: Module> ModuleCtx<M> {
    /// Next request, or `None` once the kernel shuts down.
    pub async fn recv(&mut self) -> Option<Request<M::Action>> {
        tokio::select! {
            biased;
            () = self.shutdown.cancelled() => None,
            req = self.inbox.recv() => req,
        }
    }

    pub fn state(&self) -> M::State {
        self.state.borrow().clone()
    }

    pub fn update_state(&self, f: impl FnOnce(&mut M::State)) {
        self.state.send_modify(f);
        self.revision.send_modify(|r| *r += 1);
    }

    pub fn emit(&self, kind: &str, severity: Severity, summary: impl Into<String>, payload: Value) {
        self.bus
            .emit(M::ID, kind, severity, summary.into(), payload);
    }
}

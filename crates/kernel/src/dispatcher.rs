//! Non-blocking Action dispatch for Modules.

use std::collections::HashSet;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, OnceLock};
use std::task::{Context, Poll};
use std::time::Duration;

use navori_protocol::{ActionSpec, Envelope, Risk};
use serde_json::Value;
use tokio::sync::oneshot;

use crate::module::Reply;
use crate::runtime::{DispatchError, SendFn};

pub(crate) struct Route {
    pub(crate) id: &'static str,
    pub(crate) send: SendFn,
    pub(crate) specs: Vec<ActionSpec>,
}

pub(crate) struct Router {
    pub(crate) routes: Vec<Route>,
    pub(crate) grants: HashSet<&'static str>,
    pub(crate) timeout: Duration,
}

impl Router {
    pub(crate) fn get(&self, id: &str) -> Option<&Route> {
        self.routes.iter().find(|r| r.id == id)
    }
}

fn filled(router: &SharedRouter) -> &Router {
    let Some(router) = router.get() else {
        unreachable!("router is filled before modules run");
    };
    router
}

pub(crate) type SharedRouter = Arc<OnceLock<Router>>;

/// Handle a Module uses to send Envelopes without blocking its run loop.
///
/// Await [`Pending`] in a `select!` arm or a spawned task — never inline in `run`.
#[derive(Clone)]
pub struct ModuleDispatcher {
    pub(crate) caller: &'static str,
    pub(crate) router: SharedRouter,
}

impl std::fmt::Debug for ModuleDispatcher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ModuleDispatcher")
            .field("caller", &self.caller)
            .finish_non_exhaustive()
    }
}

impl ModuleDispatcher {
    /// Enqueues `envelope` and returns a future for the reply. Does not await the target.
    pub fn send(&self, envelope: Envelope) -> Result<Pending, DispatchError> {
        let router = filled(&self.router);
        let route = router
            .get(envelope.module.as_str())
            .ok_or_else(|| DispatchError::UnknownModule(envelope.module.clone()))?;

        let action = action_name(&envelope.action).unwrap_or("");
        if let Some(spec) = route.specs.iter().find(|s| s.name == action)
            && spec.risk == Risk::Destructive
            && !router.grants.contains(self.caller)
        {
            return Err(DispatchError::Refused {
                module: envelope.module,
                action: action.to_owned(),
            });
        }

        let module = envelope.module.clone();
        let rx = (route.send)(envelope.action)?;
        Ok(Pending::new(module, router.timeout, rx))
    }

    /// Catalog Risk for `envelope`'s action, or `None` if it is not listed.
    pub fn risk_of(&self, envelope: &Envelope) -> Option<Risk> {
        let router = filled(&self.router);
        let route = router.get(envelope.module.as_str())?;
        let name = action_name(&envelope.action)?;
        route.specs.iter().find(|s| s.name == name).map(|s| s.risk)
    }
}

/// Future for one dispatched Envelope's reply (carries the dispatch timeout).
pub struct Pending {
    inner: Pin<Box<dyn Future<Output = Result<Value, DispatchError>> + Send>>,
}

impl std::fmt::Debug for Pending {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Pending").finish_non_exhaustive()
    }
}

impl Pending {
    fn new(module: String, timeout: Duration, rx: oneshot::Receiver<Reply>) -> Self {
        Self {
            inner: Box::pin(async move {
                match tokio::time::timeout(timeout, rx).await {
                    Err(_) => Err(DispatchError::Timeout(module)),
                    Ok(Err(_)) => Err(DispatchError::Unavailable(module)),
                    Ok(Ok(reply)) => reply.map_err(DispatchError::from),
                }
            }),
        }
    }
}

impl Future for Pending {
    type Output = Result<Value, DispatchError>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.inner.as_mut().poll(cx)
    }
}

fn action_name(action: &Value) -> Option<&str> {
    match action {
        Value::String(name) => Some(name.as_str()),
        Value::Object(map) if map.len() == 1 => map.keys().next().map(String::as_str),
        _ => None,
    }
}

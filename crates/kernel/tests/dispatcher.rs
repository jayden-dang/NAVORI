//! ModuleDispatcher behavior through a Kernel built with test Modules.
#![allow(clippy::unwrap_used)]

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::task::{Context, Poll};
use std::time::Duration;

use futures::stream::{FuturesUnordered, StreamExt};
use navori_kernel::{
    ActionDoc, DispatchError, Envelope, Kernel, Module, ModuleAction, ModuleCtx, NoConfig, Pending,
    Request, Risk,
};
use navori_store::Store;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
enum TargetAction {
    Poke,
    Wipe,
    Stall,
}

impl ModuleAction for TargetAction {
    fn risk(&self) -> Risk {
        match self {
            Self::Poke | Self::Stall => Risk::Write,
            Self::Wipe => Risk::Destructive,
        }
    }

    fn catalog() -> Vec<ActionDoc> {
        vec![
            ActionDoc {
                name: "poke",
                description: "A recoverable write.",
                risk: Risk::Write,
            },
            ActionDoc {
                name: "wipe",
                description: "An irreversible write.",
                risk: Risk::Destructive,
            },
            ActionDoc {
                name: "stall",
                description: "Never replies.",
                risk: Risk::Write,
            },
        ]
    }
}

#[derive(Debug, Clone, Default, Serialize)]
struct TargetState;

struct Target {
    wiped: Arc<AtomicBool>,
}

impl Module for Target {
    const ID: &'static str = "target";
    type Action = TargetAction;
    type Config = NoConfig;
    type State = TargetState;

    fn initial_state(&self) -> TargetState {
        TargetState
    }

    async fn run(&mut self, ctx: &mut ModuleCtx<Self>) -> anyhow::Result<()> {
        let mut stalled = Vec::new();
        while let Some(req) = ctx.recv().await {
            match req.action {
                TargetAction::Poke => req.respond(Ok(json!("poked"))),
                TargetAction::Wipe => {
                    self.wiped.store(true, Ordering::SeqCst);
                    req.respond(Ok(json!("wiped")));
                }
                TargetAction::Stall => stalled.push(req),
            }
        }
        Ok(())
    }
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
enum AskerAction {
    /// `envelope` is the JSON form of [`Envelope`] (Value so the action stays JsonSchema).
    Call {
        envelope: Value,
    },
    /// Like `call`, but answers immediately and stores the outcome on state (for timeout).
    Fire {
        envelope: Value,
    },
    Ping,
    RiskOf {
        envelope: Value,
    },
}

impl ModuleAction for AskerAction {
    fn risk(&self) -> Risk {
        Risk::Read
    }

    fn catalog() -> Vec<ActionDoc> {
        vec![
            ActionDoc {
                name: "call",
                description: "Dispatch an Envelope and report the outcome.",
                risk: Risk::Read,
            },
            ActionDoc {
                name: "fire",
                description: "Dispatch an Envelope; answer now; publish the outcome on state.",
                risk: Risk::Read,
            },
            ActionDoc {
                name: "ping",
                description: "Answer immediately.",
                risk: Risk::Read,
            },
            ActionDoc {
                name: "risk_of",
                description: "Read an Envelope's catalog Risk.",
                risk: Risk::Read,
            },
        ]
    }
}

#[derive(Debug, Clone, Default, Serialize)]
struct AskerState {
    last: Option<Value>,
}

#[derive(Default)]
struct Asker;

impl Module for Asker {
    const ID: &'static str = "asker";
    type Action = AskerAction;
    type Config = NoConfig;
    type State = AskerState;

    fn initial_state(&self) -> AskerState {
        AskerState::default()
    }

    async fn run(&mut self, ctx: &mut ModuleCtx<Self>) -> anyhow::Result<()> {
        let mut pending: FuturesUnordered<InFlight> = FuturesUnordered::new();
        loop {
            tokio::select! {
                req = ctx.recv() => {
                    let Some(req) = req else {
                        return Ok(());
                    };
                    if matches!(req.action, AskerAction::Ping) {
                        req.respond(Ok(json!("pong")));
                        continue;
                    }
                    if let AskerAction::RiskOf { .. } = &req.action {
                        let envelope = match &req.action {
                            AskerAction::RiskOf { envelope } => {
                                serde_json::from_value(envelope.clone()).unwrap()
                            }
                            _ => unreachable!(),
                        };
                        let risk = ctx.dispatcher().risk_of(&envelope);
                        req.respond(Ok(json!(risk)));
                        continue;
                    }
                    let (envelope, reply_mode) = match &req.action {
                        AskerAction::Call { envelope } => (
                            serde_json::from_value(envelope.clone()).unwrap(),
                            ReplyMode::Call,
                        ),
                        AskerAction::Fire { envelope } => (
                            serde_json::from_value(envelope.clone()).unwrap(),
                            ReplyMode::Fire,
                        ),
                        _ => unreachable!(),
                    };
                    match ctx.dispatcher().send(envelope) {
                        Ok(wait) => {
                            if reply_mode == ReplyMode::Fire {
                                req.respond(Ok(json!("fired")));
                                pending.push(InFlight {
                                    req: None,
                                    fire: true,
                                    wait,
                                });
                            } else {
                                pending.push(InFlight {
                                    req: Some(req),
                                    fire: false,
                                    wait,
                                });
                            }
                        }
                        Err(err) => {
                            let body = outcome_err(err);
                            if reply_mode == ReplyMode::Fire {
                                ctx.update_state(|s| s.last = Some(body));
                                req.respond(Ok(json!("fired")));
                            } else {
                                req.respond(Ok(body));
                            }
                        }
                    }
                }
                Some(published) = pending.next(), if !pending.is_empty() => {
                    if let Some(body) = published {
                        ctx.update_state(|s| s.last = Some(body));
                    }
                }
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ReplyMode {
    Call,
    Fire,
}

/// Owns the caller's `Request` until `Pending` resolves. Fire mode returns the body to publish.
struct InFlight {
    req: Option<Request<AskerAction>>,
    fire: bool,
    wait: Pending,
}

impl Future for InFlight {
    type Output = Option<Value>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        match Pin::new(&mut this.wait).poll(cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(result) => {
                let body = match result {
                    Ok(value) => json!({ "ok": value }),
                    Err(err) => outcome_err(err),
                };
                if let Some(req) = this.req.take() {
                    req.respond(Ok(body));
                    Poll::Ready(None)
                } else if this.fire {
                    Poll::Ready(Some(body))
                } else {
                    Poll::Ready(None)
                }
            }
        }
    }
}

fn outcome_err(err: DispatchError) -> Value {
    match err {
        DispatchError::UnknownModule(module) => {
            json!({ "err": "UnknownModule", "module": module })
        }
        DispatchError::Timeout(module) => json!({ "err": "Timeout", "module": module }),
        DispatchError::Unavailable(module) => {
            json!({ "err": "Unavailable", "module": module })
        }
        DispatchError::InvalidAction { module, message } => {
            json!({ "err": "InvalidAction", "module": module, "message": message })
        }
        DispatchError::Refused { module, action } => {
            json!({ "err": "Refused", "module": module, "action": action })
        }
        DispatchError::Action(action) => {
            json!({ "err": "Action", "message": action.to_string() })
        }
    }
}

fn store() -> Store {
    Store::open_in_memory().unwrap()
}

#[tokio::test]
async fn a_module_reaches_another_and_gets_its_reply() {
    let kernel = Kernel::builder(store())
        .register(Asker)
        .register(Target {
            wiped: Arc::new(AtomicBool::new(false)),
        })
        .start();

    let out = kernel
        .dispatch(Envelope {
            module: "asker".into(),
            action: json!({
                "call": {
                    "envelope": {
                        "module": "target",
                        "action": "poke"
                    }
                }
            }),
        })
        .await
        .unwrap();

    assert_eq!(out, json!({ "ok": "poked" }));
    kernel.shutdown().await;
}

fn call(envelope: Value) -> Envelope {
    Envelope {
        module: "asker".into(),
        action: json!({ "call": { "envelope": envelope } }),
    }
}

#[tokio::test]
async fn send_returns_before_the_target_handles_it() {
    let kernel = Kernel::builder(store())
        .dispatch_timeout(Duration::from_secs(5))
        .register(Asker)
        .register(Target {
            wiped: Arc::new(AtomicBool::new(false)),
        })
        .start();

    let call_stall = kernel.dispatch(call(json!({
        "module": "target",
        "action": "stall"
    })));
    tokio::pin!(call_stall);
    tokio::time::sleep(Duration::from_millis(30)).await;

    tokio::select! {
        biased;
        ping = kernel.dispatch(Envelope {
            module: "asker".into(),
            action: json!("ping"),
        }) => {
            assert_eq!(ping.unwrap(), json!("pong"));
        }
        _ = &mut call_stall => {
            panic!("stall call finished before ping — asker blocked on the target");
        }
    }

    kernel.shutdown().await;
}

#[tokio::test]
async fn a_module_can_dispatch_to_itself() {
    let kernel = Kernel::builder(store())
        .register(Asker)
        .register(Target {
            wiped: Arc::new(AtomicBool::new(false)),
        })
        .start();

    let out = kernel
        .dispatch(call(json!({
            "module": "asker",
            "action": "ping"
        })))
        .await
        .unwrap();
    assert_eq!(out, json!({ "ok": "pong" }));
    kernel.shutdown().await;
}

#[tokio::test]
async fn unknown_module_and_timeout_report_dispatch_errors() {
    let kernel = Kernel::builder(store())
        .dispatch_timeout(Duration::from_millis(100))
        .register(Asker)
        .register(Target {
            wiped: Arc::new(AtomicBool::new(false)),
        })
        .start();

    let unknown = kernel
        .dispatch(call(json!({
            "module": "nope",
            "action": "poke"
        })))
        .await
        .unwrap();
    assert_eq!(unknown, json!({ "err": "UnknownModule", "module": "nope" }));

    // Fire answers before Pending resolves so Kernel::dispatch is not racing the same timeout.
    let fired = kernel
        .dispatch(Envelope {
            module: "asker".into(),
            action: json!({
                "fire": {
                    "envelope": {
                        "module": "target",
                        "action": "stall"
                    }
                }
            }),
        })
        .await
        .unwrap();
    assert_eq!(fired, json!("fired"));

    let deadline = tokio::time::Instant::now() + Duration::from_millis(400);
    let timed_out = loop {
        if let Some(Value::Object(map)) = kernel.state("asker").and_then(|s| s.get("last").cloned())
            && map.get("err") == Some(&json!("Timeout"))
        {
            break Value::Object(map);
        }
        if tokio::time::Instant::now() >= deadline {
            panic!(
                "asker did not publish Timeout; state={:?}",
                kernel.state("asker")
            );
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    };
    assert_eq!(timed_out, json!({ "err": "Timeout", "module": "target" }));
    kernel.shutdown().await;
}

#[tokio::test]
async fn a_destructive_action_is_refused_without_a_grant() {
    let wiped = Arc::new(AtomicBool::new(false));
    let kernel = Kernel::builder(store())
        .register(Asker)
        .register(Target {
            wiped: Arc::clone(&wiped),
        })
        .start();

    let out = kernel
        .dispatch(call(json!({
            "module": "target",
            "action": "wipe"
        })))
        .await
        .unwrap();
    assert_eq!(
        out,
        json!({ "err": "Refused", "module": "target", "action": "wipe" })
    );
    assert!(
        !wiped.load(Ordering::SeqCst),
        "wipe must not run when refused"
    );
    kernel.shutdown().await;
}

#[tokio::test]
async fn a_granted_module_runs_a_destructive_action() {
    let wiped = Arc::new(AtomicBool::new(false));
    let kernel = Kernel::builder(store())
        .grant_destructive("asker")
        .register(Asker)
        .register(Target {
            wiped: Arc::clone(&wiped),
        })
        .start();

    let out = kernel
        .dispatch(call(json!({
            "module": "target",
            "action": "wipe"
        })))
        .await
        .unwrap();
    assert_eq!(out, json!({ "ok": "wiped" }));
    assert!(wiped.load(Ordering::SeqCst));
    kernel.shutdown().await;
}

#[tokio::test]
async fn risk_of_reads_the_catalog_for_string_and_object_actions() {
    let kernel = Kernel::builder(store())
        .register(Asker)
        .register(Target {
            wiped: Arc::new(AtomicBool::new(false)),
        })
        .start();

    let as_string = kernel
        .dispatch(Envelope {
            module: "asker".into(),
            action: json!({
                "risk_of": {
                    "envelope": {
                        "module": "target",
                        "action": "poke"
                    }
                }
            }),
        })
        .await
        .unwrap();
    assert_eq!(as_string, json!("write"));

    let as_object = kernel
        .dispatch(Envelope {
            module: "asker".into(),
            action: json!({
                "risk_of": {
                    "envelope": {
                        "module": "target",
                        "action": { "wipe": null }
                    }
                }
            }),
        })
        .await
        .unwrap();
    assert_eq!(as_object, json!("destructive"));

    let missing = kernel
        .dispatch(Envelope {
            module: "asker".into(),
            action: json!({
                "risk_of": {
                    "envelope": {
                        "module": "target",
                        "action": "nope"
                    }
                }
            }),
        })
        .await
        .unwrap();
    assert_eq!(missing, json!(null));
    kernel.shutdown().await;
}

#[tokio::test]
async fn kernel_dispatch_is_unchanged_for_destructive_actions() {
    let wiped = Arc::new(AtomicBool::new(false));
    let kernel = Kernel::builder(store())
        .register(Asker)
        .register(Target {
            wiped: Arc::clone(&wiped),
        })
        .start();

    let out = kernel
        .dispatch(Envelope {
            module: "target".into(),
            action: json!("wipe"),
        })
        .await
        .unwrap();
    assert_eq!(out, json!("wiped"));
    assert!(wiped.load(Ordering::SeqCst));
    kernel.shutdown().await;
}

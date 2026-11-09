#![allow(dead_code, clippy::unwrap_used)]

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use navori_config::JournalSweep;
use navori_kernel::{
    ActionDoc, ActionError, DispatchError, Envelope, Kernel, Module, ModuleAction, ModuleCtx,
    NoConfig, Risk, WallClock,
};
use navori_mod_needs_you::NeedsYou;
use navori_store::Store;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

pub const ORIGIN_MS: u64 = 1_700_000_000_000;

#[derive(Clone, Debug)]
pub struct FakeClock {
    millis: Arc<AtomicU64>,
}

impl FakeClock {
    pub fn at(millis: u64) -> Self {
        Self {
            millis: Arc::new(AtomicU64::new(millis)),
        }
    }

    pub fn millis(&self) -> u64 {
        self.millis.load(Ordering::SeqCst)
    }

    pub fn advance(&self, by: Duration) {
        let millis = u64::try_from(by.as_millis()).unwrap();
        self.millis.fetch_add(millis, Ordering::SeqCst);
    }
}

impl WallClock for FakeClock {
    fn now(&self) -> SystemTime {
        UNIX_EPOCH + Duration::from_millis(self.millis.load(Ordering::SeqCst))
    }
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TargetAction {
    Poke,
    Fail,
    Stall,
    Wipe,
}

impl ModuleAction for TargetAction {
    fn risk(&self) -> Risk {
        match self {
            Self::Wipe => Risk::Destructive,
            Self::Poke | Self::Fail | Self::Stall => Risk::Write,
        }
    }

    fn catalog() -> Vec<ActionDoc> {
        vec![
            ActionDoc {
                name: "poke",
                description: "A recoverable write for Needs you tests.",
                risk: Risk::Write,
            },
            ActionDoc {
                name: "fail",
                description: "Rejects the Action.",
                risk: Risk::Write,
            },
            ActionDoc {
                name: "stall",
                description: "Never replies.",
                risk: Risk::Write,
            },
            ActionDoc {
                name: "wipe",
                description: "A Destructive Action for Needs you tests.",
                risk: Risk::Destructive,
            },
        ]
    }
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct TargetState {
    pub seen: Vec<Value>,
}

#[derive(Default)]
pub struct Target;

impl Module for Target {
    const ID: &'static str = "target";
    type Action = TargetAction;
    type Config = NoConfig;
    type State = TargetState;

    fn initial_state(&self) -> TargetState {
        TargetState::default()
    }

    async fn run(&mut self, ctx: &mut ModuleCtx<Self>) -> anyhow::Result<()> {
        let mut stalled = Vec::new();
        while let Some(req) = ctx.recv().await {
            match req.action {
                TargetAction::Poke => {
                    ctx.update_state(|state| state.seen.push(json!("poke")));
                    req.respond(Ok(json!("poked")));
                }
                TargetAction::Fail => {
                    ctx.update_state(|state| state.seen.push(json!("fail")));
                    req.respond::<Value>(Err(ActionError::Failed("target refused".into())));
                }
                TargetAction::Wipe => {
                    ctx.update_state(|state| state.seen.push(json!("wipe")));
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
#[expect(
    clippy::large_enum_variant,
    reason = "test-only Raise carries labels and an optional Envelope"
)]
pub enum ProducerAction {
    Ping,
    Raise {
        key: String,
        title: String,
        reason: String,
        primary_label: String,
        #[serde(default)]
        #[schemars(with = "Option<Value>")]
        primary_envelope: Option<Envelope>,
        secondary_label: String,
    },
}

impl ModuleAction for ProducerAction {
    fn risk(&self) -> Risk {
        match self {
            Self::Ping => Risk::Read,
            Self::Raise { .. } => Risk::Write,
        }
    }

    fn catalog() -> Vec<ActionDoc> {
        vec![
            ActionDoc {
                name: "ping",
                description: "Answer immediately.",
                risk: Risk::Read,
            },
            ActionDoc {
                name: "raise",
                description: "Raise a Decision through the Module dispatcher.",
                risk: Risk::Write,
            },
        ]
    }
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct ProducerState;

#[derive(Default)]
pub struct Producer;

impl Module for Producer {
    const ID: &'static str = "producer";
    type Action = ProducerAction;
    type Config = NoConfig;
    type State = ProducerState;

    fn initial_state(&self) -> ProducerState {
        ProducerState
    }

    async fn run(&mut self, ctx: &mut ModuleCtx<Self>) -> anyhow::Result<()> {
        while let Some(req) = ctx.recv().await {
            match req.action {
                ProducerAction::Ping => req.respond(Ok(json!("pong"))),
                ProducerAction::Raise {
                    ref key,
                    ref title,
                    ref reason,
                    ref primary_label,
                    ref primary_envelope,
                    ref secondary_label,
                } => {
                    let mut primary = json!({ "label": primary_label });
                    if let Some(envelope) = primary_envelope {
                        primary["envelope"] = serde_json::to_value(envelope).unwrap();
                    }
                    let envelope = Envelope {
                        module: "needs_you".into(),
                        action: json!({
                            "raise": {
                                "key": key,
                                "title": title,
                                "reason": reason,
                                "primary": primary,
                                "secondary": { "label": secondary_label },
                            }
                        }),
                    };
                    match ctx.dispatcher().send(envelope) {
                        Ok(wait) => {
                            let outcome = wait
                                .await
                                .map_err(|err| ActionError::Failed(err.to_string()));
                            req.respond(outcome);
                        }
                        Err(err) => {
                            req.respond::<Value>(Err(ActionError::Failed(err.to_string())));
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

pub struct Harness {
    pub kernel: Kernel,
    pub clock: FakeClock,
    pub store: Store,
}

pub fn harness() -> Harness {
    harness_with_timeout(Duration::from_secs(30))
}

pub fn harness_with_timeout(timeout: Duration) -> Harness {
    start_harness(timeout, true)
}

pub fn harness_ungranted() -> Harness {
    start_harness(Duration::from_secs(30), false)
}

fn start_harness(timeout: Duration, grant: bool) -> Harness {
    let clock = FakeClock::at(ORIGIN_MS);
    let store = Store::open_in_memory().unwrap();
    let mut builder = Kernel::builder_with_clock(store.clone(), Arc::new(clock.clone()))
        .pause_journal_sweep()
        .dispatch_timeout(timeout)
        .register(NeedsYou::new(JournalSweep::Paused))
        .register(Target)
        .register(Producer);
    if grant {
        builder = builder.grant_destructive("needs_you");
    }
    Harness {
        kernel: builder.start(),
        clock,
        store,
    }
}

impl Harness {
    pub async fn raise(&self, action: Value) -> Result<Value, DispatchError> {
        self.kernel
            .dispatch(Envelope {
                module: "needs_you".into(),
                action,
            })
            .await
    }

    pub async fn answer(
        &self,
        id: i64,
        revision: u32,
        choice: &str,
    ) -> Result<Value, DispatchError> {
        self.kernel
            .dispatch(Envelope {
                module: "needs_you".into(),
                action: json!({
                    "answer": {
                        "id": id,
                        "revision": revision,
                        "choice": choice,
                    }
                }),
            })
            .await
    }

    pub fn state(&self) -> Value {
        self.kernel.state("needs_you").unwrap()
    }

    pub fn target_state(&self) -> Value {
        self.kernel.state("target").unwrap()
    }

    pub async fn idle(&self) {
        for _ in 0..30 {
            tokio::task::yield_now().await;
        }
    }

    pub async fn settle(&self) {
        for _ in 0..100 {
            tokio::task::yield_now().await;
        }
    }

    pub async fn reopen(&mut self) {
        self.reopen_with_timeout(Duration::from_secs(30)).await;
    }

    pub async fn reopen_with_timeout(&mut self, timeout: Duration) {
        self.kernel.shutdown().await;
        self.kernel = Kernel::builder_with_clock(self.store.clone(), Arc::new(self.clock.clone()))
            .pause_journal_sweep()
            .dispatch_timeout(timeout)
            .register(NeedsYou::new(JournalSweep::Paused))
            .register(Target)
            .register(Producer)
            .grant_destructive("needs_you")
            .start();
        self.idle().await;
    }

    pub async fn shutdown(self) {
        self.kernel.shutdown().await;
    }

    pub async fn advance(&self, by: Duration) {
        self.clock.advance(by);
        tokio::time::advance(by).await;
    }
}

pub fn raise_body(
    key: &str,
    title: &str,
    reason: &str,
    primary_label: &str,
    secondary_label: &str,
) -> Value {
    json!({
        "raise": {
            "key": key,
            "title": title,
            "reason": reason,
            "primary": { "label": primary_label },
            "secondary": { "label": secondary_label },
        }
    })
}

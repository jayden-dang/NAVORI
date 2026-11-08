#![allow(dead_code, clippy::unwrap_used)]

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use navori_config::JournalSweep;
use navori_kernel::{
    ActionDoc, DispatchError, Envelope, Kernel, Module, ModuleAction, ModuleCtx, NoConfig, Risk,
    WallClock,
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
}

impl ModuleAction for TargetAction {
    fn risk(&self) -> Risk {
        Risk::Write
    }

    fn catalog() -> Vec<ActionDoc> {
        vec![ActionDoc {
            name: "poke",
            description: "A recoverable write for Needs you tests.",
            risk: Risk::Write,
        }]
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
        while let Some(req) = ctx.recv().await {
            match req.action {
                TargetAction::Poke => {
                    ctx.update_state(|state| state.seen.push(json!("poke")));
                    req.respond(Ok(json!("poked")));
                }
            }
        }
        Ok(())
    }
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ProducerAction {
    Ping,
}

impl ModuleAction for ProducerAction {
    fn risk(&self) -> Risk {
        Risk::Read
    }

    fn catalog() -> Vec<ActionDoc> {
        vec![ActionDoc {
            name: "ping",
            description: "Answer immediately.",
            risk: Risk::Read,
        }]
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
    let clock = FakeClock::at(ORIGIN_MS);
    let store = Store::open_in_memory().unwrap();
    let kernel = Kernel::builder_with_clock(store.clone(), Arc::new(clock.clone()))
        .pause_journal_sweep()
        .register(NeedsYou::new(JournalSweep::Paused))
        .register(Target)
        .register(Producer)
        .start();
    Harness {
        kernel,
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

    pub fn state(&self) -> Value {
        self.kernel.state("needs_you").unwrap()
    }

    pub async fn idle(&self) {
        for _ in 0..30 {
            tokio::task::yield_now().await;
        }
    }

    pub async fn reopen(&mut self) {
        self.kernel.shutdown().await;
        self.kernel = Kernel::builder_with_clock(self.store.clone(), Arc::new(self.clock.clone()))
            .pause_journal_sweep()
            .register(NeedsYou::new(JournalSweep::Paused))
            .register(Target)
            .register(Producer)
            .start();
        self.idle().await;
    }

    pub async fn shutdown(self) {
        self.kernel.shutdown().await;
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

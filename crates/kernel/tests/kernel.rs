//! Kernel behavior observed through its public API with a real (in-memory) store.
#![allow(clippy::unwrap_used)] // helpers outside #[test] fns

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use navori_kernel::{
    ActionDoc, ActionError, DispatchError, Envelope, Event, Kernel, Module, ModuleAction,
    ModuleConfig, ModuleCtx, ModuleState, NoConfig, RestartPolicy, Risk, Severity, Wake, WallClock,
};
use navori_store::Store;
use serde::{Deserialize, Serialize};
use serde_json::json;

/// A small counter module used as the kernel's test subject.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
enum CounterAction {
    /// Add `by` to the counter.
    Add { by: u32 },
    /// Read the counter.
    Get,
    /// Crash the module (panic).
    Crash,
    /// Never answer.
    Hang,
}

impl ModuleAction for CounterAction {
    fn risk(&self) -> Risk {
        match self {
            Self::Get => Risk::Read,
            Self::Add { .. } | Self::Hang => Risk::Write,
            Self::Crash => Risk::Destructive,
        }
    }
    fn catalog() -> Vec<ActionDoc> {
        vec![
            ActionDoc {
                name: "add",
                description: "Add to the counter.",
                risk: Risk::Write,
            },
            ActionDoc {
                name: "get",
                description: "Read the counter.",
                risk: Risk::Read,
            },
            ActionDoc {
                name: "crash",
                description: "Panic.",
                risk: Risk::Destructive,
            },
            ActionDoc {
                name: "hang",
                description: "Never answer.",
                risk: Risk::Write,
            },
        ]
    }
}

#[derive(Debug, Clone, Default, Serialize)]
struct CounterState {
    value: u32,
}

#[derive(Debug, Default)]
struct Counter {
    starts: Arc<AtomicU32>,
    crash_on_start: bool,
}

impl Module for Counter {
    const ID: &'static str = "counter";
    type Action = CounterAction;
    type Config = NoConfig;
    type State = CounterState;

    fn initial_state(&self) -> CounterState {
        CounterState::default()
    }

    async fn run(&mut self, ctx: &mut ModuleCtx<Self>) -> anyhow::Result<()> {
        self.starts.fetch_add(1, Ordering::SeqCst);
        if self.crash_on_start {
            anyhow::bail!("refusing to start");
        }
        let mut hung = Vec::new();
        while let Some(req) = ctx.recv().await {
            match req.action {
                CounterAction::Add { by } => {
                    let mut now = 0;
                    ctx.update_state(|s| {
                        s.value += by;
                        now = s.value;
                    });
                    ctx.emit(
                        "counter.added",
                        Severity::Info,
                        format!("counter is now {now}"),
                        json!({ "value": now }),
                    );
                    req.respond(Ok(json!({ "value": now })));
                }
                CounterAction::Get => {
                    req.respond::<()>(Err(ActionError::Failed("use state instead".into())));
                }
                CounterAction::Crash => panic!("boom"),
                CounterAction::Hang => hung.push(req),
            }
        }
        Ok(())
    }
}

fn fast_restarts(max: u32) -> RestartPolicy {
    RestartPolicy {
        max_restarts: max,
        base_delay: Duration::from_millis(5),
        max_delay: Duration::from_millis(20),
    }
}

fn env(action: serde_json::Value) -> Envelope {
    Envelope {
        module: "counter".into(),
        action,
    }
}

fn kernel_with(counter: Counter) -> Kernel {
    Kernel::builder(Store::open_in_memory().unwrap())
        .restart_policy(fast_restarts(3))
        .dispatch_timeout(Duration::from_millis(300))
        .register(counter)
        .start()
}

async fn wait_for(kernel: &Kernel, pred: impl Fn(&ModuleState) -> bool) -> ModuleState {
    for _ in 0..200 {
        if let Some(m) = kernel.modules().into_iter().find(|m| m.id == "counter")
            && pred(&m.state)
        {
            return m.state;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    panic!("condition not reached; modules: {:?}", kernel.modules());
}

#[tokio::test]
async fn dispatch_routes_an_envelope_to_the_module_and_returns_its_answer() {
    let kernel = kernel_with(Counter::default());
    let out = kernel
        .dispatch(env(json!({ "add": { "by": 2 } })))
        .await
        .unwrap();
    assert_eq!(out, json!({ "value": 2 }));
    let out = kernel
        .dispatch(env(json!({ "add": { "by": 3 } })))
        .await
        .unwrap();
    assert_eq!(out, json!({ "value": 5 }));
    assert_eq!(kernel.state("counter"), Some(json!({ "value": 5 })));
    kernel.shutdown().await;
}

#[tokio::test]
async fn action_errors_are_returned_to_the_caller() {
    let kernel = kernel_with(Counter::default());
    let err = kernel.dispatch(env(json!("get"))).await.unwrap_err();
    assert_eq!(
        err,
        DispatchError::Action(ActionError::Failed("use state instead".into()))
    );
    kernel.shutdown().await;
}

#[tokio::test]
async fn unknown_modules_and_malformed_actions_are_rejected() {
    let kernel = kernel_with(Counter::default());
    let err = kernel
        .dispatch(Envelope {
            module: "nope".into(),
            action: json!("get"),
        })
        .await
        .unwrap_err();
    assert_eq!(err, DispatchError::UnknownModule("nope".into()));

    let err = kernel
        .dispatch(env(json!({ "add": { "by": "two" } })))
        .await
        .unwrap_err();
    assert!(
        matches!(err, DispatchError::InvalidAction { ref module, .. } if module == "counter"),
        "{err:?}"
    );
    kernel.shutdown().await;
}

#[tokio::test]
async fn actions_are_listed_with_risk_and_input_schema() {
    let kernel = kernel_with(Counter::default());
    let specs: Vec<_> = kernel
        .actions()
        .into_iter()
        .filter(|s| s.module == "counter")
        .collect();
    let names: Vec<_> = specs.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, vec!["add", "get", "crash", "hang"]);
    assert_eq!(specs[2].risk, Risk::Destructive);
    assert!(
        specs[0].input_schema.to_string().contains("by"),
        "{}",
        specs[0].input_schema
    );
    kernel.shutdown().await;
}

#[tokio::test]
async fn module_events_are_broadcast_and_journaled() {
    let kernel = kernel_with(Counter::default());
    let mut rx = kernel.bus().subscribe();
    kernel
        .dispatch(env(json!({ "add": { "by": 7 } })))
        .await
        .unwrap();

    let ev = tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            let ev = rx.recv().await.unwrap();
            if ev.kind == "counter.added" {
                return ev;
            }
        }
    })
    .await
    .unwrap();
    assert_eq!(ev.module, "counter");
    assert_eq!(ev.summary, "counter is now 7");
    assert!(!ev.id.is_empty());

    let journaled = kernel.recent_events(50).unwrap();
    assert!(journaled.iter().any(|e| e.id == ev.id), "{journaled:?}");
    kernel.shutdown().await;
}

#[tokio::test]
async fn state_changes_bump_the_revision() {
    let kernel = kernel_with(Counter::default());
    let rev = kernel.revision();
    let before = *rev.borrow();
    kernel
        .dispatch(env(json!({ "add": { "by": 1 } })))
        .await
        .unwrap();
    assert!(*rev.borrow() > before);
    kernel.shutdown().await;
}

#[tokio::test]
async fn a_crashed_module_is_restarted_and_keeps_serving() {
    let starts = Arc::new(AtomicU32::new(0));
    let kernel = kernel_with(Counter {
        starts: starts.clone(),
        crash_on_start: false,
    });
    wait_for(&kernel, |s| *s == ModuleState::Running).await;

    let err = kernel.dispatch(env(json!("crash"))).await.unwrap_err();
    assert_eq!(err, DispatchError::Unavailable("counter".into()));

    wait_for(&kernel, |s| *s == ModuleState::Running).await;
    let out = kernel
        .dispatch(env(json!({ "add": { "by": 1 } })))
        .await
        .unwrap();
    assert_eq!(out, json!({ "value": 1 }));
    assert_eq!(starts.load(Ordering::SeqCst), 2);
    let info = kernel
        .modules()
        .into_iter()
        .find(|m| m.id == "counter")
        .unwrap();
    assert_eq!(info.restarts, 1);
    assert!(
        kernel
            .recent_events(50)
            .unwrap()
            .iter()
            .any(|e| e.kind == "module.crashed")
    );
    kernel.shutdown().await;
}

#[tokio::test]
async fn a_module_that_keeps_crashing_is_marked_failed_and_unavailable() {
    let kernel = kernel_with(Counter {
        starts: Arc::default(),
        crash_on_start: true,
    });
    let state = wait_for(&kernel, |s| matches!(s, ModuleState::Failed { .. })).await;
    assert!(
        matches!(state, ModuleState::Failed { ref last_error } if last_error.contains("refusing to start"))
    );
    let err = kernel
        .dispatch(env(json!({ "add": { "by": 1 } })))
        .await
        .unwrap_err();
    assert_eq!(err, DispatchError::Unavailable("counter".into()));
    kernel.shutdown().await;
}

#[tokio::test]
async fn a_module_that_never_answers_times_out() {
    let kernel = kernel_with(Counter::default());
    let err = kernel.dispatch(env(json!("hang"))).await.unwrap_err();
    assert_eq!(err, DispatchError::Timeout("counter".into()));
    kernel.shutdown().await;
}

#[tokio::test]
async fn shutdown_stops_every_module() {
    let kernel = kernel_with(Counter::default());
    wait_for(&kernel, |s| *s == ModuleState::Running).await;
    tokio::time::timeout(Duration::from_secs(2), kernel.shutdown())
        .await
        .unwrap();
    let info = kernel
        .modules()
        .into_iter()
        .find(|m| m.id == "counter")
        .unwrap();
    assert_eq!(info.state, ModuleState::Stopped);
}

#[tokio::test]
async fn the_builtin_system_module_answers_ping() {
    let kernel = Kernel::builder(Store::open_in_memory().unwrap()).start();
    let out = kernel
        .dispatch(Envelope {
            module: "system".into(),
            action: json!("ping"),
        })
        .await
        .unwrap();
    assert_eq!(out, json!("pong"));
    kernel.shutdown().await;
}

#[tokio::test]
async fn the_system_module_prunes_the_journal_using_the_configured_retention() {
    let store = Store::open_in_memory().unwrap();
    let day_ms = 24 * 60 * 60 * 1000;
    let now_ms = i64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis(),
    )
    .unwrap();
    for (id, age_days) in [("old", 10), ("recent", 1)] {
        store
            .journal_append(&navori_kernel::Event {
                id: id.into(),
                ts_ms: now_ms - age_days * day_ms,
                module: "test".into(),
                kind: "seed".into(),
                severity: Severity::Info,
                summary: id.into(),
                payload: json!(null),
            })
            .unwrap();
    }
    let kernel = Kernel::builder(store).journal_retention_days(3).start();
    for _ in 0..200 {
        if kernel
            .recent_events(50)
            .unwrap()
            .iter()
            .all(|e| e.id != "old")
        {
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    let ids: Vec<_> = kernel
        .recent_events(50)
        .unwrap()
        .into_iter()
        .map(|e| e.id)
        .collect();
    assert!(!ids.contains(&"old".to_owned()), "{ids:?}");
    assert!(ids.contains(&"recent".to_owned()), "{ids:?}");
    kernel.shutdown().await;
}

#[derive(Debug)]
struct NoteConfig {
    note: u32,
    hits: AtomicU32,
}

impl Default for NoteConfig {
    fn default() -> Self {
        Self {
            note: 7,
            hits: AtomicU32::new(0),
        }
    }
}

impl<'de> Deserialize<'de> for NoteConfig {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Raw {
            note: u32,
        }
        let raw = Raw::deserialize(deserializer)?;
        Ok(Self {
            note: raw.note,
            hits: AtomicU32::new(0),
        })
    }
}

impl ModuleConfig for NoteConfig {
    fn check(&self) -> Result<(), &'static str> {
        if self.note == 0 {
            Err("note must not be zero")
        } else {
            Ok(())
        }
    }
}

#[derive(Debug, Clone, Default, Serialize)]
struct NotedState {
    note: u32,
    hits: u32,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
enum NotedAction {
    /// Keep the module running after it has published its config.
    Hold,
}

impl ModuleAction for NotedAction {
    fn risk(&self) -> Risk {
        Risk::Read
    }

    fn catalog() -> Vec<ActionDoc> {
        vec![ActionDoc {
            name: "hold",
            description: "Keep the module running after it has published its config.",
            risk: Risk::Read,
        }]
    }
}

#[derive(Debug)]
struct Noted {
    crash_on_start: bool,
}

impl Module for Noted {
    const ID: &'static str = "noted";
    type Action = NotedAction;
    type State = NotedState;
    type Config = NoteConfig;

    fn initial_state(&self) -> NotedState {
        NotedState::default()
    }

    async fn run(&mut self, ctx: &mut ModuleCtx<Self>) -> anyhow::Result<()> {
        publish_note(ctx);
        if self.crash_on_start {
            self.crash_on_start = false;
            anyhow::bail!("once");
        }
        while let Some(req) = ctx.recv().await {
            match req.action {
                NotedAction::Hold => req.respond(Ok(())),
            }
        }
        Ok(())
    }
}

#[derive(Debug, Default)]
struct Tuned;

impl Module for Tuned {
    const ID: &'static str = "tuned";
    type Action = NotedAction;
    type State = NotedState;
    type Config = NoteConfig;

    fn initial_state(&self) -> NotedState {
        NotedState::default()
    }

    async fn run(&mut self, ctx: &mut ModuleCtx<Self>) -> anyhow::Result<()> {
        publish_note(ctx);
        while let Some(req) = ctx.recv().await {
            match req.action {
                NotedAction::Hold => req.respond(Ok(())),
            }
        }
        Ok(())
    }
}

fn publish_note<M>(ctx: &ModuleCtx<M>)
where
    M: Module<Config = NoteConfig, State = NotedState>,
{
    let config = ctx.config();
    let hits = config.hits.fetch_add(1, Ordering::SeqCst) + 1;
    let note = config.note;
    ctx.update_state(|state| {
        state.note = note;
        state.hits = hits;
    });
}

#[derive(Debug, Clone, Default, Serialize)]
struct OutsideState {
    note: u32,
    woke: bool,
}

#[derive(Debug, Default)]
struct Outside;

impl Module for Outside {
    const ID: &'static str = "outside";
    type Action = NotedAction;
    type State = OutsideState;
    type Config = NoteConfig;

    fn initial_state(&self) -> OutsideState {
        OutsideState::default()
    }

    async fn run(&mut self, ctx: &mut ModuleCtx<Self>) -> anyhow::Result<()> {
        let note = ctx.config().note;
        let mut handle = ctx.wait_handle();
        let woke = matches!(
            handle.wait_until(Some(SystemTime::UNIX_EPOCH)).await,
            Wake::Due
        );
        ctx.update_state(|state| {
            state.note = note;
            state.woke = woke;
        });
        while let Some(req) = ctx.recv().await {
            match req.action {
                NotedAction::Hold => req.respond(Ok(())),
            }
        }
        Ok(())
    }
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
enum KeeperAction {
    /// Add `by` to the kept value.
    Add { by: u32 },
    /// Crash the module.
    Crash,
}

impl ModuleAction for KeeperAction {
    fn risk(&self) -> Risk {
        match self {
            Self::Add { .. } => Risk::Write,
            Self::Crash => Risk::Destructive,
        }
    }

    fn catalog() -> Vec<ActionDoc> {
        vec![
            ActionDoc {
                name: "add",
                description: "Add to the kept value.",
                risk: Risk::Write,
            },
            ActionDoc {
                name: "crash",
                description: "Crash the module.",
                risk: Risk::Destructive,
            },
        ]
    }
}

#[derive(Debug, Clone, Default, Serialize)]
struct KeeperState {
    marker: u64,
    runs: u32,
    value: u32,
}

#[derive(Debug)]
struct Keeper {
    marker: u64,
    runs: u32,
}

impl Module for Keeper {
    const ID: &'static str = "keeper";
    type Action = KeeperAction;
    type State = KeeperState;
    type Config = NoConfig;

    fn initial_state(&self) -> KeeperState {
        KeeperState::default()
    }

    async fn run(&mut self, ctx: &mut ModuleCtx<Self>) -> anyhow::Result<()> {
        self.runs += 1;
        let marker = self.marker;
        let runs = self.runs;
        ctx.update_state(|state| {
            state.marker = marker;
            state.runs = runs;
        });
        while let Some(req) = ctx.recv().await {
            match req.action {
                KeeperAction::Add { by } => {
                    let mut now = 0;
                    ctx.update_state(|state| {
                        state.value += by;
                        now = state.value;
                    });
                    req.respond(Ok(json!({ "value": now })));
                }
                KeeperAction::Crash => panic!("boom"),
            }
        }
        Ok(())
    }
}

fn sections(pairs: &[(&str, serde_json::Value)]) -> serde_json::Map<String, serde_json::Value> {
    pairs
        .iter()
        .map(|(key, value)| ((*key).to_owned(), value.clone()))
        .collect()
}

async fn wait_state(
    kernel: &Kernel,
    module: &str,
    pred: impl Fn(&serde_json::Value) -> bool,
) -> serde_json::Value {
    for _ in 0..200 {
        if let Some(value) = kernel.state(module)
            && pred(&value)
        {
            return value;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    panic!("state not reached for {module}: {:?}", kernel.state(module));
}

async fn wait_module(
    kernel: &Kernel,
    id: &str,
    pred: impl Fn(&ModuleState) -> bool,
) -> ModuleState {
    for _ in 0..200 {
        if let Some(module) = kernel.modules().into_iter().find(|module| module.id == id)
            && pred(&module.state)
        {
            return module.state;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    panic!("condition not reached; modules: {:?}", kernel.modules());
}

fn noted_kernel(module: Noted, sections: serde_json::Map<String, serde_json::Value>) -> Kernel {
    Kernel::builder(Store::open_in_memory().unwrap())
        .restart_policy(fast_restarts(3))
        .dispatch_timeout(Duration::from_millis(300))
        .config_sections(sections)
        .register(module)
        .start()
}

#[tokio::test]
async fn a_module_outside_the_kernel_uses_config_and_a_wait_handle() {
    let kernel = Kernel::builder(Store::open_in_memory().unwrap())
        .config_sections(sections(&[("outside", json!({ "note": 4 }))]))
        .register(Outside)
        .start();
    let state = wait_state(&kernel, "outside", |state| state["woke"] == json!(true)).await;
    assert_eq!(state, json!({ "note": 4, "woke": true }));
    kernel.shutdown().await;
}

#[tokio::test]
async fn a_module_reads_its_config_section() {
    let kernel = Kernel::builder(Store::open_in_memory().unwrap())
        .config_sections(sections(&[("noted", json!({ "note": 4 }))]))
        .register(Noted {
            crash_on_start: false,
        })
        .start();
    let state = wait_state(&kernel, "noted", |state| state["hits"] == json!(1)).await;
    assert_eq!(state, json!({ "note": 4, "hits": 1 }));
    assert!(kernel.config_issues().is_empty());
    kernel.shutdown().await;
}

#[tokio::test]
async fn an_absent_section_gives_the_default() {
    let kernel = Kernel::builder(Store::open_in_memory().unwrap())
        .register(Noted {
            crash_on_start: false,
        })
        .start();
    let state = wait_state(&kernel, "noted", |state| state["hits"] == json!(1)).await;
    assert_eq!(state, json!({ "note": 7, "hits": 1 }));
    assert!(kernel.config_issues().is_empty());
    kernel.shutdown().await;
}

#[tokio::test]
async fn an_undecodable_section_gives_that_module_alone_the_default_and_is_listed() {
    let kernel = Kernel::builder(Store::open_in_memory().unwrap())
        .config_sections(sections(&[
            ("noted", json!(3)),
            ("tuned", json!({ "note": 11 })),
        ]))
        .register(Noted {
            crash_on_start: false,
        })
        .register(Tuned)
        .start();
    let noted = wait_state(&kernel, "noted", |state| state["hits"] == json!(1)).await;
    let tuned = wait_state(&kernel, "tuned", |state| state["hits"] == json!(1)).await;
    assert_eq!(noted, json!({ "note": 7, "hits": 1 }));
    assert_eq!(tuned, json!({ "note": 11, "hits": 1 }));
    assert_eq!(kernel.config_issues(), ["noted"].as_slice());
    kernel.shutdown().await;
}

#[tokio::test]
async fn a_section_failing_the_check_gives_the_default_and_is_listed() {
    let kernel = noted_kernel(
        Noted {
            crash_on_start: false,
        },
        sections(&[("noted", json!({ "note": 0 }))]),
    );
    let state = wait_state(&kernel, "noted", |state| state["hits"] == json!(1)).await;
    assert_eq!(state, json!({ "note": 7, "hits": 1 }));
    assert_eq!(kernel.config_issues(), ["noted"].as_slice());
    kernel.shutdown().await;
}

#[tokio::test]
async fn a_restarted_module_sees_the_same_config() {
    let kernel = noted_kernel(
        Noted {
            crash_on_start: true,
        },
        sections(&[("noted", json!({ "note": 4 }))]),
    );
    let state = wait_state(&kernel, "noted", |state| state["hits"] == json!(2)).await;
    assert_eq!(state, json!({ "note": 4, "hits": 2 }));
    assert!(kernel.config_issues().is_empty());
    kernel.shutdown().await;
}

#[tokio::test]
async fn a_module_with_no_config_ignores_a_section_with_its_id() {
    let kernel = Kernel::builder(Store::open_in_memory().unwrap())
        .config_sections(sections(&[("system", json!(3))]))
        .start();
    assert!(kernel.config_issues().is_empty());
    let out = kernel
        .dispatch(Envelope {
            module: "system".into(),
            action: json!("ping"),
        })
        .await
        .unwrap();
    assert_eq!(out, json!("pong"));
    kernel.shutdown().await;
}

#[tokio::test]
async fn config_sections_may_be_set_after_register() {
    let kernel = Kernel::builder(Store::open_in_memory().unwrap())
        .register(Noted {
            crash_on_start: false,
        })
        .config_sections(sections(&[("noted", json!({ "note": 4 }))]))
        .start();
    let state = wait_state(&kernel, "noted", |state| state["hits"] == json!(1)).await;
    assert_eq!(state, json!({ "note": 4, "hits": 1 }));
    kernel.shutdown().await;
}

#[tokio::test]
async fn a_restart_keeps_the_module_fields_state_and_queued_requests() {
    let kernel = Kernel::builder(Store::open_in_memory().unwrap())
        .restart_policy(RestartPolicy {
            max_restarts: 3,
            base_delay: Duration::from_millis(200),
            max_delay: Duration::from_millis(200),
        })
        .dispatch_timeout(Duration::from_secs(2))
        .register(Keeper {
            marker: 42,
            runs: 0,
        })
        .start();
    let out = kernel
        .dispatch(Envelope {
            module: "keeper".into(),
            action: json!({ "add": { "by": 5 } }),
        })
        .await
        .unwrap();
    assert_eq!(out, json!({ "value": 5 }));

    let err = kernel
        .dispatch(Envelope {
            module: "keeper".into(),
            action: json!("crash"),
        })
        .await
        .unwrap_err();
    assert_eq!(err, DispatchError::Unavailable("keeper".into()));
    wait_module(&kernel, "keeper", |state| {
        matches!(state, ModuleState::Restarting { .. })
    })
    .await;

    let out = tokio::time::timeout(
        Duration::from_secs(2),
        kernel.dispatch(Envelope {
            module: "keeper".into(),
            action: json!({ "add": { "by": 1 } }),
        }),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(out, json!({ "value": 6 }));
    assert_eq!(
        kernel.state("keeper"),
        Some(json!({ "marker": 42, "runs": 2, "value": 6 }))
    );
    kernel.shutdown().await;
}

const DAY: Duration = Duration::from_secs(24 * 60 * 60);
const SIX_HOURS: Duration = Duration::from_secs(6 * 60 * 60);
const ORIGIN_MS: u64 = 1_700_000_000_000;

#[derive(Clone, Debug)]
struct FakeClock {
    millis: Arc<AtomicU64>,
}

impl FakeClock {
    fn at(millis: u64) -> Self {
        Self {
            millis: Arc::new(AtomicU64::new(millis)),
        }
    }

    fn advance(&self, by: Duration) {
        let millis = u64::try_from(by.as_millis()).unwrap();
        self.millis.fetch_add(millis, Ordering::SeqCst);
    }
}

impl WallClock for FakeClock {
    fn now(&self) -> SystemTime {
        UNIX_EPOCH + Duration::from_millis(self.millis.load(Ordering::SeqCst))
    }
}

fn seed_event(store: &Store, id: &str, age: Duration) {
    let now_ms = i64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis(),
    )
    .unwrap();
    let age_ms = i64::try_from(age.as_millis()).unwrap();
    store
        .journal_append(&Event {
            id: id.into(),
            ts_ms: now_ms - age_ms,
            module: "test".into(),
            kind: "seed".into(),
            severity: Severity::Info,
            summary: id.into(),
            payload: json!(null),
        })
        .unwrap();
}

fn event_ids(kernel: &Kernel) -> Vec<String> {
    kernel
        .recent_events(50)
        .unwrap()
        .into_iter()
        .map(|event| event.id)
        .collect()
}

fn has_event(kernel: &Kernel, id: &str) -> bool {
    event_ids(kernel).iter().any(|event| event == id)
}

fn sweep_has_run(kernel: &Kernel) -> bool {
    kernel
        .state("system")
        .and_then(|state| state.get("last_prune_removed").cloned())
        .is_some_and(|removed| !removed.is_null())
}

async fn drive_until(mut ready: impl FnMut() -> bool) {
    for _ in 0..100 {
        if ready() {
            return;
        }
        tokio::task::yield_now().await;
    }
    panic!("condition not reached");
}

#[tokio::test(start_paused = true)]
async fn a_paused_sweep_deletes_nothing_at_start_or_after_six_hours() {
    let store = Store::open_in_memory().unwrap();
    seed_event(&store, "old", DAY * 40);
    let clock = FakeClock::at(ORIGIN_MS);
    let kernel = Kernel::builder_with_clock(store, Arc::new(clock.clone()))
        .pause_journal_sweep()
        .start();
    for _ in 0..20 {
        tokio::task::yield_now().await;
    }
    assert!(has_event(&kernel, "old"), "{:?}", event_ids(&kernel));

    clock.advance(SIX_HOURS + Duration::from_secs(1));
    tokio::time::advance(SIX_HOURS + Duration::from_secs(1)).await;
    for _ in 0..20 {
        tokio::task::yield_now().await;
    }
    assert!(has_event(&kernel, "old"), "{:?}", event_ids(&kernel));
    kernel.shutdown().await;
}

#[tokio::test]
async fn a_paused_sweep_is_reported_in_state_as_soon_as_start_returns() {
    let kernel = Kernel::builder(Store::open_in_memory().unwrap())
        .pause_journal_sweep()
        .start();
    assert_eq!(
        kernel.state("system"),
        Some(json!({
            "last_prune_removed": null,
            "sweep_paused": true
        }))
    );
    kernel.shutdown().await;
}

#[tokio::test]
async fn an_unset_builder_sweeps_at_30_days() {
    let store = Store::open_in_memory().unwrap();
    seed_event(&store, "old", DAY * 31);
    seed_event(&store, "kept", DAY * 29);
    let kernel = Kernel::builder(store).start();
    for _ in 0..200 {
        if !has_event(&kernel, "old") {
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    let ids = event_ids(&kernel);
    assert!(!ids.contains(&"old".to_owned()), "{ids:?}");
    assert!(ids.contains(&"kept".to_owned()), "{ids:?}");
    kernel.shutdown().await;
}

#[tokio::test]
async fn a_retention_below_one_is_raised_to_one() {
    let store = Store::open_in_memory().unwrap();
    seed_event(&store, "stale", DAY * 2);
    seed_event(&store, "fresh", Duration::from_secs(12 * 60 * 60));
    let kernel = Kernel::builder(store).journal_retention_days(0).start();
    for _ in 0..200 {
        if !has_event(&kernel, "stale") {
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    let ids = event_ids(&kernel);
    assert!(!ids.contains(&"stale".to_owned()), "{ids:?}");
    assert!(ids.contains(&"fresh".to_owned()), "{ids:?}");
    kernel.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn a_later_sweep_runs_six_hours_after_the_last() {
    let clock = FakeClock::at(ORIGIN_MS);
    let kernel =
        Kernel::builder_with_clock(Store::open_in_memory().unwrap(), Arc::new(clock.clone()))
            .start();
    drive_until(|| sweep_has_run(&kernel)).await;
    seed_event(kernel.store(), "later", DAY * 40);

    clock.advance(SIX_HOURS - Duration::from_secs(1));
    tokio::time::advance(SIX_HOURS - Duration::from_secs(1)).await;
    assert!(has_event(&kernel, "later"), "{:?}", event_ids(&kernel));

    clock.advance(Duration::from_secs(1));
    tokio::time::advance(Duration::from_secs(1)).await;
    drive_until(|| !has_event(&kernel, "later")).await;
    assert!(!has_event(&kernel, "later"), "{:?}", event_ids(&kernel));
    kernel.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn one_long_sleep_gives_one_catch_up_sweep() {
    let store = Store::open_in_memory().unwrap();
    let clock = FakeClock::at(ORIGIN_MS);
    let kernel = Kernel::builder_with_clock(store.clone(), Arc::new(clock.clone())).start();
    drive_until(|| sweep_has_run(&kernel)).await;
    seed_event(&store, "first", DAY * 40);

    clock.advance(DAY * 3);
    kernel.scheduler().wake_pulse();
    drive_until(|| !has_event(&kernel, "first")).await;
    assert!(!has_event(&kernel, "first"), "{:?}", event_ids(&kernel));

    seed_event(&store, "second", DAY * 40);
    assert!(has_event(&kernel, "second"), "{:?}", event_ids(&kernel));

    clock.advance(SIX_HOURS - Duration::from_secs(1));
    kernel.scheduler().wake_pulse();
    for _ in 0..20 {
        tokio::task::yield_now().await;
    }
    assert!(has_event(&kernel, "second"), "{:?}", event_ids(&kernel));

    clock.advance(Duration::from_secs(1));
    kernel.scheduler().wake_pulse();
    drive_until(|| !has_event(&kernel, "second")).await;
    assert!(!has_event(&kernel, "second"), "{:?}", event_ids(&kernel));
    kernel.shutdown().await;
}

#[tokio::test]
async fn prune_journal_with_a_day_count_still_deletes_while_paused() {
    let store = Store::open_in_memory().unwrap();
    seed_event(&store, "old", DAY * 40);
    seed_event(&store, "recent", DAY);
    let kernel = Kernel::builder(store).pause_journal_sweep().start();
    for _ in 0..40 {
        assert!(has_event(&kernel, "old"), "{:?}", event_ids(&kernel));
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    let removed = kernel
        .dispatch(Envelope {
            module: "system".into(),
            action: json!({ "prune_journal": { "older_than_days": 3 } }),
        })
        .await
        .unwrap();
    assert_eq!(removed, json!(1));
    let ids = event_ids(&kernel);
    assert!(!ids.contains(&"old".to_owned()), "{ids:?}");
    assert!(ids.contains(&"recent".to_owned()), "{ids:?}");
    kernel.shutdown().await;
}

#[tokio::test]
async fn a_failed_sweep_keeps_the_module_running() {
    let store = Store::open_in_memory().unwrap();
    store.with_conn(|conn| {
        conn.execute_batch("DROP TABLE events").unwrap();
    });
    let kernel = Kernel::builder(store).start();
    tokio::time::sleep(Duration::from_millis(200)).await;
    let out = kernel
        .dispatch(Envelope {
            module: "system".into(),
            action: json!("ping"),
        })
        .await
        .unwrap();
    assert_eq!(out, json!("pong"));
    let system = kernel
        .modules()
        .into_iter()
        .find(|module| module.id == "system")
        .unwrap();
    assert_eq!(system.restarts, 0);
    assert_eq!(system.state, ModuleState::Running);
    kernel.shutdown().await;
}

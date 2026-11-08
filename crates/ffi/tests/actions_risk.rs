//! The Risk the catalog shows for each Action, checked against the Risk the Action reports itself.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use futures::future::BoxFuture;
use navori_context::{SendError, Sender};
use navori_ffi::{AiDeps, CoreConfig, CoreSnapshot, NavoriCore, SnapshotListener};
use navori_kernel::{ModuleAction, SystemAction};
use navori_mod_awake::AwakeAction;
use navori_mod_integrations::IntegrationsAction;
use navori_mod_needs_you::NeedsYouAction;
use navori_mod_ports::PortsAction;
use navori_secrets::Secret;
use navori_secrets::SecretStore;
use navori_secrets::fake::{FakeKeychain, MemoryStore};
use serde_json::{Value, json};

struct Quiet;

impl SnapshotListener for Quiet {
    fn on_snapshot(&self, _: CoreSnapshot) {}
}

struct StubSender;

impl Sender for StubSender {
    fn send<'a>(
        &'a self,
        _: &'a Secret,
        _: &'a str,
        _: &'a str,
    ) -> BoxFuture<'a, Result<String, SendError>> {
        Box::pin(async { Ok("stub".to_owned()) })
    }
}

fn deps() -> AiDeps {
    let vault = FakeKeychain::new();
    AiDeps {
        open_secrets: Arc::new(move |service| {
            let store: Arc<dyn SecretStore> = Arc::new(MemoryStore::new(service, &vault));
            Ok(store)
        }),
        make_sender: Arc::new(|| Arc::new(StubSender)),
        ..AiDeps::production()
    }
}

fn config(home: &tempfile::TempDir) -> CoreConfig {
    CoreConfig {
        profile: "dev".into(),
        home_dir: home.path().to_string_lossy().into_owned(),
        uid: std::os::unix::fs::MetadataExt::uid(&std::fs::metadata(home.path()).unwrap()),
        app_version: "0.1.0-test".into(),
        log_to_file: false,
        helper_path: home
            .path()
            .join("Navori Dev.app/Contents/Helpers/navori")
            .to_string_lossy()
            .into_owned(),
    }
}

const SAMPLED: usize = 28;

fn sample(module: &str, name: &str) -> Value {
    match (module, name) {
        ("system", "ping") => json!("ping"),
        ("system", "prune_journal") => json!({"prune_journal": {"older_than_days": 30}}),
        ("awake", "status") => json!("status"),
        ("awake", "hold_for") => json!({"hold_for": {"seconds": 60}}),
        ("awake", "hold_until_off") => json!("hold_until_off"),
        ("awake", "display") => json!({"display": {"on": true}}),
        ("awake", "hold_command") => json!({"hold_command": {"pid": 1}}),
        ("awake", "hold_session") => {
            json!({"hold_session": {"session_id": "s", "pid": 1, "ppid": 1}})
        }
        ("awake", "stop_session") => json!({"stop_session": {"session_id": "s"}}),
        ("awake", "end_session") => json!({"end_session": {"session_id": "s"}}),
        ("awake", "release_all") => json!("release_all"),
        ("awake", "keep_for") => json!({"keep_for": {"seconds": 60}}),
        ("awake", "keep_indefinitely") => json!("keep_indefinitely"),
        ("awake", "keep_off") => json!("keep_off"),
        ("awake", "tracking") => json!({"tracking": {"on": true}}),
        ("awake", "lid") => json!({"lid": {"on": true}}),
        ("awake", "slept") => json!({"slept": {"at_ms": 1}}),
        ("awake", "woke") => json!("woke"),
        ("awake", "subagent_start") => {
            json!({"subagent_start": {"session_id": "s", "agent_id": "a", "pid": 1, "ppid": 1}})
        }
        ("awake", "subagent_stop") => {
            json!({"subagent_stop": {"session_id": "s", "agent_id": "a"}})
        }
        ("ports", "list") => json!("list"),
        ("ports", "open") => json!({"open": {"port": 3000}}),
        ("ports", "kill") => json!({"kill": {
            "selector": {"pid": 1},
            "targets": [{"process": {"pid": 1, "start_us": 1}}],
        }}),
        ("integrations", "status") => json!("status"),
        ("integrations", "install") => json!({"install": {"harness": "claude"}}),
        ("integrations", "repair") => json!({"repair": {"harness": "claude"}}),
        ("integrations", "remove") => json!({"remove": {"harness": "claude"}}),
        ("needs_you", "raise") => json!({
            "raise": {
                "key": "k",
                "title": "t",
                "reason": "r",
                "primary": {"label": "Ok"},
                "secondary": {"label": "Later"},
            }
        }),
        other => panic!("no sample for the Action {other:?}"),
    }
}

fn own_risk<A: ModuleAction>(sample: Value) -> Value {
    let action: A = serde_json::from_value(sample).unwrap();
    serde_json::to_value(action.risk()).unwrap()
}

#[test]
fn every_action_s_catalog_risk_matches_its_own_risk() {
    let home = tempfile::Builder::new()
        .prefix("nv")
        .tempdir_in("/tmp")
        .unwrap();
    let core = NavoriCore::start_with(config(&home), Arc::new(Quiet), deps()).unwrap();

    let views = core.actions();
    for view in &views {
        let value = sample(&view.module, &view.name);
        let own = match view.module.as_str() {
            "system" => own_risk::<SystemAction>(value),
            "awake" => own_risk::<AwakeAction>(value),
            "ports" => own_risk::<PortsAction>(value),
            "integrations" => own_risk::<IntegrationsAction>(value),
            "needs_you" => own_risk::<NeedsYouAction>(value),
            module => panic!("no action type for the module {module}"),
        };
        assert_eq!(
            own,
            Value::String(view.risk.clone()),
            "{}.{}",
            view.module,
            view.name
        );
    }
    assert_eq!(views.len(), SAMPLED);
}

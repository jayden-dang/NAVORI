//! Needs you Destructive grant, Navori AI's refusal to answer, and History merge.
#![allow(clippy::unwrap_used)]

use std::sync::Arc;
use std::time::{Duration, Instant};

use futures::executor::block_on;
use navori_agent::{Dispatcher, Toolset};
use navori_config::JournalSweep;
use navori_ffi::{CoreConfig, CoreSnapshot, NavoriCore, SnapshotListener};
use navori_kernel::{Envelope, Kernel};
use navori_llm::ToolCall;
use navori_mod_needs_you::NeedsYou;
use navori_store::Store;
use serde_json::json;

struct Quiet;

impl SnapshotListener for Quiet {
    fn on_snapshot(&self, _: CoreSnapshot) {}
}

fn home() -> tempfile::TempDir {
    tempfile::Builder::new()
        .prefix("nv")
        .tempdir_in("/tmp")
        .unwrap()
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

fn eventually(mut pred: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline {
        if pred() {
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("condition not reached in time");
}

fn dispatch(core: &NavoriCore, module: &str, action: serde_json::Value) -> serde_json::Value {
    let raw = block_on(core.dispatch(module.to_owned(), action.to_string())).unwrap();
    serde_json::from_str(&raw).unwrap()
}

#[tokio::test]
async fn navori_ai_cannot_answer_a_decision() {
    let store = Store::open_in_memory().unwrap();
    let kernel = Kernel::builder(store)
        .pause_journal_sweep()
        .register(NeedsYou::new(JournalSweep::Paused))
        .grant_destructive("needs_you")
        .start();

    for _ in 0..30 {
        tokio::task::yield_now().await;
    }

    let raised = kernel
        .dispatch(Envelope {
            module: "needs_you".into(),
            action: json!({
                "raise": {
                    "key": "k1",
                    "title": "Stay open",
                    "reason": "AI must not answer",
                    "primary": { "label": "Ok" },
                    "secondary": { "label": "Later" },
                }
            }),
        })
        .await
        .unwrap();
    let id = raised["id"].as_i64().unwrap();
    let revision = raised["revision"].as_u64().unwrap();

    let toolset = Toolset::from_specs(&kernel.actions());
    let outcome = Dispatcher::new(Duration::from_secs(5))
        .call(
            &kernel,
            &toolset,
            &ToolCall {
                id: "1".into(),
                name: "needs_you_answer".into(),
                arguments: json!({
                    "id": id,
                    "revision": revision,
                    "choice": "primary",
                })
                .to_string(),
            },
        )
        .await;

    assert!(outcome.is_error, "{outcome:?}");
    assert!(
        outcome.text.contains("not available") || outcome.text.contains("cannot run"),
        "{}",
        outcome.text
    );

    let state = kernel.state("needs_you").unwrap();
    let decisions = state["decisions"].as_array().unwrap();
    assert_eq!(decisions.len(), 1);
    assert_eq!(decisions[0]["state"], "open");

    kernel.shutdown().await;
}

#[test]
fn history_merges_both_modules_newest_first() {
    let home = home();
    let core = NavoriCore::start(config(&home), Arc::new(Quiet)).unwrap();
    eventually(|| {
        let status = core.status();
        status
            .modules
            .iter()
            .any(|m| m.id == "needs_you" && m.state == "running")
            && status
                .modules
                .iter()
                .any(|m| m.id == "todo" && m.state == "running")
    });

    let todo = dispatch(
        &core,
        "todo",
        json!({"add": {"title": "Older todo", "due_on": null}}),
    );
    let todo_id = todo["id"].as_i64().unwrap();
    dispatch(&core, "todo", json!({"tick": {"id": todo_id}}));

    std::thread::sleep(Duration::from_millis(5));

    let raised = dispatch(
        &core,
        "needs_you",
        json!({
            "raise": {
                "key": "hist-1",
                "title": "Newer decision",
                "reason": "merge check",
                "primary": { "label": "Ok" },
                "secondary": { "label": "Later" },
            }
        }),
    );
    let id = raised["id"].as_i64().unwrap();
    let revision = raised["revision"].as_u64().unwrap();
    dispatch(
        &core,
        "needs_you",
        json!({"answer": {"id": id, "revision": revision, "choice": "primary"}}),
    );

    let history = block_on(core.needs_you_history()).unwrap();
    assert!(
        history.entries.len() >= 2,
        "expected decision and todo, got {history:?}"
    );
    assert_eq!(history.entries[0].kind, "decision");
    assert_eq!(history.entries[0].title, "Newer decision");
    assert_eq!(history.entries[0].outcome.as_deref(), Some("Ok"));
    assert!(history.entries[0].undo_request.is_none());

    let todo_entry = history
        .entries
        .iter()
        .find(|e| e.kind == "todo")
        .expect("todo history entry");
    assert_eq!(todo_entry.title, "Older todo");
    assert!(todo_entry.outcome.is_none());
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(todo_entry.undo_request.as_ref().unwrap())
            .unwrap(),
        json!({"undo": {"id": todo_id}})
    );
    assert!(
        history.entries[0].resolved_at_ms >= todo_entry.resolved_at_ms,
        "newest first: {history:?}"
    );

    core.shutdown();
}

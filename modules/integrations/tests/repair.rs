#![allow(clippy::unwrap_used)]

mod common;

use std::path::{Path, PathBuf};

use common::{
    CLAUDE, CODEX, DEV_HELPER, FIVE, RELEASE_HELPER, Rig, THREE, command, complete, group, handler,
    tree,
};
use serde_json::{Value, json};

const CLAUDE_FILE: &str = ".claude/settings.json";
const CODEX_FILE: &str = ".codex/hooks.json";
const GROK_FILE: &str = ".grok/hooks/navori.json";

const BOTH_PROFILES: &str = include_str!("fixtures/claude_both_profiles.json");

fn release_command() -> String {
    command(RELEASE_HELPER, false)
}

async fn repair(rig: &Rig, harness: &str) -> Result<Value, navori_kernel::DispatchError> {
    rig.dispatch(json!({ "repair": { "harness": harness } }))
        .await
}

fn read_json(path: &Path) -> Value {
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}

fn backups(dir: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.to_string_lossy().contains(".navori-bak-"))
        .collect()
}

fn ordered(doc: &Value) -> String {
    serde_json::to_string(doc).unwrap()
}

fn ours(doc: &Value, event: &str, command: &str) -> usize {
    doc["hooks"][event]
        .as_array()
        .map(|groups| {
            groups
                .iter()
                .flat_map(|g| g["hooks"].as_array().unwrap())
                .filter(|h| h["command"] == command)
                .count()
        })
        .unwrap_or(0)
}

async fn assert_repairs_to_installed(rig: &Rig, file: &str, harness: &str) -> Value {
    assert_eq!(rig.row(harness).await["state"], "needs_repair");
    let reply = repair(rig, harness).await.unwrap();
    let row = reply["harnesses"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["harness"] == harness)
        .unwrap();
    assert_eq!(row["state"], "installed");
    assert_eq!(rig.row(harness).await["state"], "installed");
    read_json(&rig.path(file))
}

#[tokio::test]
async fn repair_replaces_an_entry_whose_helper_moved_and_leaves_one_entry_per_event() {
    let rig = Rig::release();
    let stale = command("/Volumes/Old/Navori.app/Contents/Helpers/navori", false);
    rig.write_json(CLAUDE_FILE, &complete(&stale, &CLAUDE));

    let doc = assert_repairs_to_installed(&rig, CLAUDE_FILE, "claude").await;

    assert_eq!(doc, complete(&release_command(), &CLAUDE));
    for event in CLAUDE {
        assert_eq!(ours(&doc, event, &release_command()), 1, "{event}");
        assert_eq!(ours(&doc, event, &stale), 0, "{event}");
    }
}

#[tokio::test]
async fn repair_adds_a_missing_event() {
    let rig = Rig::release();
    rig.write_json(
        CLAUDE_FILE,
        &complete(&release_command(), &["UserPromptSubmit", "Stop"]),
    );
    let mut grok = complete(&release_command(), &FIVE);
    grok["hooks"]
        .as_object_mut()
        .unwrap()
        .shift_remove("StopFailure");
    rig.write_json(GROK_FILE, &grok);

    let claude = assert_repairs_to_installed(&rig, CLAUDE_FILE, "claude").await;
    let grok = assert_repairs_to_installed(&rig, GROK_FILE, "grok").await;

    assert_eq!(claude, complete(&release_command(), &CLAUDE));
    assert_eq!(grok, complete(&release_command(), &FIVE));
}

#[tokio::test]
async fn repair_adds_stop_failure_to_a_claude_set_of_the_three_older_events() {
    let rig = Rig::release();
    let mut old = complete(&release_command(), &THREE);
    old["hooks"]["PreToolUse"] = json!([group(vec![handler("echo foreign", 1)])]);
    rig.write_json(CLAUDE_FILE, &old);

    let doc = assert_repairs_to_installed(&rig, CLAUDE_FILE, "claude").await;

    let mut expected = complete(&release_command(), &CLAUDE);
    expected["hooks"]["PreToolUse"] = json!([group(vec![handler("echo foreign", 1)])]);
    assert_eq!(doc, expected);
    for event in THREE {
        assert_eq!(doc["hooks"][event], old["hooks"][event], "{event}");
    }
}

#[tokio::test]
async fn repair_removes_an_async_flag() {
    let rig = Rig::release();
    let mut doc = complete(&release_command(), &CLAUDE);
    doc["hooks"]["Stop"][0]["hooks"][0]["async"] = json!(true);
    rig.write_json(CLAUDE_FILE, &doc);

    let doc = assert_repairs_to_installed(&rig, CLAUDE_FILE, "claude").await;

    assert!(doc["hooks"]["Stop"][0]["hooks"][0].get("async").is_none());
    assert_eq!(doc, complete(&release_command(), &CLAUDE));
}

#[tokio::test]
async fn repair_removes_an_entry_for_an_event_install_would_not_write() {
    let rig = Rig::release();
    let mut doc = complete(&release_command(), &CLAUDE);
    doc["hooks"]["StopCancelled"] = json!([group(vec![handler(&release_command(), 2)])]);
    doc["hooks"]["PreToolUse"] = json!([group(vec![handler("echo foreign", 1)])]);
    rig.write_json(CLAUDE_FILE, &doc);

    let doc = assert_repairs_to_installed(&rig, CLAUDE_FILE, "claude").await;

    assert!(doc["hooks"].get("StopCancelled").is_none());
    assert_eq!(
        doc["hooks"]["PreToolUse"],
        json!([group(vec![handler("echo foreign", 1)])])
    );
    let mut expected = complete(&release_command(), &CLAUDE);
    expected["hooks"]["PreToolUse"] = json!([group(vec![handler("echo foreign", 1)])]);
    assert_eq!(ordered(&doc), ordered(&expected));
}

#[tokio::test]
async fn a_wrong_timeout_on_an_otherwise_correct_set_needs_repair_and_is_repaired() {
    let rig = Rig::release();
    let mut doc = complete(&release_command(), &CLAUDE);
    doc["hooks"]["Stop"][0]["hooks"][0]["timeout"] = json!(30);
    rig.write_json(CLAUDE_FILE, &doc);

    let doc = assert_repairs_to_installed(&rig, CLAUDE_FILE, "claude").await;

    assert_eq!(doc["hooks"]["Stop"][0]["hooks"][0]["timeout"], json!(2));
}

#[tokio::test]
async fn a_duplicate_handler_in_one_event_needs_repair_and_is_repaired_to_one() {
    let rig = Rig::release();
    let mut doc = complete(&release_command(), &CLAUDE);
    doc["hooks"]["Stop"]
        .as_array_mut()
        .unwrap()
        .push(group(vec![handler(&release_command(), 2)]));
    rig.write_json(CLAUDE_FILE, &doc);

    let doc = assert_repairs_to_installed(&rig, CLAUDE_FILE, "claude").await;

    assert_eq!(ours(&doc, "Stop", &release_command()), 1);
    assert_eq!(doc, complete(&release_command(), &CLAUDE));
}

#[tokio::test]
async fn repair_moves_a_handler_out_of_a_group_with_a_matcher_and_keeps_the_matcher_group() {
    let rig = Rig::release();
    let mut doc = complete(&release_command(), &CLAUDE);
    doc["hooks"]["Stop"] = json!([{
        "matcher": "Bash",
        "hooks": [handler("echo foreign", 1), handler(&release_command(), 2)]
    }]);
    rig.write_json(CLAUDE_FILE, &doc);

    let doc = assert_repairs_to_installed(&rig, CLAUDE_FILE, "claude").await;

    assert_eq!(
        doc["hooks"]["Stop"],
        json!([
            { "matcher": "Bash", "hooks": [handler("echo foreign", 1)] },
            group(vec![handler(&release_command(), 2)])
        ])
    );
}

#[tokio::test]
async fn repair_on_an_installed_harness_changes_nothing_and_writes_no_backup() {
    let rig = Rig::release();
    let body = serde_json::to_string_pretty(&complete(&release_command(), &CLAUDE)).unwrap();
    rig.write(CLAUDE_FILE, &body);
    let before = tree(rig.home.path());

    let reply = repair(&rig, "claude").await.unwrap();

    assert_eq!(reply["harnesses"][0]["state"], "installed");
    assert_eq!(tree(rig.home.path()), before);
    assert_eq!(
        std::fs::read_to_string(rig.path(CLAUDE_FILE)).unwrap(),
        body
    );
    assert!(backups(&rig.path(".claude")).is_empty());
}

#[tokio::test]
async fn repair_writes_a_backup_and_refuses_an_unparseable_file() {
    let rig = Rig::release();
    let stale = command("/Volumes/Old/Navori.app/Contents/Helpers/navori", false);
    let body = serde_json::to_string_pretty(&complete(&stale, &CLAUDE)).unwrap();
    rig.write(CLAUDE_FILE, &body);

    repair(&rig, "claude").await.unwrap();

    let found = backups(&rig.path(".claude"));
    assert_eq!(found.len(), 1);
    assert_eq!(std::fs::read_to_string(&found[0]).unwrap(), body);

    let broken = Rig::release();
    broken.write(CLAUDE_FILE, "{ \"hooks\": ");
    let before = tree(broken.home.path());

    let error = repair(&broken, "claude").await.unwrap_err().to_string();

    assert!(
        error.contains(&broken.path(CLAUDE_FILE).display().to_string()),
        "{error}"
    );
    assert!(error.contains("not valid JSON"), "{error}");
    assert_eq!(tree(broken.home.path()), before);
    assert!(backups(&broken.path(".claude")).is_empty());
}

#[tokio::test]
async fn repair_touches_only_the_running_profiles_entry() {
    let rig = Rig::dev();
    let mut before: Value = serde_json::from_str(BOTH_PROFILES).unwrap();
    before["hooks"]["Stop"]
        .as_array_mut()
        .unwrap()
        .push(group(vec![handler(&command(DEV_HELPER, true), 99)]));
    rig.write_json(CLAUDE_FILE, &before);

    repair(&rig, "claude").await.unwrap();

    let doc = read_json(&rig.path(CLAUDE_FILE));
    for event in CLAUDE {
        assert_eq!(
            serde_json::to_string(&doc["hooks"][event][0]).unwrap(),
            serde_json::to_string(&before["hooks"][event][0]).unwrap(),
            "{event}"
        );
    }
    assert_eq!(doc["hooks"]["Stop"][1]["hooks"][0]["timeout"], json!(2));
    assert_eq!(rig.row("claude").await["state"], "installed");
}

#[tokio::test]
async fn a_repair_emits_one_event() {
    let rig = Rig::release();
    let stale = command("/Volumes/Old/Navori.app/Contents/Helpers/navori", false);
    rig.write_json(CLAUDE_FILE, &complete(&stale, &CLAUDE));

    repair(&rig, "claude").await.unwrap();
    assert!(repair(&rig, "grok").await.is_err());

    let events = rig.kernel.recent_events(20).unwrap();
    let repaired: Vec<_> = events
        .iter()
        .filter(|e| e.kind == "integrations.hook_repaired")
        .collect();
    assert_eq!(repaired.len(), 1);
    assert!(repaired[0].summary.contains("Claude Code"));
    assert!(repaired[0].summary.contains("Repaired"));
    assert_eq!(format!("{:?}", repaired[0].severity), "Info");
    let failed: Vec<_> = events
        .iter()
        .filter(|e| e.kind == "integrations.hook_failed")
        .collect();
    assert_eq!(failed.len(), 1);
    assert!(failed[0].summary.contains("repair"));
}

#[tokio::test]
async fn a_repair_that_finds_nothing_to_do_still_emits_one_event_with_its_own_sentence() {
    let rig = Rig::release();
    rig.write_json(CLAUDE_FILE, &complete(&release_command(), &CLAUDE));

    repair(&rig, "claude").await.unwrap();

    let events = rig.kernel.recent_events(20).unwrap();
    let repaired: Vec<_> = events
        .iter()
        .filter(|e| e.kind == "integrations.hook_repaired")
        .collect();
    assert_eq!(repaired.len(), 1);
    assert!(repaired[0].summary.contains("already"));
}

#[tokio::test]
async fn repair_leaves_a_double_quoted_hand_edited_entry_alone_as_foreign() {
    let rig = Rig::release();
    let hand = "\"/Applications/Navori.app/Contents/Helpers/navori\" hook";
    let mut doc = complete(&release_command(), &CLAUDE);
    doc["hooks"]["Stop"] = json!([group(vec![handler(hand, 2)])]);
    rig.write_json(CLAUDE_FILE, &doc);

    repair(&rig, "claude").await.unwrap();

    let doc = read_json(&rig.path(CLAUDE_FILE));
    assert_eq!(doc["hooks"]["Stop"][0], group(vec![handler(hand, 2)]));
    assert_eq!(ours(&doc, "Stop", &release_command()), 1);
    assert_eq!(rig.row("claude").await["state"], "installed");
}

#[tokio::test]
async fn repairing_a_codex_file_from_before_interrupt_adds_only_interrupt() {
    let rig = Rig::release();
    let old = complete(&release_command(), &THREE);
    rig.write_json(CODEX_FILE, &old);
    assert_eq!(rig.row("codex").await["state"], "needs_repair");

    repair(&rig, "codex").await.unwrap();

    let doc = read_json(&rig.path(CODEX_FILE));
    for event in THREE {
        assert_eq!(doc["hooks"][event], old["hooks"][event], "{event}");
    }
    assert_eq!(
        doc["hooks"]["Interrupt"],
        json!([group(vec![handler(&release_command(), 2)])])
    );
    assert_eq!(rig.row("codex").await["state"], "installed");
}

#[tokio::test]
async fn codex_repair_and_remove_behave_like_claude() {
    let rig = Rig::release();
    let foreign = handler("echo foreign", 1);
    rig.write_json(
        CODEX_FILE,
        &json!({ "hooks": {
            "Stop": [group(vec![foreign.clone()])],
            "UserPromptSubmit": [group(vec![handler(&release_command(), 9)])],
        } }),
    );

    repair(&rig, "codex").await.unwrap();

    let doc = read_json(&rig.path(CODEX_FILE));
    let expected = complete(&release_command(), &CODEX);
    for event in CODEX {
        let ours = expected["hooks"][event][0].clone();
        assert!(
            doc["hooks"][event].as_array().unwrap().contains(&ours),
            "{event}"
        );
    }
    assert_eq!(rig.row("codex").await["state"], "installed");

    rig.dispatch(json!({ "remove": { "harness": "codex" } }))
        .await
        .unwrap();

    let doc = read_json(&rig.path(CODEX_FILE));
    assert_eq!(doc["hooks"], json!({ "Stop": [group(vec![foreign])] }));
    assert_eq!(rig.row("codex").await["state"], "not_installed");
}

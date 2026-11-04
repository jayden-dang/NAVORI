//! `navori doctor`'s integrations row, read through a real Core over a temp home.
#![allow(clippy::unwrap_used)]

use std::sync::Arc;

use navori_ffi::{CheckView, CoreConfig, CoreSnapshot, NavoriCore, SnapshotListener};
use serde_json::Value;

struct Ignore;

impl SnapshotListener for Ignore {
    fn on_snapshot(&self, _: CoreSnapshot) {}
}

/// A short home path so the socket stays under the 104-byte `sun_path` limit.
fn home() -> tempfile::TempDir {
    tempfile::Builder::new()
        .prefix("nv")
        .tempdir_in("/tmp")
        .unwrap()
}

fn start(home: &tempfile::TempDir) -> Arc<NavoriCore> {
    NavoriCore::start(
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
        },
        Arc::new(Ignore),
    )
    .unwrap()
}

fn mkdir(home: &tempfile::TempDir, dir: &str) {
    std::fs::create_dir_all(home.path().join(dir)).unwrap();
}

fn dispatch(core: &NavoriCore, action: &str) -> String {
    futures::executor::block_on(core.dispatch("integrations".to_owned(), action.to_owned()))
        .unwrap()
}

fn row(core: &NavoriCore) -> CheckView {
    core.doctor()
        .into_iter()
        .find(|check| check.name == "integrations")
        .unwrap()
}

fn break_claude_hooks(home: &tempfile::TempDir) {
    let file = home.path().join(".claude/settings.json");
    let mut doc: Value = serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
    doc["hooks"].as_object_mut().unwrap().remove("SessionEnd");
    std::fs::write(&file, serde_json::to_vec(&doc).unwrap()).unwrap();
}

#[test]
fn doctor_has_an_integrations_row_between_awake_and_logs() {
    let home = home();
    let core = start(&home);
    let names: Vec<_> = core.doctor().into_iter().map(|check| check.name).collect();
    let at = names
        .iter()
        .position(|name| name == "integrations")
        .unwrap();
    assert_eq!(names[at - 1], "awake");
    assert_eq!(names[at + 1], "logs");
    core.shutdown();
}

#[test]
fn the_settings_empty_state_names_every_doctor_row() {
    let home = home();
    let core = start(&home);
    let names: Vec<_> = core.doctor().into_iter().map(|check| check.name).collect();
    core.shutdown();
    let view = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../apps/macos/Navori/Features/Settings/SettingsView.swift");
    let source = std::fs::read_to_string(view).unwrap();
    let sentence = source
        .lines()
        .find_map(|line| line.trim().strip_prefix("Text(\"Run the checks to verify "))
        .unwrap();
    let sentence = sentence.split('"').next().unwrap().to_lowercase();
    for name in names {
        assert!(
            sentence.contains(&name),
            "the sentence misses `{name}`: {sentence}"
        );
    }
}

#[test]
fn the_existing_rows_keep_their_order() {
    let home = home();
    let core = start(&home);
    let names: Vec<_> = core.doctor().into_iter().map(|check| check.name).collect();
    assert_eq!(
        names,
        [
            "config",
            "database",
            "ipc",
            "modules",
            "awake",
            "integrations",
            "logs",
            "ports"
        ]
    );
    core.shutdown();
}

#[test]
fn the_row_is_ok_when_every_harness_is_not_found() {
    let home = home();
    let core = start(&home);
    let row = row(&core);
    assert!(row.ok, "{row:?}");
    core.shutdown();
}

#[test]
fn the_row_is_ok_when_a_harness_is_not_installed() {
    let home = home();
    mkdir(&home, ".claude");
    let core = start(&home);
    let row = row(&core);
    assert!(row.ok, "{row:?}");
    assert!(row.detail.contains("claude: not installed"), "{row:?}");
    core.shutdown();
}

#[test]
fn the_row_fails_when_a_harness_needs_repair_and_names_it() {
    let home = home();
    mkdir(&home, ".claude");
    let core = start(&home);
    dispatch(&core, r#"{"install":{"harness":"claude"}}"#);
    break_claude_hooks(&home);
    let row = row(&core);
    assert!(!row.ok, "{row:?}");
    assert!(row.detail.contains("claude: needs repair"), "{row:?}");
    core.shutdown();
}

#[test]
fn the_row_fails_when_a_hook_file_is_unreadable_and_names_it() {
    let home = home();
    mkdir(&home, ".grok/hooks");
    std::fs::write(home.path().join(".grok/hooks/navori.json"), "{ \"hooks\": ").unwrap();
    let core = start(&home);
    let row = row(&core);
    assert!(!row.ok, "{row:?}");
    assert!(row.detail.contains("grok: unreadable"), "{row:?}");
    core.shutdown();
}

#[test]
fn a_codex_that_needs_repair_fails_the_row() {
    let home = home();
    mkdir(&home, ".codex");
    std::fs::write(
        home.path().join(".codex/hooks.json"),
        r#"{"hooks":{"Stop":[{"hooks":[{"type":"command","command":"'/old/Navori.app/Contents/Helpers/navori' --profile dev hook","timeout":2}]}]}}"#,
    )
    .unwrap();
    let core = start(&home);
    let row = row(&core);
    assert!(!row.ok, "{row:?}");
    assert!(row.detail.contains("codex: needs repair"), "{row:?}");
    core.shutdown();
}

#[test]
fn the_detail_names_all_three_harnesses_with_their_states() {
    let home = home();
    mkdir(&home, ".claude");
    mkdir(&home, ".codex");
    let core = start(&home);
    dispatch(&core, r#"{"install":{"harness":"claude"}}"#);
    assert_eq!(
        row(&core).detail,
        "claude: installed, codex: not installed, grok: not found"
    );
    core.shutdown();
}

#[test]
fn the_detail_carries_no_paths_or_file_contents() {
    let home = home();
    mkdir(&home, ".claude");
    std::fs::write(home.path().join(".claude/settings.json"), "SECRET {").unwrap();
    let core = start(&home);
    let row = row(&core);
    assert!(!row.detail.contains("SECRET"), "{row:?}");
    assert!(!row.detail.contains('/'), "{row:?}");
    core.shutdown();
}

#[test]
fn doctor_and_the_status_action_agree_on_each_harness() {
    let home = home();
    mkdir(&home, ".claude");
    mkdir(&home, ".codex");
    mkdir(&home, ".grok/hooks");
    std::fs::write(home.path().join(".grok/hooks/navori.json"), "[").unwrap();
    let core = start(&home);
    dispatch(&core, r#"{"install":{"harness":"claude"}}"#);
    break_claude_hooks(&home);

    let reply: Value = serde_json::from_str(&dispatch(&core, r#""status""#)).unwrap();
    let from_action: Vec<String> = reply["harnesses"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| {
            format!(
                "{}: {}",
                row["harness"].as_str().unwrap(),
                row["state"].as_str().unwrap().replace('_', " ")
            )
        })
        .collect();
    assert_eq!(row(&core).detail, from_action.join(", "));
    core.shutdown();
}

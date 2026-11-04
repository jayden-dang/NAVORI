#![allow(clippy::unwrap_used)]

mod common;

use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};

use common::{CLAUDE, DEV_HELPER, FIVE, RELEASE_HELPER, Rig, command, complete, tree};
use serde_json::{Value, json};

const CLAUDE_FILE: &str = ".claude/settings.json";
const GROK_FILE: &str = ".grok/hooks/navori.json";

const FOREIGN: &str = include_str!("fixtures/claude_foreign.json");
const BOTH_PROFILES: &str = include_str!("fixtures/claude_both_profiles.json");

async fn install(rig: &Rig, harness: &str) -> Result<Value, navori_kernel::DispatchError> {
    rig.dispatch(json!({ "install": { "harness": harness } }))
        .await
}

fn read_json(path: &Path) -> Value {
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}

fn keys(value: &Value) -> Vec<&str> {
    value
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect()
}

fn backups(dir: &Path) -> Vec<PathBuf> {
    let mut found: Vec<_> = std::fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.to_string_lossy().contains(".navori-bak-"))
        .collect();
    found.sort();
    found
}

fn handlers(doc: &Value) -> Vec<&Value> {
    doc["hooks"]
        .as_object()
        .unwrap()
        .values()
        .flat_map(|groups| groups.as_array().unwrap())
        .flat_map(|group| group["hooks"].as_array().unwrap())
        .collect()
}

#[tokio::test]
async fn installing_into_a_claude_file_adds_six_entries_and_keeps_foreign_hooks_and_key_order() {
    let rig = Rig::release();
    rig.write(CLAUDE_FILE, FOREIGN);

    let reply = install(&rig, "claude").await.unwrap();

    let doc = read_json(&rig.path(CLAUDE_FILE));
    assert_eq!(keys(&doc), ["model", "hooks", "permissions", "theme"]);
    assert_eq!(
        keys(&doc["hooks"]),
        [
            "PreToolUse",
            "Stop",
            "UserPromptSubmit",
            "StopFailure",
            "SubagentStart",
            "SubagentStop",
            "SessionEnd"
        ]
    );
    let expected = complete(&command(RELEASE_HELPER, false), &CLAUDE);
    for event in CLAUDE {
        let ours = expected["hooks"][event][0].clone();
        assert!(
            doc["hooks"][event].as_array().unwrap().contains(&ours),
            "{event}"
        );
    }
    assert_eq!(
        doc["hooks"]["StopFailure"][0]["hooks"][0]["timeout"],
        json!(2)
    );
    let claude = reply["harnesses"][0].clone();
    assert_eq!(claude["harness"], "claude");
    assert_eq!(claude["state"], "installed");
}

#[tokio::test]
async fn foreign_hooks_still_present_after_install() {
    let rig = Rig::release();
    rig.write(CLAUDE_FILE, FOREIGN);
    let before: Value = serde_json::from_str(FOREIGN).unwrap();

    install(&rig, "claude").await.unwrap();

    let doc = read_json(&rig.path(CLAUDE_FILE));
    assert_eq!(doc["hooks"]["PreToolUse"], before["hooks"]["PreToolUse"]);
    assert_eq!(doc["hooks"]["Stop"][0], before["hooks"]["Stop"][0]);
    assert_eq!(doc["hooks"]["Stop"].as_array().unwrap().len(), 2);
    assert_eq!(doc["permissions"], before["permissions"]);
}

#[tokio::test]
async fn installing_for_grok_creates_navori_json_with_five_events() {
    let rig = Rig::release();
    rig.make_dir(".grok/hooks");

    install(&rig, "grok").await.unwrap();

    let doc = read_json(&rig.path(GROK_FILE));
    assert_eq!(doc, complete(&command(RELEASE_HELPER, false), &FIVE));
    let mode = std::fs::metadata(rig.path(GROK_FILE)).unwrap().mode() & 0o777;
    assert_eq!(mode, 0o600);
}

#[tokio::test]
async fn a_second_install_for_grok_on_the_dev_profile_changes_nothing_and_writes_no_backup() {
    let rig = Rig::dev();
    rig.make_dir(".grok/hooks");
    install(&rig, "grok").await.unwrap();
    let before = tree(rig.home.path());
    let body = std::fs::read_to_string(rig.path(GROK_FILE)).unwrap();

    let reply = install(&rig, "grok").await.unwrap();

    assert_eq!(tree(rig.home.path()), before);
    assert_eq!(std::fs::read_to_string(rig.path(GROK_FILE)).unwrap(), body);
    assert!(backups(&rig.path(".grok/hooks")).is_empty());
    assert_eq!(reply["harnesses"][2]["state"], "installed");
}

#[tokio::test]
async fn installing_for_grok_creates_the_hooks_directory_when_grok_exists() {
    let rig = Rig::release();
    rig.make_dir(".grok");

    install(&rig, "grok").await.unwrap();

    assert!(rig.path(GROK_FILE).is_file());
}

#[tokio::test]
async fn installing_never_creates_a_harness_config_directory() {
    let rig = Rig::release();

    for harness in ["claude", "codex", "grok"] {
        assert!(install(&rig, harness).await.is_err(), "{harness}");
    }

    assert!(tree(rig.home.path()).is_empty());
}

#[tokio::test]
async fn a_backup_is_written_beside_the_file_before_the_change() {
    let rig = Rig::release();
    rig.write(CLAUDE_FILE, FOREIGN);

    install(&rig, "claude").await.unwrap();

    let found = backups(&rig.path(".claude"));
    assert_eq!(found.len(), 1);
    let name = found[0].file_name().unwrap().to_string_lossy().into_owned();
    let seconds = name.strip_prefix("settings.json.navori-bak-").unwrap();
    assert!(seconds.parse::<u64>().is_ok(), "{name}");
    assert_eq!(std::fs::read_to_string(&found[0]).unwrap(), FOREIGN);
}

#[tokio::test]
async fn installing_when_the_file_is_already_correct_changes_nothing_and_writes_no_backup() {
    let rig = Rig::release();
    let body =
        serde_json::to_string_pretty(&complete(&command(RELEASE_HELPER, false), &CLAUDE)).unwrap();
    rig.write(CLAUDE_FILE, &body);
    let before = tree(rig.home.path());

    let reply = install(&rig, "claude").await.unwrap();

    assert_eq!(tree(rig.home.path()), before);
    assert_eq!(
        std::fs::read_to_string(rig.path(CLAUDE_FILE)).unwrap(),
        body
    );
    assert!(backups(&rig.path(".claude")).is_empty());
    assert_eq!(reply["harnesses"][0]["state"], "installed");
}

#[tokio::test]
async fn installing_into_an_unparseable_file_changes_nothing_writes_no_backup_and_names_the_file() {
    let rig = Rig::release();
    rig.write(CLAUDE_FILE, "{ \"hooks\": ");
    let before = tree(rig.home.path());

    let error = install(&rig, "claude").await.unwrap_err().to_string();

    assert!(
        error.contains(&rig.path(CLAUDE_FILE).display().to_string()),
        "{error}"
    );
    assert!(error.contains("not valid JSON"), "{error}");
    assert_eq!(tree(rig.home.path()), before);
    assert!(backups(&rig.path(".claude")).is_empty());
}

struct Restore(PathBuf);

impl Drop for Restore {
    fn drop(&mut self) {
        let _ = std::fs::set_permissions(&self.0, std::fs::Permissions::from_mode(0o755));
    }
}

#[tokio::test]
async fn a_blocked_backup_leaves_the_original_alone() {
    let rig = Rig::release();
    rig.write(CLAUDE_FILE, FOREIGN);
    let dir = rig.path(".claude");
    let _restore = Restore(dir.clone());
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o555)).unwrap();
    if std::fs::write(dir.join("probe"), "x").is_ok() {
        eprintln!("skipped: this user can write into a read-only directory (running as root)");
        return;
    }

    let error = install(&rig, "claude").await;

    assert!(error.is_err());
    assert_eq!(
        std::fs::read_to_string(rig.path(CLAUDE_FILE)).unwrap(),
        FOREIGN
    );
    assert!(backups(&dir).is_empty());
}

struct Unlock(PathBuf);

impl Drop for Unlock {
    fn drop(&mut self) {
        let _ = std::process::Command::new("chflags")
            .arg("nouchg")
            .arg(&self.0)
            .status();
    }
}

#[tokio::test]
async fn a_write_that_fails_after_the_backup_leaves_the_original_and_a_complete_backup() {
    let rig = Rig::release();
    rig.write(CLAUDE_FILE, FOREIGN);
    let file = rig.path(CLAUDE_FILE);
    let _unlock = Unlock(file.clone());
    let locked = std::process::Command::new("chflags")
        .arg("uchg")
        .arg(&file)
        .status()
        .unwrap();
    assert!(locked.success());

    let error = install(&rig, "claude").await;

    assert!(error.is_err());
    assert_eq!(std::fs::read_to_string(&file).unwrap(), FOREIGN);
    let dir = rig.path(".claude");
    let found = backups(&dir);
    assert_eq!(found.len(), 1);
    assert_eq!(std::fs::read_to_string(&found[0]).unwrap(), FOREIGN);
    let mut names: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    assert_eq!(names.len(), 2, "stray files: {names:?}");
}

#[tokio::test]
async fn a_symlinked_hook_file_is_written_through_and_the_link_stays() {
    let rig = Rig::release();
    rig.make_dir(".claude");
    rig.write("dotfiles/settings.json", FOREIGN);
    let link = rig.path(CLAUDE_FILE);
    std::os::unix::fs::symlink(rig.path("dotfiles/settings.json"), &link).unwrap();

    install(&rig, "claude").await.unwrap();

    assert!(
        std::fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    let doc = read_json(&rig.path("dotfiles/settings.json"));
    assert_eq!(doc["hooks"]["Stop"].as_array().unwrap().len(), 2);
    assert_eq!(backups(&rig.path("dotfiles")).len(), 1);
    assert!(backups(&rig.path(".claude")).is_empty());
}

#[tokio::test]
async fn the_written_file_keeps_the_original_mode() {
    let rig = Rig::release();
    rig.write(CLAUDE_FILE, FOREIGN);
    std::fs::set_permissions(
        rig.path(CLAUDE_FILE),
        std::fs::Permissions::from_mode(0o640),
    )
    .unwrap();

    install(&rig, "claude").await.unwrap();

    let mode = std::fs::metadata(rig.path(CLAUDE_FILE)).unwrap().mode() & 0o777;
    assert_eq!(mode, 0o640);
}

#[tokio::test]
async fn the_release_command_is_the_single_quoted_helper_path_then_hook() {
    let rig = Rig::release();
    rig.make_dir(".claude");

    install(&rig, "claude").await.unwrap();

    let doc = read_json(&rig.path(CLAUDE_FILE));
    for handler in handlers(&doc) {
        assert_eq!(
            handler["command"],
            "'/Applications/Navori.app/Contents/Helpers/navori' hook"
        );
    }
}

#[tokio::test]
async fn a_helper_path_with_a_space_and_a_quote_is_quoted_safely() {
    let rig = Rig::new(false, "/Apps/It's Mine/Navori.app/Contents/Helpers/navori");
    rig.make_dir(".claude");

    install(&rig, "claude").await.unwrap();

    let doc = read_json(&rig.path(CLAUDE_FILE));
    assert_eq!(
        handlers(&doc)[0]["command"],
        "'/Apps/It'\\''s Mine/Navori.app/Contents/Helpers/navori' hook"
    );
}

#[tokio::test]
async fn the_dev_command_carries_profile_dev_before_hook() {
    let rig = Rig::dev();
    rig.make_dir(".claude");

    install(&rig, "claude").await.unwrap();

    let doc = read_json(&rig.path(CLAUDE_FILE));
    for handler in handlers(&doc) {
        assert_eq!(
            handler["command"],
            "'/Applications/Navori Dev.app/Contents/Helpers/navori' --profile dev hook"
        );
    }
}

#[tokio::test]
async fn no_entry_uses_async_and_timeouts_are_2_and_1_seconds() {
    let rig = Rig::release();
    rig.make_dir(".claude");
    rig.make_dir(".grok");

    install(&rig, "claude").await.unwrap();
    install(&rig, "grok").await.unwrap();

    for file in [CLAUDE_FILE, GROK_FILE] {
        let doc = read_json(&rig.path(file));
        for (event, groups) in doc["hooks"].as_object().unwrap() {
            let handler = &groups[0]["hooks"][0];
            let expected = if event == "SessionEnd" { 1 } else { 2 };
            assert_eq!(handler["timeout"], json!(expected), "{event}");
            assert!(handler.get("async").is_none(), "{event}");
            assert_eq!(groups[0].as_object().unwrap().len(), 1);
        }
    }
}

#[tokio::test]
async fn dev_install_leaves_a_release_entry_byte_identical() {
    let rig = Rig::dev();
    rig.write(CLAUDE_FILE, BOTH_PROFILES);
    let before: Value = serde_json::from_str(BOTH_PROFILES).unwrap();

    install(&rig, "claude").await.unwrap();

    let doc = read_json(&rig.path(CLAUDE_FILE));
    for event in CLAUDE {
        assert_eq!(
            serde_json::to_string(&doc["hooks"][event][0]).unwrap(),
            serde_json::to_string(&before["hooks"][event][0]).unwrap(),
            "{event}"
        );
        let dev = &doc["hooks"][event][1]["hooks"][0]["command"];
        assert_eq!(dev, &json!(command(DEV_HELPER, true)), "{event}");
    }
}

#[tokio::test]
async fn an_install_emits_one_event_naming_the_harness_and_a_failure_emits_an_error_event() {
    let rig = Rig::release();
    rig.make_dir(".claude");
    install(&rig, "claude").await.unwrap();
    assert!(install(&rig, "grok").await.is_err());

    let events = rig.kernel.recent_events(20).unwrap();
    let installed: Vec<_> = events
        .iter()
        .filter(|e| e.kind == "integrations.hook_installed")
        .collect();
    assert_eq!(installed.len(), 1);
    assert!(installed[0].summary.contains("Claude Code"));
    assert_eq!(format!("{:?}", installed[0].severity), "Info");
    let failed: Vec<_> = events
        .iter()
        .filter(|e| e.kind == "integrations.hook_failed")
        .collect();
    assert_eq!(failed.len(), 1);
    assert!(failed[0].summary.contains("Grok Build"));
    assert_eq!(format!("{:?}", failed[0].severity), "Error");
    assert!(
        !failed[0]
            .summary
            .contains(&rig.home.path().display().to_string())
    );
}

#[tokio::test]
async fn only_the_three_hook_file_paths_and_their_backups_are_written() {
    let rig = Rig::release();
    rig.write(CLAUDE_FILE, FOREIGN);
    rig.write(".claude/other.json", "{}");
    rig.make_dir(".grok");
    rig.write(".codex/hooks.json", "{}");
    rig.write("notes.txt", "keep");
    let before = tree(rig.home.path());

    install(&rig, "claude").await.unwrap();
    install(&rig, "grok").await.unwrap();
    install(&rig, "codex").await.unwrap_err();

    let after = tree(rig.home.path());
    let changed: Vec<&String> = after.iter().filter(|line| !before.contains(line)).collect();
    assert!(!changed.is_empty());
    for line in changed {
        let path = line.split(' ').next().unwrap();
        let allowed = [
            ".claude/settings.json",
            ".grok/hooks/navori.json",
            ".grok/hooks/",
        ];
        assert!(allowed.iter().any(|a| path.starts_with(a)), "{path}");
    }
    assert_eq!(
        std::fs::read_to_string(rig.path(".codex/hooks.json")).unwrap(),
        "{}"
    );
}

#[tokio::test]
async fn an_empty_helper_path_writes_nothing_and_returns_an_error() {
    let rig = Rig::new(false, "");
    rig.make_dir(".claude");
    let before = tree(rig.home.path());

    assert!(install(&rig, "claude").await.is_err());

    assert_eq!(tree(rig.home.path()), before);
}

#[tokio::test]
async fn installing_for_codex_changes_nothing_and_keeps_the_interim_reason() {
    let rig = Rig::release();
    rig.write(".codex/hooks.json", "{}");
    let before = tree(rig.home.path());

    assert!(install(&rig, "codex").await.is_err());

    assert_eq!(tree(rig.home.path()), before);
    let row = rig.row("codex").await;
    assert_eq!(row["reason"], "The Codex timeout unit is not verified yet.");
    assert_eq!(row["offers"], json!([]));
}

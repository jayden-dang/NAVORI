#![allow(clippy::unwrap_used)]

mod common;

use std::path::{Path, PathBuf};

use common::{
    CLAUDE, DEV_HELPER, FIVE, RELEASE_HELPER, Rig, command, complete, group, handler,
    legacy_command, tree,
};
use serde_json::{Value, json};

const CLAUDE_FILE: &str = ".claude/settings.json";
const GROK_FILE: &str = ".grok/hooks/navori.json";

fn release_command() -> String {
    command(RELEASE_HELPER, false)
}

fn foreign() -> Value {
    handler("echo foreign", 1)
}

async fn remove(rig: &Rig, harness: &str) -> Result<Value, navori_kernel::DispatchError> {
    rig.dispatch(json!({ "remove": { "harness": harness } }))
        .await
}

fn read_text(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap()
}

fn read_json(path: &Path) -> Value {
    serde_json::from_str(&read_text(path)).unwrap()
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

#[tokio::test]
async fn removing_from_claude_drops_only_navoris_entries_and_keeps_every_foreign_hook_and_key_order()
 {
    let rig = Rig::release();
    let ours = release_command();
    let before = json!({
        "model": "opus",
        "hooks": {
            "PreToolUse": [{ "matcher": "Bash", "hooks": [foreign()] }],
            "UserPromptSubmit": [group(vec![handler(&ours, 2)])],
            "Notification": [group(vec![foreign()])],
            "Stop": [
                { "matcher": "m", "hooks": [handler(&ours, 2), foreign()] },
                group(vec![handler(&ours, 2)])
            ],
            "StopFailure": [group(vec![handler(&ours, 2)])],
            "PostToolUse": [group(vec![foreign()])],
            "SessionEnd": [group(vec![handler(&ours, 1)])]
        },
        "theme": "dark"
    });
    rig.write_json(CLAUDE_FILE, &before);

    remove(&rig, "claude").await.unwrap();

    let expected = json!({
        "model": "opus",
        "hooks": {
            "PreToolUse": [{ "matcher": "Bash", "hooks": [foreign()] }],
            "Notification": [group(vec![foreign()])],
            "Stop": [{ "matcher": "m", "hooks": [foreign()] }],
            "PostToolUse": [group(vec![foreign()])]
        },
        "theme": "dark"
    });
    assert_eq!(
        ordered(&read_json(&rig.path(CLAUDE_FILE))),
        ordered(&expected)
    );
    assert_eq!(rig.row("claude").await["state"], "not_installed");
    assert_eq!(backups(&rig.path(".claude")).len(), 1);
}

#[tokio::test]
async fn an_event_array_emptied_by_removal_is_dropped_and_a_foreign_one_stays() {
    let rig = Rig::release();
    let mut before = complete(&release_command(), &CLAUDE);
    before["hooks"]["Notification"] = json!([]);
    before["hooks"]["Stop"]
        .as_array_mut()
        .unwrap()
        .push(json!({ "hooks": [] }));
    rig.write_json(CLAUDE_FILE, &before);

    remove(&rig, "claude").await.unwrap();

    let doc = read_json(&rig.path(CLAUDE_FILE));
    assert_eq!(
        ordered(&doc),
        ordered(&json!({ "hooks": { "Stop": [{ "hooks": [] }], "Notification": [] } }))
    );
}

#[tokio::test]
async fn a_hooks_object_emptied_by_removal_is_dropped_but_the_file_stays() {
    let rig = Rig::release();
    let mut before = complete(&release_command(), &CLAUDE);
    before["model"] = json!("opus");
    rig.write_json(CLAUDE_FILE, &before);

    remove(&rig, "claude").await.unwrap();

    assert_eq!(
        read_json(&rig.path(CLAUDE_FILE)),
        json!({ "model": "opus" })
    );

    let only = Rig::release();
    only.write_json(CLAUDE_FILE, &complete(&release_command(), &CLAUDE));
    remove(&only, "claude").await.unwrap();
    assert_eq!(read_json(&only.path(CLAUDE_FILE)), json!({}));
}

#[tokio::test]
async fn an_empty_hooks_object_that_was_already_there_stays() {
    let rig = Rig::release();
    let body = "{\n  \"hooks\": {},\n  \"model\": \"opus\"\n}";
    rig.write(CLAUDE_FILE, body);
    let before = tree(rig.home.path());

    remove(&rig, "claude").await.unwrap();

    assert_eq!(read_text(&rig.path(CLAUDE_FILE)), body);
    assert_eq!(tree(rig.home.path()), before);
}

#[tokio::test]
async fn removing_from_grok_deletes_navori_json_after_a_backup_when_it_holds_only_navori_entries() {
    let rig = Rig::release();
    let before = complete(&release_command(), &FIVE);
    rig.write_json(GROK_FILE, &before);
    let body = read_text(&rig.path(GROK_FILE));

    let reply = remove(&rig, "grok").await.unwrap();

    assert!(!rig.path(GROK_FILE).exists());
    let found = backups(&rig.path(".grok/hooks"));
    assert_eq!(found.len(), 1);
    assert_eq!(read_text(&found[0]), body);
    let row = reply["harnesses"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["harness"] == "grok")
        .unwrap();
    assert_eq!(row["state"], "not_installed");
}

#[tokio::test]
async fn removing_from_grok_keeps_the_file_when_it_holds_anything_else() {
    let rig = Rig::release();
    let mut with_hook = complete(&release_command(), &FIVE);
    with_hook["hooks"]["Stop"]
        .as_array_mut()
        .unwrap()
        .push(group(vec![foreign()]));
    rig.write_json(GROK_FILE, &with_hook);

    remove(&rig, "grok").await.unwrap();

    assert_eq!(
        read_json(&rig.path(GROK_FILE)),
        json!({ "hooks": { "Stop": [group(vec![foreign()])] } })
    );
    assert_eq!(backups(&rig.path(".grok/hooks")).len(), 1);

    let keyed = Rig::release();
    let mut with_key = complete(&release_command(), &FIVE);
    with_key["version"] = json!(1);
    keyed.write_json(GROK_FILE, &with_key);

    remove(&keyed, "grok").await.unwrap();

    assert_eq!(read_json(&keyed.path(GROK_FILE)), json!({ "version": 1 }));
}

#[tokio::test]
async fn removing_when_nothing_of_the_profile_is_there_changes_nothing_and_writes_no_backup() {
    let rig = Rig::release();
    let other = command(DEV_HELPER, true);
    let mut claude = complete(&other, &CLAUDE);
    claude["hooks"]["Stop"]
        .as_array_mut()
        .unwrap()
        .push(group(vec![foreign()]));
    rig.write_json(CLAUDE_FILE, &claude);
    rig.make_dir(".grok");
    let before = tree(rig.home.path());

    remove(&rig, "claude").await.unwrap();
    remove(&rig, "grok").await.unwrap();

    assert_eq!(tree(rig.home.path()), before);
    assert!(backups(&rig.path(".claude")).is_empty());
    assert!(!rig.path(".grok/hooks").exists());
}

#[tokio::test]
async fn remove_refuses_an_unparseable_file() {
    let rig = Rig::release();
    rig.write(CLAUDE_FILE, "{ \"hooks\": ");
    rig.write(GROK_FILE, "not json");
    let before = tree(rig.home.path());

    let error = remove(&rig, "claude").await.unwrap_err().to_string();
    assert!(
        error.contains(&rig.path(CLAUDE_FILE).display().to_string()),
        "{error}"
    );
    assert!(error.contains("not valid JSON"), "{error}");
    assert!(remove(&rig, "grok").await.is_err());

    assert_eq!(tree(rig.home.path()), before);
}

#[tokio::test]
async fn remove_writes_through_a_symlink() {
    let rig = Rig::release();
    rig.make_dir(".claude");
    let mut before = complete(&release_command(), &CLAUDE);
    before["model"] = json!("opus");
    rig.write_json("dotfiles/settings.json", &before);
    let link = rig.path(CLAUDE_FILE);
    std::os::unix::fs::symlink(rig.path("dotfiles/settings.json"), &link).unwrap();

    remove(&rig, "claude").await.unwrap();

    assert!(
        std::fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(
        read_json(&rig.path("dotfiles/settings.json")),
        json!({ "model": "opus" })
    );
    assert_eq!(backups(&rig.path("dotfiles")).len(), 1);
    assert!(backups(&rig.path(".claude")).is_empty());
}

#[tokio::test]
async fn removing_from_a_symlinked_grok_file_keeps_the_link_and_writes_an_empty_document_through_it()
 {
    let rig = Rig::release();
    rig.make_dir(".grok/hooks");
    rig.write_json("dotfiles/navori.json", &complete(&release_command(), &FIVE));
    let body = read_text(&rig.path("dotfiles/navori.json"));
    let link = rig.path(GROK_FILE);
    std::os::unix::fs::symlink(rig.path("dotfiles/navori.json"), &link).unwrap();

    remove(&rig, "grok").await.unwrap();

    assert!(
        std::fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(read_json(&rig.path("dotfiles/navori.json")), json!({}));
    let found = backups(&rig.path("dotfiles"));
    assert_eq!(found.len(), 1);
    assert_eq!(read_text(&found[0]), body);
    assert!(backups(&rig.path(".grok/hooks")).is_empty());
    assert_eq!(rig.row("grok").await["state"], "not_installed");

    remove(&rig, "grok").await.unwrap();

    assert_eq!(backups(&rig.path("dotfiles")).len(), 1);
    assert_eq!(read_json(&rig.path("dotfiles/navori.json")), json!({}));
}

#[tokio::test]
async fn dev_remove_leaves_a_release_entry_alone() {
    let rig = Rig::dev();
    let release = release_command();
    let dev = command(DEV_HELPER, true);
    let mut doc = complete(&release, &CLAUDE);
    for event in CLAUDE {
        doc["hooks"][event]
            .as_array_mut()
            .unwrap()
            .push(group(vec![handler(&dev, 2)]));
    }
    rig.write_json(CLAUDE_FILE, &doc);

    remove(&rig, "claude").await.unwrap();

    assert_eq!(
        read_json(&rig.path(CLAUDE_FILE)),
        complete(&release, &CLAUDE)
    );
}

#[tokio::test]
async fn a_removal_emits_one_event() {
    let rig = Rig::release();
    rig.write_json(CLAUDE_FILE, &complete(&release_command(), &CLAUDE));

    remove(&rig, "claude").await.unwrap();
    assert!(remove(&rig, "grok").await.is_err());

    let events = rig.kernel.recent_events(20).unwrap();
    let removed: Vec<_> = events
        .iter()
        .filter(|e| e.kind == "integrations.hook_removed")
        .collect();
    assert_eq!(removed.len(), 1);
    assert!(removed[0].summary.contains("Claude Code"));
    assert!(removed[0].summary.contains("Removed"));
    assert_eq!(format!("{:?}", removed[0].severity), "Info");
    let failed: Vec<_> = events
        .iter()
        .filter(|e| e.kind == "integrations.hook_failed")
        .collect();
    assert_eq!(failed.len(), 1);
    assert!(failed[0].summary.contains("remove"));
}

#[tokio::test]
async fn a_removal_that_finds_nothing_to_do_still_emits_one_event_with_its_own_sentence() {
    let rig = Rig::release();
    rig.write_json(CLAUDE_FILE, &json!({ "model": "opus" }));

    remove(&rig, "claude").await.unwrap();

    let events = rig.kernel.recent_events(20).unwrap();
    let removed: Vec<_> = events
        .iter()
        .filter(|e| e.kind == "integrations.hook_removed")
        .collect();
    assert_eq!(removed.len(), 1);
    assert!(removed[0].summary.contains("no hooks of Navori"));
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
async fn a_deletion_that_fails_after_the_backup_leaves_the_file_and_a_complete_backup() {
    let rig = Rig::release();
    rig.write_json(GROK_FILE, &complete(&release_command(), &FIVE));
    let file = rig.path(GROK_FILE);
    let body = read_text(&file);
    let _unlock = Unlock(file.clone());
    let locked = std::process::Command::new("chflags")
        .arg("uchg")
        .arg(&file)
        .status()
        .unwrap();
    assert!(locked.success());

    let error = remove(&rig, "grok").await;

    assert!(error.is_err());
    assert_eq!(read_text(&file), body);
    let found = backups(&rig.path(".grok/hooks"));
    assert_eq!(found.len(), 1);
    assert_eq!(read_text(&found[0]), body);
}

#[tokio::test]
async fn remove_deletes_the_running_profiles_hooks_in_both_forms_and_nothing_else() {
    let rig = Rig::release();
    let guarded = release_command();
    let earlier = legacy_command(RELEASE_HELPER, false);
    let dev = command(DEV_HELPER, true);
    let dev_earlier = legacy_command(DEV_HELPER, true);
    rig.write_json(
        CLAUDE_FILE,
        &json!({
            "hooks": {
                "Stop": [
                    group(vec![handler(&guarded, 2)]),
                    group(vec![handler(&earlier, 2)]),
                    group(vec![handler(&dev, 2)]),
                    group(vec![handler(&dev_earlier, 2)]),
                    group(vec![foreign()]),
                ]
            }
        }),
    );

    remove(&rig, "claude").await.unwrap();

    assert_eq!(
        read_json(&rig.path(CLAUDE_FILE)),
        json!({
            "hooks": {
                "Stop": [
                    group(vec![handler(&dev, 2)]),
                    group(vec![handler(&dev_earlier, 2)]),
                    group(vec![foreign()]),
                ]
            }
        })
    );
}

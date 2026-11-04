#![allow(clippy::unwrap_used)]

mod common;

use common::{
    CLAUDE, CODEX, DEV_HELPER, FIVE, RELEASE_HELPER, Rig, THREE, command, complete, group, handler,
    legacy_command, tree,
};
use serde_json::{Value, json};

const CLAUDE_FILE: &str = ".claude/settings.json";
const CODEX_FILE: &str = ".codex/hooks.json";
const GROK_FILE: &str = ".grok/hooks/navori.json";

fn release_command() -> String {
    command(RELEASE_HELPER, false)
}

fn dev_command() -> String {
    command(DEV_HELPER, true)
}

fn offers(row: &Value) -> Vec<String> {
    row["offers"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| o.as_str().unwrap().to_owned())
        .collect()
}

#[tokio::test]
async fn a_harness_without_a_config_directory_is_not_found_with_a_reason() {
    let rig = Rig::release();
    rig.make_dir(".claude");
    let row = rig.row("codex").await;
    assert_eq!(row["state"], "not_found");
    let reason = row["reason"].as_str().unwrap();
    assert!(reason.ends_with('.') && !reason.is_empty(), "{reason}");
    assert!(reason.contains(".codex"), "{reason}");
    assert!(offers(&row).is_empty());
    assert_eq!(rig.row("claude").await["state"], "not_installed");
}

#[tokio::test]
async fn a_config_directory_with_no_hook_file_is_not_installed() {
    let rig = Rig::release();
    rig.make_dir(".claude");
    rig.make_dir(".grok");
    for harness in ["claude", "grok"] {
        let row = rig.row(harness).await;
        assert_eq!(row["state"], "not_installed", "{harness}");
        assert_eq!(offers(&row), ["install"]);
        assert!(row["reason"].is_null());
    }
}

#[tokio::test]
async fn a_hook_file_with_only_foreign_hooks_is_not_installed() {
    let rig = Rig::release();
    rig.write_json(
        CLAUDE_FILE,
        &json!({
            "model": "opus",
            "hooks": {
                "Stop": [ group(vec![handler("/usr/local/bin/other-tool stop", 5)]) ],
                "Notification": [ { "matcher": "*", "hooks": [ handler("echo hi", 1) ] } ]
            }
        }),
    );
    let row = rig.row("claude").await;
    assert_eq!(row["state"], "not_installed");
    assert_eq!(offers(&row), ["install"]);
}

#[tokio::test]
async fn a_complete_correct_set_is_installed() {
    let rig = Rig::release();
    rig.write(CLAUDE_FILE, include_str!("fixtures/claude_guarded.json"));
    rig.write(GROK_FILE, include_str!("fixtures/grok_guarded.json"));
    for harness in ["claude", "grok"] {
        let row = rig.row(harness).await;
        assert_eq!(row["state"], "installed", "{harness}");
        assert_eq!(offers(&row), ["remove"], "{harness}");
        assert!(row["reason"].is_null());
    }
}

#[tokio::test]
async fn a_set_missing_an_event_needs_repair() {
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
    for harness in ["claude", "grok"] {
        let row = rig.row(harness).await;
        assert_eq!(row["state"], "needs_repair", "{harness}");
        assert_eq!(offers(&row), ["repair", "remove"], "{harness}");
    }
}

#[tokio::test]
async fn a_claude_set_of_the_three_older_events_needs_repair() {
    let rig = Rig::release();
    rig.write_json(CLAUDE_FILE, &complete(&release_command(), &THREE));
    let row = rig.row("claude").await;
    assert_eq!(row["state"], "needs_repair");
    assert_eq!(offers(&row), ["repair", "remove"]);
}

#[tokio::test]
async fn a_stale_helper_path_needs_repair() {
    let rig = Rig::release();
    let stale = command("/Volumes/Old/Navori.app/Contents/Helpers/navori", false);
    rig.write_json(CLAUDE_FILE, &complete(&stale, &CLAUDE));
    assert_eq!(rig.row("claude").await["state"], "needs_repair");
}

#[tokio::test]
async fn an_async_entry_needs_repair() {
    let rig = Rig::release();
    let mut doc = complete(&release_command(), &CLAUDE);
    doc["hooks"]["Stop"][0]["hooks"][0]["async"] = json!(true);
    rig.write_json(CLAUDE_FILE, &doc);
    assert_eq!(rig.row("claude").await["state"], "needs_repair");
}

#[tokio::test]
async fn a_handler_sharing_a_group_with_a_matcher_needs_repair() {
    let rig = Rig::release();
    let mut doc = complete(&release_command(), &CLAUDE);
    doc["hooks"]["Stop"][0]["matcher"] = json!("Bash");
    rig.write_json(CLAUDE_FILE, &doc);
    assert_eq!(rig.row("claude").await["state"], "needs_repair");

    let mut shared = complete(&release_command(), &CLAUDE);
    shared["hooks"]["Stop"][0]["hooks"]
        .as_array_mut()
        .unwrap()
        .push(handler("echo foreign", 1));
    rig.write_json(CLAUDE_FILE, &shared);
    assert_eq!(rig.row("claude").await["state"], "needs_repair");
}

#[tokio::test]
async fn a_file_that_is_not_json_is_unreadable_with_the_parse_failure() {
    let rig = Rig::release();
    rig.write(CLAUDE_FILE, "{ \"hooks\": secret-token-123 ");
    let row = rig.row("claude").await;
    assert_eq!(row["state"], "unreadable");
    assert!(offers(&row).is_empty());
    let reason = row["reason"].as_str().unwrap();
    assert!(reason.contains("line 1 column"), "{reason}");
    assert!(!reason.contains("secret-token-123"), "{reason}");
}

#[tokio::test]
async fn a_root_that_is_not_an_object_is_unreadable() {
    let rig = Rig::release();
    rig.write(CLAUDE_FILE, "[1, 2, 3]");
    rig.write(GROK_FILE, r#"{"hooks": {"Stop": "nope"}}"#);
    for harness in ["claude", "grok"] {
        let row = rig.row(harness).await;
        assert_eq!(row["state"], "unreadable", "{harness}");
        assert!(offers(&row).is_empty());
        assert!(row["reason"].as_str().unwrap().ends_with('.'));
    }
}

#[tokio::test]
async fn a_dangling_symlink_is_unreadable() {
    let rig = Rig::release();
    rig.make_dir(".claude");
    std::os::unix::fs::symlink(rig.path("gone.json"), rig.path(CLAUDE_FILE)).unwrap();
    assert_eq!(rig.row("claude").await["state"], "unreadable");
}

#[tokio::test]
async fn a_symlinked_hook_file_is_read_through_the_link() {
    let rig = Rig::release();
    rig.write(
        "dotfiles/settings.json",
        include_str!("fixtures/claude_guarded.json"),
    );
    rig.make_dir(".claude");
    std::os::unix::fs::symlink(rig.path("dotfiles/settings.json"), rig.path(CLAUDE_FILE)).unwrap();
    assert_eq!(rig.row("claude").await["state"], "installed");
}

#[tokio::test]
async fn a_dev_entry_does_not_make_the_release_profile_installed() {
    let dev_only = complete(&dev_command(), &CLAUDE);

    let release = Rig::release();
    release.write_json(CLAUDE_FILE, &dev_only);
    assert_eq!(release.row("claude").await["state"], "not_installed");

    let dev = Rig::dev();
    dev.write_json(CLAUDE_FILE, &dev_only);
    assert_eq!(dev.row("claude").await["state"], "installed");

    let release_only = complete(&release_command(), &CLAUDE);
    let dev = Rig::dev();
    dev.write_json(CLAUDE_FILE, &release_only);
    assert_eq!(dev.row("claude").await["state"], "not_installed");
}

#[tokio::test]
async fn both_profiles_side_by_side_are_each_read_on_their_own() {
    let mut doc = complete(&release_command(), &CLAUDE);
    for event in CLAUDE {
        doc["hooks"][event]
            .as_array_mut()
            .unwrap()
            .push(group(vec![handler(&dev_command(), 2)]));
    }
    // The dev entry carries the SessionEnd timeout of 1 only on SessionEnd.
    doc["hooks"]["SessionEnd"][1]["hooks"][0]["timeout"] = json!(1);

    let release = Rig::release();
    release.write_json(CLAUDE_FILE, &doc);
    assert_eq!(release.row("claude").await["state"], "installed");

    let dev = Rig::dev();
    dev.write_json(CLAUDE_FILE, &doc);
    assert_eq!(dev.row("claude").await["state"], "installed");
}

#[tokio::test]
async fn codex_offers_the_same_buttons_as_claude_and_says_the_hooks_need_trusting() {
    let rig = Rig::release();
    rig.make_dir(".codex");
    let row = rig.row("codex").await;
    assert_eq!(row["state"], "not_installed");
    assert_eq!(offers(&row), ["install"]);
    assert_eq!(row["reason"], Value::Null);

    rig.write_json(CODEX_FILE, &complete(&release_command(), &CLAUDE));
    let row = rig.row("codex").await;
    assert_eq!(row["state"], "needs_repair");
    assert_eq!(offers(&row), ["repair", "remove"]);
    assert!(row["reason"].as_str().unwrap().contains("/hooks"));

    rig.write_json(CODEX_FILE, &complete(&release_command(), &THREE));
    assert_eq!(rig.row("codex").await["state"], "needs_repair");

    rig.write_json(CODEX_FILE, &complete(&release_command(), &CODEX));
    let row = rig.row("codex").await;
    assert_eq!(row["state"], "installed");
    assert_eq!(offers(&row), ["remove"]);
    assert!(row["reason"].as_str().unwrap().contains("/hooks"));
}

#[tokio::test]
async fn only_codex_says_its_hooks_need_trusting() {
    let rig = Rig::release();
    rig.write_json(CLAUDE_FILE, &complete(&release_command(), &CLAUDE));
    rig.write_json(GROK_FILE, &complete(&release_command(), &FIVE));

    assert_eq!(rig.row("claude").await["reason"], Value::Null);
    assert_eq!(rig.row("grok").await["reason"], Value::Null);
}

#[tokio::test]
async fn the_three_harnesses_come_back_in_a_fixed_order() {
    let rig = Rig::release();
    let state = rig.status().await;
    let order: Vec<&str> = state["harnesses"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["harness"].as_str().unwrap())
        .collect();
    assert_eq!(order, ["claude", "codex", "grok"]);
    let paths: Vec<String> = state["harnesses"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["path"].as_str().unwrap().to_owned())
        .collect();
    assert!(paths[0].ends_with(".claude/settings.json"));
    assert!(paths[1].ends_with(".codex/hooks.json"));
    assert!(paths[2].ends_with(".grok/hooks/navori.json"));
}

#[tokio::test]
async fn status_changes_no_file_and_creates_no_directory() {
    let rig = Rig::release();
    rig.make_dir(".grok");
    rig.write(CLAUDE_FILE, include_str!("fixtures/claude_guarded.json"));
    let before = tree(rig.home.path());
    rig.status().await;
    rig.status().await;
    assert_eq!(tree(rig.home.path()), before);
}

#[tokio::test]
async fn the_published_state_stays_empty() {
    let rig = Rig::release();
    rig.make_dir(".claude");
    rig.status().await;
    assert_eq!(
        rig.kernel.state("integrations").unwrap(),
        json!({ "harnesses": [] })
    );
}

#[tokio::test]
async fn a_command_that_is_not_the_single_quoted_helper_then_hook_reads_not_installed() {
    let lookalikes = [
        "\"/A/Navori.app/Contents/Helpers/navori\" hook",
        "/A/Navori.app/Contents/Helpers/navori hook",
        "'/A/Navori.app/Contents/Helpers/navori' hook extra",
        "'/A/Navori.app/Contents/Helpers/navori' hook; rm -rf x",
        "'/A/Navori.app/Contents/Helpers/other' hook",
        "'/A/Navori.app/Contents/Helpers/navori'",
        "'/A/Navori.app/Contents/Helpers/navori' --profile dev hook ",
        "'/A/Navori.app/Contents/Helpers/navori'\\",
        "''",
        "",
    ];
    for lookalike in lookalikes {
        for rig in [Rig::release(), Rig::dev()] {
            rig.write_json(CLAUDE_FILE, &complete(lookalike, &CLAUDE));
            assert_eq!(
                rig.row("claude").await["state"],
                "not_installed",
                "{lookalike}"
            );
        }
    }
}

#[tokio::test]
async fn a_quote_inside_the_helper_path_is_read_back_as_ours() {
    let rig = Rig::new(false, "/A/It's/Navori.app/Contents/Helpers/navori");
    let q = "'/A/It'\\''s/Navori.app/Contents/Helpers/navori'";
    let written = format!("[ -f {q} ] && [ -x {q} ] && exec {q} hook || true");
    rig.write_json(CLAUDE_FILE, &complete(&written, &CLAUDE));
    assert_eq!(rig.row("claude").await["state"], "installed");
}

#[tokio::test]
async fn hooks_written_in_the_earlier_form_need_repair() {
    let rig = Rig::release();
    rig.write(CLAUDE_FILE, include_str!("fixtures/claude_installed.json"));
    let row = rig.row("claude").await;
    assert_eq!(row["state"], "needs_repair");
    assert_eq!(offers(&row), ["repair", "remove"]);

    let dev = Rig::dev();
    dev.write_json(
        CLAUDE_FILE,
        &complete(&legacy_command(DEV_HELPER, true), &CLAUDE),
    );
    assert_eq!(dev.row("claude").await["state"], "needs_repair");
}

#[tokio::test]
async fn guarded_lookalikes_are_not_navoris() {
    let h = "/A/Navori.app/Contents/Helpers/navori";
    let guard = |a: &str, b: &str, c: &str, tail: &str| {
        format!("[ -f '{a}' ] && [ -x '{b}' ] && exec '{c}'{tail}")
    };
    let lookalikes = [
        guard(
            h,
            h,
            "/B/Navori.app/Contents/Helpers/navori",
            " hook || true",
        ),
        guard(
            h,
            "/B/Navori.app/Contents/Helpers/navori",
            h,
            " hook || true",
        ),
        guard(
            "/B/Navori.app/Contents/Helpers/navori",
            h,
            h,
            " hook || true",
        ),
        format!("[ -f '{h}' ] && [ -x '{h}' ] && '{h}' hook || true"),
        guard(h, h, h, " hook || false"),
        guard(h, h, h, " --profile dev hook || true"),
        guard(h, h, h, " hook || true; rm -rf x"),
        guard(h, h, h, " hook"),
        guard(
            "/A/Navori.app/Contents/Helpers/other",
            "/A/Navori.app/Contents/Helpers/other",
            "/A/Navori.app/Contents/Helpers/other",
            " hook || true",
        ),
    ];
    for lookalike in &lookalikes {
        let rig = Rig::release();
        rig.write_json(CLAUDE_FILE, &complete(lookalike, &CLAUDE));
        assert_eq!(
            rig.row("claude").await["state"],
            "not_installed",
            "{lookalike}"
        );
    }
    let dev_guard = guard(h, h, h, " hook || true");
    let rig = Rig::dev();
    rig.write_json(CLAUDE_FILE, &complete(&dev_guard, &CLAUDE));
    assert_eq!(rig.row("claude").await["state"], "not_installed");
}

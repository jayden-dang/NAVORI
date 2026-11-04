#![allow(clippy::unwrap_used)]

mod common;

use std::fs;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use common::{ASSERTION_NAME, ORIGIN_MS, Rig};
use navori_kernel::{ActionError, DispatchError, Severity};
use navori_macos::AssertionKind;
use navori_macos::fake::FakeProcess;
use serde_json::{Value, json};

const CLAUDE: i32 = 100;
const SHELL: i32 = 101;
const HOOK: i32 = 102;
const CLAUDE_START: u64 = 100_000;
const SHELL_START: u64 = 101_000;
const HOOK_START: u64 = 102_000;
const SESSION: &str = "sess-1";
const TTL_MS: i64 = 8 * 3_600_000;
const HOUR_MS: i64 = 3_600_000;

struct Checkout {
    path: PathBuf,
}

impl Checkout {
    fn new() -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("navori-session-cwd-{nanos}"));
        let git = path.join(".git");
        fs::create_dir_all(&git).unwrap();
        fs::write(git.join("HEAD"), "ref: refs/heads/feature-awake\n").unwrap();
        Self { path }
    }

    fn label(&self) -> String {
        let name = self.path.file_name().unwrap().to_string_lossy();
        format!("{name}/feature-awake")
    }
}

impl Drop for Checkout {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn hold_session(session_id: &str, pid: i32, ppid: i32) -> Value {
    json!({ "hold_session": { "session_id": session_id, "pid": pid, "ppid": ppid } })
}

fn end_session(session_id: &str) -> Value {
    json!({ "end_session": { "session_id": session_id } })
}

fn hold_command(pid: i32) -> Value {
    json!({ "hold_command": { "pid": pid } })
}

fn lease_count(state: &Value) -> usize {
    state["leases"].as_array().map(Vec::len).unwrap_or(0)
}

fn hook_chain(rig: &Rig, checkout: &Checkout) {
    rig.processes.add(
        FakeProcess::new(CLAUDE, 1, 501, CLAUDE_START, "/usr/local/bin/claude")
            .cwd(checkout.path.clone()),
    );
    rig.processes
        .add(FakeProcess::new(SHELL, CLAUDE, 501, SHELL_START, "/bin/sh"));
    rig.processes.add(FakeProcess::new(
        HOOK,
        SHELL,
        501,
        HOOK_START,
        "/usr/local/bin/navori",
    ));
}

fn session_row(label: &str, ends_at_ms: i64) -> Value {
    json!({
        "key": format!("session:{SESSION}"),
        "kind": "session",
        "harness": "Claude Code",
        "label": label,
        "pid": CLAUDE,
        "stopped": false,
        "ends_at_ms": ends_at_ms,
        "title": null
    })
}

fn awake_state(rig: &Rig) -> Value {
    match rig.kernel.state("awake") {
        Some(state) => state,
        None => panic!("awake state is missing"),
    }
}

async fn until(rig: &Rig, mut pred: impl FnMut(&Value) -> bool) -> Value {
    for _ in 0..50 {
        let state = awake_state(rig);
        if pred(&state) {
            return state;
        }
        rig.idle().await;
    }
    let state = awake_state(rig);
    panic!("condition not met: {state}");
}

fn started_summaries(rig: &Rig) -> Vec<String> {
    rig.kernel
        .recent_events(20)
        .unwrap()
        .into_iter()
        .filter(|event| event.kind == "awake.lease_started")
        .map(|event| event.summary)
        .collect()
}

fn ended_summaries(rig: &Rig) -> Vec<String> {
    rig.kernel
        .recent_events(20)
        .unwrap()
        .into_iter()
        .filter(|event| event.kind == "awake.lease_ended")
        .map(|event| event.summary)
        .collect()
}

fn assert_harness_source(summary: &str, checkout: &Checkout) {
    assert!(
        !summary.contains("feature-awake"),
        "event source used a branch name: {summary}"
    );
    assert!(
        !summary.contains(&checkout.label()),
        "event source used a project name: {summary}"
    );
}

#[tokio::test(start_paused = true)]
async fn a_session_lease_watches_the_harness_not_the_hook() {
    let checkout = Checkout::new();
    let rig = Rig::new();
    hook_chain(&rig, &checkout);

    let state = rig
        .dispatch(hold_session(SESSION, HOOK, SHELL))
        .await
        .unwrap();

    assert_eq!(
        rig.power.held(),
        vec![(AssertionKind::System, ASSERTION_NAME.to_owned())]
    );
    assert_eq!(rig.power.releases(), 0);
    assert_eq!(rig.exits.watched(), vec![CLAUDE]);
    assert_eq!(rig.exits.tokens(CLAUDE).len(), 1);
    assert!(rig.exits.tokens(HOOK).is_empty());
    assert!(rig.exits.tokens(SHELL).is_empty());
    assert_eq!(
        state,
        json!({
            "leases": [session_row(&checkout.label(), ORIGIN_MS as i64 + TTL_MS)],
            "system_held": true,
            "display_held": false,
            "since_ms": ORIGIN_MS as i64,
            "problem": null,
            "tracking": true
        })
    );
    assert!(
        rig.processes.reads().iter().all(|(_, kind)| *kind != "cpu"),
        "a session lease must not read cpu: {:?}",
        rig.processes.reads()
    );

    let started: Vec<_> = rig
        .kernel
        .recent_events(10)
        .unwrap()
        .into_iter()
        .filter(|event| event.kind == "awake.lease_started")
        .collect();
    assert_eq!(started.len(), 1);
    assert_eq!(started[0].module, "awake");
    assert_eq!(started[0].severity, Severity::Info);
    assert_eq!(started[0].summary, "Keeping this Mac awake: Claude Code.");
    assert_harness_source(&started[0].summary, &checkout);
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn a_dead_hook_pid_falls_back_to_its_parent() {
    let checkout = Checkout::new();
    let rig = Rig::new();
    rig.processes.add(
        FakeProcess::new(CLAUDE, 1, 501, CLAUDE_START, "/usr/local/bin/claude")
            .cwd(checkout.path.clone()),
    );
    rig.processes
        .add(FakeProcess::new(SHELL, CLAUDE, 501, SHELL_START, "/bin/sh"));

    let state = rig
        .dispatch(hold_session(SESSION, HOOK, SHELL))
        .await
        .unwrap();

    assert_eq!(
        state["leases"],
        json!([session_row(&checkout.label(), ORIGIN_MS as i64 + TTL_MS)])
    );
    assert_eq!(rig.exits.watched(), vec![CLAUDE]);
    assert!(rig.exits.tokens(SHELL).is_empty());
    assert!(rig.exits.tokens(HOOK).is_empty());
    assert_eq!(
        started_summaries(&rig),
        vec!["Keeping this Mac awake: Claude Code.".to_owned()]
    );
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn a_hold_from_a_process_outside_a_harness_is_invalid_with_no_lease() {
    let rig = Rig::new();
    rig.processes
        .add(FakeProcess::new(50, 1, 501, 50_000, "/bin/bash"));

    let err = rig
        .dispatch(hold_session(SESSION, 50, 1))
        .await
        .unwrap_err();

    assert!(
        matches!(
            err,
            DispatchError::Action(ActionError::Invalid(ref message))
                if message == "pid 50 is not inside a harness; no session lease was held"
        ),
        "{err:?}"
    );
    let state = awake_state(&rig);
    assert_eq!(lease_count(&state), 0);
    assert_eq!(state["system_held"], false);
    assert!(rig.power.held().is_empty());
    assert!(rig.exits.watched().is_empty());
    assert!(started_summaries(&rig).is_empty());
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn a_hold_with_both_pids_gone_is_invalid_with_no_lease() {
    let rig = Rig::new();

    let err = rig
        .dispatch(hold_session(SESSION, HOOK, SHELL))
        .await
        .unwrap_err();

    assert!(
        matches!(
            err,
            DispatchError::Action(ActionError::Invalid(ref message))
                if message == "pid 102 and parent 101 are not running; no session lease was held"
        ),
        "{err:?}"
    );
    let state = awake_state(&rig);
    assert_eq!(lease_count(&state), 0);
    assert_eq!(state["system_held"], false);
    assert!(rig.power.held().is_empty());
    assert!(rig.exits.watched().is_empty());
    assert!(started_summaries(&rig).is_empty());
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn a_repeated_hold_keeps_one_lease_moves_the_deadline_and_keeps_the_first_hold() {
    let checkout = Checkout::new();
    let rig = Rig::new();
    hook_chain(&rig, &checkout);
    let first = rig
        .dispatch(hold_session(SESSION, HOOK, SHELL))
        .await
        .unwrap();
    assert_eq!(
        first["leases"][0]["ends_at_ms"].as_i64().unwrap(),
        ORIGIN_MS as i64 + TTL_MS
    );
    assert_eq!(first["since_ms"].as_i64().unwrap(), ORIGIN_MS as i64);

    rig.advance(Duration::from_secs(60 * 60)).await;
    let second = rig
        .dispatch(hold_session(SESSION, HOOK, SHELL))
        .await
        .unwrap();

    assert_eq!(lease_count(&second), 1);
    assert_eq!(second["leases"][0]["key"], format!("session:{SESSION}"));
    assert_eq!(second["leases"][0]["kind"], "session");
    assert_eq!(second["leases"][0]["pid"], CLAUDE);
    assert_eq!(
        second["leases"][0]["ends_at_ms"].as_i64().unwrap(),
        ORIGIN_MS as i64 + HOUR_MS + TTL_MS
    );
    assert_eq!(second["since_ms"].as_i64().unwrap(), ORIGIN_MS as i64);
    assert_eq!(rig.exits.watched(), vec![CLAUDE]);
    assert_eq!(rig.exits.tokens(CLAUDE).len(), 1);
    assert_eq!(rig.power.releases(), 0);
    assert_eq!(
        started_summaries(&rig),
        vec!["Keeping this Mac awake: Claude Code.".to_owned()]
    );
    assert!(ended_summaries(&rig).is_empty());
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn end_session_ends_it_by_session_id_with_no_process_read() {
    let checkout = Checkout::new();
    let rig = Rig::new();
    hook_chain(&rig, &checkout);
    rig.dispatch(hold_session(SESSION, HOOK, SHELL))
        .await
        .unwrap();
    let lists = rig.processes.list_calls();

    let state = rig.dispatch(end_session(SESSION)).await.unwrap();

    assert_eq!(rig.processes.list_calls(), lists);
    assert_eq!(lease_count(&state), 0);
    assert_eq!(state["system_held"], false);
    assert_eq!(state["since_ms"], Value::Null);
    assert!(rig.power.held().is_empty());
    assert_eq!(rig.power.releases(), 1);
    assert!(rig.exits.watched().is_empty());
    assert_eq!(
        ended_summaries(&rig),
        vec!["Stopped keeping this Mac awake: Claude Code.".to_owned()]
    );
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn the_harness_exiting_ends_the_lease() {
    let checkout = Checkout::new();
    let rig = Rig::new();
    hook_chain(&rig, &checkout);
    rig.dispatch(hold_session(SESSION, HOOK, SHELL))
        .await
        .unwrap();

    rig.exits.fire(CLAUDE);
    let state = until(&rig, |state| lease_count(state) == 0).await;

    assert_eq!(state["system_held"], false);
    assert_eq!(state["since_ms"], Value::Null);
    assert!(rig.power.held().is_empty());
    assert_eq!(rig.power.releases(), 1);
    let ended = ended_summaries(&rig);
    assert_eq!(
        ended,
        vec!["Stopped keeping this Mac awake: Claude Code exited.".to_owned()]
    );
    assert_harness_source(&ended[0], &checkout);
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn the_session_ttl_ends_the_lease_8_hours_after_the_latest_hold() {
    let checkout = Checkout::new();
    let rig = Rig::new();
    hook_chain(&rig, &checkout);
    rig.dispatch(hold_session(SESSION, HOOK, SHELL))
        .await
        .unwrap();

    rig.advance(Duration::from_secs(60 * 60)).await;
    rig.dispatch(hold_session(SESSION, HOOK, SHELL))
        .await
        .unwrap();

    rig.advance(Duration::from_secs(7 * 60 * 60)).await;
    rig.idle().await;
    let still = awake_state(&rig);
    assert_eq!(
        lease_count(&still),
        1,
        "ttl is counted from the latest hold"
    );
    assert_eq!(still["since_ms"].as_i64().unwrap(), ORIGIN_MS as i64);
    assert_eq!(rig.power.held().len(), 1);

    rig.advance(Duration::from_secs(60 * 60)).await;
    let state = until(&rig, |state| lease_count(state) == 0).await;
    assert_eq!(state["system_held"], false);
    assert!(rig.power.held().is_empty());
    assert_eq!(rig.power.releases(), 1);
    assert!(rig.exits.watched().is_empty());
    let ended = ended_summaries(&rig);
    assert_eq!(
        ended,
        vec!["Stopped keeping this Mac awake: Claude Code reached its end time.".to_owned()]
    );
    assert_harness_source(&ended[0], &checkout);
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn a_session_id_outside_the_allowed_characters_is_invalid() {
    let rig = Rig::new();
    rig.processes.add(FakeProcess::new(
        CLAUDE,
        1,
        501,
        CLAUDE_START,
        "/usr/local/bin/claude",
    ));
    let too_long = "a".repeat(129);
    let cases = ["", too_long.as_str(), "sess ion", "sess/ion"];

    for session_id in cases {
        let err = rig
            .dispatch(hold_session(session_id, HOOK, SHELL))
            .await
            .unwrap_err();
        assert!(
            matches!(
                err,
                DispatchError::Action(ActionError::Invalid(ref message))
                    if message == "session_id must be 1 to 128 bytes of letters, digits, '.', '_', ':', or '-'"
            ),
            "{session_id:?} -> {err:?}"
        );
    }

    let state = awake_state(&rig);
    assert_eq!(lease_count(&state), 0);
    assert!(rig.power.held().is_empty());
    assert!(rig.exits.watched().is_empty());
    assert_eq!(rig.processes.list_calls(), 0);
    assert!(started_summaries(&rig).is_empty());

    let accepted = format!("A._:-{}", "b".repeat(123));
    assert_eq!(accepted.len(), 128);
    let held = rig
        .dispatch(hold_session(&accepted, CLAUDE, 1))
        .await
        .unwrap();
    assert_eq!(lease_count(&held), 1);
    assert_eq!(held["leases"][0]["key"], format!("session:{accepted}"));
    assert_eq!(held["leases"][0]["harness"], "Claude Code");
    assert_eq!(held["leases"][0]["label"], Value::Null);
    assert_eq!(held["leases"][0]["pid"], CLAUDE);
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn a_non_positive_pid_or_negative_ppid_is_invalid() {
    let rig = Rig::new();
    let cases = [
        (0, 1, "pid must be positive, not 0"),
        (-1, 1, "pid must be positive, not -1"),
        (CLAUDE, -1, "ppid must be zero or positive, not -1"),
    ];
    for (pid, ppid, message) in cases {
        let err = rig
            .dispatch(hold_session(SESSION, pid, ppid))
            .await
            .unwrap_err();
        assert!(
            matches!(
                err,
                DispatchError::Action(ActionError::Invalid(ref text)) if text == message
            ),
            "{err:?}"
        );
    }
    assert_eq!(rig.processes.list_calls(), 0);
    assert!(rig.power.held().is_empty());
    assert!(rig.exits.watched().is_empty());
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn a_hold_answers_without_waiting_for_grace() {
    let checkout = Checkout::new();
    let rig = Rig::new();
    hook_chain(&rig, &checkout);

    let state = rig
        .dispatch(hold_session(SESSION, HOOK, SHELL))
        .await
        .unwrap();

    assert_eq!(lease_count(&state), 1);
    assert_eq!(
        state["leases"][0]["ends_at_ms"].as_i64().unwrap(),
        ORIGIN_MS as i64 + TTL_MS
    );
    assert_eq!(state["since_ms"].as_i64().unwrap(), ORIGIN_MS as i64);
    assert_eq!(rig.power.held().len(), 1);

    rig.advance(Duration::from_secs(120)).await;
    rig.idle().await;
    let during_grace = awake_state(&rig);
    assert_eq!(lease_count(&during_grace), 1);
    assert_eq!(during_grace["system_held"], true);
    assert_eq!(rig.power.releases(), 0);
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn a_session_and_a_command_on_one_process_unwatch_only_when_the_last_lease_is_gone() {
    let checkout = Checkout::new();
    let rig = Rig::new();
    hook_chain(&rig, &checkout);

    rig.dispatch(hold_command(CLAUDE)).await.unwrap();
    let held = rig
        .dispatch(hold_session(SESSION, HOOK, SHELL))
        .await
        .unwrap();

    assert_eq!(lease_count(&held), 2);
    assert_eq!(
        held["leases"][0]["key"],
        format!("command:{CLAUDE}:{CLAUDE_START}")
    );
    assert_eq!(held["leases"][0]["kind"], "command");
    assert_eq!(held["leases"][1]["key"], format!("session:{SESSION}"));
    assert_eq!(held["leases"][1]["kind"], "session");
    assert_eq!(held["leases"][1]["pid"], CLAUDE);
    assert_eq!(rig.exits.watched(), vec![CLAUDE]);
    assert_eq!(rig.exits.tokens(CLAUDE).len(), 1);
    let command_rows = held["leases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|lease| lease["kind"] == "command")
        .count();
    assert_eq!(command_rows, 1);

    let after_end = rig.dispatch(end_session(SESSION)).await.unwrap();
    assert_eq!(lease_count(&after_end), 1);
    assert_eq!(after_end["leases"][0]["kind"], "command");
    assert_eq!(after_end["system_held"], true);
    assert_eq!(rig.exits.watched(), vec![CLAUDE]);
    assert_eq!(rig.exits.tokens(CLAUDE).len(), 1);
    assert_eq!(rig.power.releases(), 0);

    let cleared = rig.dispatch(json!("release_all")).await.unwrap();
    assert_eq!(lease_count(&cleared), 0);
    assert_eq!(cleared["system_held"], false);
    assert!(rig.power.held().is_empty());
    assert!(rig.exits.watched().is_empty());

    let started = started_summaries(&rig);
    assert!(started.contains(&"Keeping this Mac awake: claude.".to_owned()));
    assert!(started.contains(&"Keeping this Mac awake: Claude Code.".to_owned()));
    for summary in &started {
        assert_harness_source(summary, &checkout);
    }
    rig.shutdown().await;
}

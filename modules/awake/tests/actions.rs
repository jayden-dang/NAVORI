#![allow(clippy::unwrap_used)]

mod common;

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use common::{ORIGIN_MS, Rig};
use navori_kernel::{ModuleAction, Risk, Severity};
use navori_macos::fake::FakeProcess;
use navori_mod_awake::AwakeAction;
use serde_json::{Value, json};

const CLAUDE: i32 = 100;
const CHILD: i32 = 200;
const COMMAND: i32 = 42;
const CLAUDE_START: u64 = 100_000;
const COMMAND_START: u64 = 5_000;
const SESSION: &str = "sess-1";
const FIRST_HOLD_US: u64 = ORIGIN_MS * 1_000;

fn awake_specs(rig: &Rig) -> Vec<navori_kernel::ActionSpec> {
    rig.kernel
        .actions()
        .into_iter()
        .filter(|spec| spec.module == "awake")
        .collect()
}

fn sample(name: &str) -> Value {
    match name {
        "status" => json!("status"),
        "hold_for" => json!({ "hold_for": { "seconds": 1 } }),
        "hold_until_off" => json!("hold_until_off"),
        "display" => json!({ "display": { "on": true } }),
        "hold_command" => json!({ "hold_command": { "pid": 1 } }),
        "hold_session" => {
            json!({ "hold_session": { "session_id": "s", "pid": 1, "ppid": 1 } })
        }
        "stop_session" => json!({ "stop_session": { "session_id": "s" } }),
        "end_session" => json!({ "end_session": { "session_id": "s" } }),
        "release_all" => json!("release_all"),
        other => panic!("no sample payload for catalog name {other}"),
    }
}

fn assert_one_sentence(summary: &str) {
    assert!(!summary.contains('\n'), "{summary}");
    assert!(summary.ends_with('.'), "{summary}");
    assert_eq!(summary.matches('.').count(), 1, "{summary}");
}

fn lease_events(rig: &Rig) -> Vec<navori_kernel::Event> {
    let mut events: Vec<_> = rig
        .kernel
        .recent_events(20)
        .unwrap()
        .into_iter()
        .filter(|event| event.kind == "awake.lease_started" || event.kind == "awake.lease_ended")
        .collect();
    events.sort_by(|left, right| left.ts_ms.cmp(&right.ts_ms).then(left.id.cmp(&right.id)));
    events
}

fn awake_state(rig: &Rig) -> Value {
    match rig.kernel.state("awake") {
        Some(state) => state,
        None => panic!("awake state is missing"),
    }
}

async fn until_gone(rig: &Rig) {
    for _ in 0..50 {
        let state = awake_state(rig);
        if state["leases"].as_array().is_some_and(Vec::is_empty) {
            return;
        }
        rig.idle().await;
    }
    panic!("lease did not end: {}", awake_state(rig));
}

async fn one_start(action: Value) -> String {
    let rig = Rig::new();
    rig.dispatch(action).await.unwrap();
    let events = lease_events(&rig);
    assert_eq!(events.len(), 1, "{events:?}");
    assert_eq!(events[0].module, "awake");
    assert_eq!(events[0].kind, "awake.lease_started");
    assert_eq!(events[0].severity, Severity::Info);
    assert_one_sentence(&events[0].summary);
    let summary = events[0].summary.clone();
    rig.shutdown().await;
    summary
}

async fn deadline_end(seconds: u64) -> (Severity, String) {
    let rig = Rig::new();
    rig.dispatch(json!({ "hold_for": { "seconds": seconds } }))
        .await
        .unwrap();
    rig.advance(Duration::from_secs(seconds.saturating_sub(1)))
        .await;
    rig.idle().await;
    assert!(
        !awake_state(&rig)["leases"]
            .as_array()
            .is_some_and(Vec::is_empty),
        "the lease ended before its deadline"
    );
    rig.advance(Duration::from_secs(1)).await;
    until_gone(&rig).await;
    let ended = ended_only(&rig);
    rig.shutdown().await;
    ended
}

fn ended_only(rig: &Rig) -> (Severity, String) {
    let ended: Vec<_> = lease_events(rig)
        .into_iter()
        .filter(|event| event.kind == "awake.lease_ended")
        .collect();
    assert_eq!(ended.len(), 1, "{ended:?}");
    assert_eq!(ended[0].module, "awake");
    assert_one_sentence(&ended[0].summary);
    (ended[0].severity, ended[0].summary.clone())
}

fn claude(rig: &Rig, cwd: &Path) {
    rig.processes
        .add(FakeProcess::new(CLAUDE, 1, 501, CLAUDE_START, "/usr/local/bin/claude").cwd(cwd));
}

fn child(rig: &Rig, start_us: u64) {
    rig.processes
        .add(FakeProcess::new(CHILD, CLAUDE, 501, start_us, "/bin/sleep"));
}

async fn session_ttl_end() -> (Severity, String) {
    let checkout = Checkout::new();
    let rig = Rig::new();
    claude(&rig, &checkout.path);
    rig.dispatch(json!({
        "hold_session": { "session_id": SESSION, "pid": CLAUDE, "ppid": 1 }
    }))
    .await
    .unwrap();
    rig.advance(Duration::from_secs(8 * 60 * 60)).await;
    until_gone(&rig).await;
    let ended = ended_only(&rig);
    rig.shutdown().await;
    ended
}

async fn command_exit_end() -> (Severity, String) {
    let rig = Rig::new();
    rig.processes
        .add(command_process("/opt/secret-bin-path/sleep"));
    rig.dispatch(json!({ "hold_command": { "pid": COMMAND } }))
        .await
        .unwrap();
    rig.exits.fire(COMMAND);
    until_gone(&rig).await;
    let ended = ended_only(&rig);
    rig.shutdown().await;
    ended
}

async fn grace_end() -> (Severity, String) {
    let checkout = Checkout::new();
    let rig = Rig::new();
    claude(&rig, &checkout.path);
    child(&rig, FIRST_HOLD_US - 1);
    rig.dispatch(json!({
        "hold_session": { "session_id": SESSION, "pid": CLAUDE, "ppid": 1 }
    }))
    .await
    .unwrap();
    rig.dispatch(json!({ "stop_session": { "session_id": SESSION } }))
        .await
        .unwrap();
    rig.advance(Duration::from_secs(120)).await;
    until_gone(&rig).await;
    let ended = ended_only(&rig);
    rig.shutdown().await;
    ended
}

async fn cap_end() -> (Severity, String) {
    let checkout = Checkout::new();
    let rig = Rig::new();
    claude(&rig, &checkout.path);
    child(&rig, FIRST_HOLD_US + 1);
    rig.dispatch(json!({
        "hold_session": { "session_id": SESSION, "pid": CLAUDE, "ppid": 1 }
    }))
    .await
    .unwrap();
    rig.dispatch(json!({ "stop_session": { "session_id": SESSION } }))
        .await
        .unwrap();
    rig.advance(Duration::from_secs(2 * 60 * 60)).await;
    until_gone(&rig).await;
    let ended = ended_only(&rig);
    rig.shutdown().await;
    ended
}

async fn release_all_end() -> (Severity, String) {
    let rig = Rig::new();
    rig.dispatch(json!({ "hold_for": { "seconds": 3600 } }))
        .await
        .unwrap();
    rig.dispatch(json!({ "display": { "on": true } }))
        .await
        .unwrap();
    rig.dispatch(json!("release_all")).await.unwrap();
    let ended = ended_only(&rig);
    rig.shutdown().await;
    ended
}

async fn display_off_ends() -> Vec<String> {
    let rig = Rig::new();
    rig.dispatch(json!({ "display": { "on": true } }))
        .await
        .unwrap();
    rig.dispatch(json!({ "display": { "on": false } }))
        .await
        .unwrap();
    let ended = ended_summaries(&rig);
    rig.shutdown().await;
    ended
}

async fn end_session_ends() -> Vec<String> {
    let checkout = Checkout::new();
    let rig = Rig::new();
    claude(&rig, &checkout.path);
    rig.dispatch(json!({
        "hold_session": { "session_id": SESSION, "pid": CLAUDE, "ppid": 1 }
    }))
    .await
    .unwrap();
    rig.dispatch(json!({ "end_session": { "session_id": SESSION } }))
        .await
        .unwrap();
    let ended = ended_summaries(&rig);
    rig.shutdown().await;
    ended
}

fn ended_summaries(rig: &Rig) -> Vec<String> {
    lease_events(rig)
        .into_iter()
        .filter(|event| event.kind == "awake.lease_ended")
        .map(|event| event.summary)
        .collect()
}

fn command_process(exe: &str) -> FakeProcess {
    FakeProcess::new(COMMAND, 1, 501, COMMAND_START, exe)
        .argv(&["sleep", "--flag", "secret-argv"])
        .cwd("/tmp/secret-cwd-navori")
}

struct Checkout {
    path: PathBuf,
}

impl Checkout {
    fn new() -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("navori-secret-project-{nanos}"));
        let git = path.join(".git");
        fs::create_dir_all(&git).unwrap();
        fs::write(git.join("HEAD"), "ref: refs/heads/secret-branch\n").unwrap();
        Self { path }
    }
}

impl Drop for Checkout {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

#[tokio::test(start_paused = true)]
async fn every_action_has_a_description_a_schema_and_a_risk() {
    let rig = Rig::new();
    let specs = awake_specs(&rig);
    let names: Vec<_> = specs.iter().map(|spec| spec.name.as_str()).collect();
    assert_eq!(
        names,
        vec![
            "status",
            "hold_for",
            "hold_until_off",
            "display",
            "hold_command",
            "hold_session",
            "stop_session",
            "end_session",
            "release_all",
        ]
    );
    for spec in &specs {
        assert!(
            spec.description.contains(' ') && spec.description.ends_with('.'),
            "{}",
            spec.description
        );
        assert_ne!(spec.description, spec.name);
        assert!(spec.input_schema.is_object(), "{}", spec.input_schema);
        assert!(
            spec.input_schema.to_string().contains(&spec.name),
            "schema for {} does not name it: {}",
            spec.name,
            spec.input_schema
        );
        assert!(matches!(
            spec.risk,
            Risk::Read | Risk::Write | Risk::Destructive
        ));
    }
    let docs = AwakeAction::catalog();
    assert_eq!(docs.len(), specs.len());
    for (doc, spec) in docs.iter().zip(&specs) {
        assert_eq!(doc.name, spec.name);
        assert_eq!(doc.description, spec.description);
        assert_eq!(doc.risk, spec.risk);
    }
    rig.shutdown().await;
}

#[test]
fn every_catalog_name_decodes_as_an_action() {
    for doc in AwakeAction::catalog() {
        let action: AwakeAction = serde_json::from_value(sample(doc.name))
            .unwrap_or_else(|err| panic!("{} did not decode: {err}", doc.name));
        assert_eq!(action.risk(), doc.risk, "{}", doc.name);
    }
}

#[test]
fn hold_until_off_says_a_later_until_off_replaces_the_manual_lease() {
    let doc = AwakeAction::catalog()
        .into_iter()
        .find(|doc| doc.name == "hold_until_off")
        .unwrap();
    assert_eq!(
        doc.description,
        "Keep this Mac awake until it is turned off. A later duration or until-off request replaces this lease."
    );
}

#[tokio::test(start_paused = true)]
async fn status_is_read_and_the_others_are_write() {
    let rig = Rig::new();
    for spec in awake_specs(&rig) {
        let action: AwakeAction = serde_json::from_value(sample(&spec.name)).unwrap();
        if spec.name == "status" {
            assert_eq!(spec.risk, Risk::Read);
            assert_eq!(action.risk(), Risk::Read);
        } else {
            assert_eq!(spec.risk, Risk::Write, "{}", spec.name);
            assert_eq!(action.risk(), Risk::Write, "{}", spec.name);
        }
    }
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn no_action_is_destructive() {
    let rig = Rig::new();
    for spec in awake_specs(&rig) {
        let action: AwakeAction = serde_json::from_value(sample(&spec.name)).unwrap();
        assert_ne!(spec.risk, Risk::Destructive, "{}", spec.name);
        assert_ne!(action.risk(), Risk::Destructive, "{}", spec.name);
    }
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn a_started_lease_emits_one_sentence() {
    let durations = [
        (1_u64, "Keeping this Mac awake: 1 second."),
        (10, "Keeping this Mac awake: 10 seconds."),
        (60, "Keeping this Mac awake: 1 minute."),
        (90, "Keeping this Mac awake: 90 seconds."),
        (120, "Keeping this Mac awake: 2 minutes."),
        (3_600, "Keeping this Mac awake: 1 hour."),
        (3_660, "Keeping this Mac awake: 61 minutes."),
        (7_200, "Keeping this Mac awake: 2 hours."),
        (23 * 3_600, "Keeping this Mac awake: 23 hours."),
        (25 * 3_600, "Keeping this Mac awake: 25 hours."),
        (86_400, "Keeping this Mac awake: 1 day."),
        (2 * 86_400, "Keeping this Mac awake: 2 days."),
        (7 * 86_400, "Keeping this Mac awake: 7 days."),
    ];
    for (seconds, sentence) in durations {
        let summary = one_start(json!({ "hold_for": { "seconds": seconds } })).await;
        assert_eq!(summary, sentence);
        if seconds != 3_600 {
            assert!(
                !summary.contains("1 hour"),
                "{seconds} seconds was recorded as 1 hour"
            );
        }
    }

    assert_eq!(
        one_start(json!("hold_until_off")).await,
        "Keeping this Mac awake: until off."
    );
    assert_eq!(
        one_start(json!({ "display": { "on": true } })).await,
        "Keeping this Mac awake: display."
    );

    let rig = Rig::new();
    rig.processes
        .add(command_process("/opt/secret-bin-path/sleep"));
    assert_eq!(
        started_summaries_after(&rig, json!({ "hold_command": { "pid": COMMAND } })).await,
        vec!["Keeping this Mac awake: sleep.".to_owned()]
    );
    rig.shutdown().await;

    let checkout = Checkout::new();
    let rig = Rig::new();
    claude(&rig, &checkout.path);
    assert_eq!(
        started_summaries_after(
            &rig,
            json!({ "hold_session": { "session_id": SESSION, "pid": CLAUDE, "ppid": 1 } })
        )
        .await,
        vec!["Keeping this Mac awake: Claude Code.".to_owned()]
    );
    rig.shutdown().await;

    let rig = Rig::new();
    rig.dispatch(json!({ "hold_for": { "seconds": 3600 } }))
        .await
        .unwrap();
    rig.dispatch(json!({ "hold_for": { "seconds": 7200 } }))
        .await
        .unwrap();
    let events = lease_events(&rig);
    let mut started: Vec<_> = events
        .iter()
        .filter(|event| event.kind == "awake.lease_started")
        .map(|event| event.summary.clone())
        .collect();
    started.sort();
    let ended: Vec<_> = events
        .iter()
        .filter(|event| event.kind == "awake.lease_ended")
        .collect();
    assert_eq!(
        started,
        vec![
            "Keeping this Mac awake: 1 hour.".to_owned(),
            "Keeping this Mac awake: 2 hours.".to_owned(),
        ]
    );
    assert_eq!(ended.len(), 1, "{ended:?}");
    assert_eq!(ended[0].summary, "Stopped keeping this Mac awake: 1 hour.");
    rig.shutdown().await;
}

async fn started_summaries_after(rig: &Rig, action: Value) -> Vec<String> {
    rig.dispatch(action).await.unwrap();
    lease_events(rig)
        .into_iter()
        .filter(|event| event.kind == "awake.lease_started")
        .map(|event| {
            assert_eq!(event.severity, Severity::Info);
            assert_one_sentence(&event.summary);
            event.summary
        })
        .collect()
}

#[tokio::test(start_paused = true)]
async fn an_ended_lease_emits_one_sentence() {
    assert_eq!(
        deadline_end(3_600).await,
        (
            Severity::Info,
            "Stopped keeping this Mac awake: 1 hour reached its end time.".to_owned()
        )
    );
    let short = deadline_end(10).await;
    assert_eq!(
        short,
        (
            Severity::Info,
            "Stopped keeping this Mac awake: 10 seconds reached its end time.".to_owned()
        )
    );
    assert!(!short.1.contains("1 hour"));
    assert_eq!(
        session_ttl_end().await,
        (
            Severity::Info,
            "Stopped keeping this Mac awake: Claude Code reached its end time.".to_owned()
        )
    );
    assert_eq!(
        command_exit_end().await,
        (
            Severity::Info,
            "Stopped keeping this Mac awake: sleep exited.".to_owned()
        )
    );
    assert_eq!(
        grace_end().await,
        (
            Severity::Info,
            "Stopped keeping this Mac awake: Claude Code stopped.".to_owned()
        )
    );
    assert_eq!(
        cap_end().await,
        (
            Severity::Warn,
            "Stopped keeping this Mac awake: Claude Code was still busy 2 hours after it stopped."
                .to_owned()
        )
    );
    assert_eq!(
        release_all_end().await,
        (
            Severity::Info,
            "Stopped keeping this Mac awake: Release all.".to_owned()
        )
    );
    assert_eq!(
        display_off_ends().await,
        vec!["Stopped keeping this Mac awake: display.".to_owned()]
    );
    assert_eq!(
        end_session_ends().await,
        vec!["Stopped keeping this Mac awake: Claude Code.".to_owned()]
    );
}

#[tokio::test(start_paused = true)]
async fn no_event_names_a_project_or_a_branch() {
    let checkout = Checkout::new();
    let project = checkout
        .path
        .file_name()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let rig = Rig::new();
    claude(&rig, &checkout.path);
    rig.processes
        .add(command_process("/opt/secret-bin-path/sleep"));
    rig.dispatch(json!({
        "hold_session": { "session_id": SESSION, "pid": CLAUDE, "ppid": 1 }
    }))
    .await
    .unwrap();
    rig.dispatch(json!({ "hold_command": { "pid": COMMAND } }))
        .await
        .unwrap();

    let events = lease_events(&rig);
    let session = events
        .iter()
        .find(|event| event.payload["kind"] == "session")
        .unwrap();
    let command = events
        .iter()
        .find(|event| event.payload["kind"] == "command")
        .unwrap();
    assert_eq!(events.len(), 2);
    assert_eq!(session.summary, "Keeping this Mac awake: Claude Code.");
    assert_eq!(command.summary, "Keeping this Mac awake: sleep.");
    assert_eq!(
        session.payload,
        json!({ "key": "session:sess-1", "kind": "session" })
    );
    assert_eq!(
        command.payload,
        json!({ "key": "command:42:5000", "kind": "command" })
    );
    for event in &events {
        let text = format!("{} {}", event.summary, event.payload);
        for secret in [
            project.as_str(),
            "secret-branch",
            "secret-argv",
            "secret-cwd-navori",
            "secret-bin-path",
            "/usr/local/bin/claude",
            checkout.path.to_str().unwrap(),
        ] {
            assert!(!text.contains(secret), "{secret} leaked into {text}");
        }
    }
    rig.shutdown().await;
}

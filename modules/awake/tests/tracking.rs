#![allow(clippy::unwrap_used)]

mod common;

use common::{ASSERTION_NAME, Rig};
use navori_kernel::{ActionError, DispatchError};
use navori_macos::AssertionKind;
use navori_macos::fake::FakeProcess;
use serde_json::{Value, json};
use std::time::Duration;

const CLAUDE: i32 = 100;
const HOOK: i32 = 102;
const SESSION: &str = "sess-1";

fn hook_chain(rig: &Rig) {
    rig.processes.add(FakeProcess::new(
        CLAUDE,
        1,
        501,
        100_000,
        "/usr/local/bin/claude",
    ));
    rig.processes.add(FakeProcess::new(
        HOOK,
        CLAUDE,
        501,
        102_000,
        "/usr/local/bin/navori",
    ));
}

fn hold() -> Value {
    json!({ "hold_session": { "session_id": SESSION, "pid": HOOK, "ppid": CLAUDE } })
}

fn tracking(on: bool) -> Value {
    json!({ "tracking": { "on": on } })
}

fn state(rig: &Rig) -> Value {
    rig.kernel.state("awake").unwrap()
}

fn kinds(state: &Value) -> Vec<String> {
    state["leases"]
        .as_array()
        .unwrap()
        .iter()
        .map(|lease| lease["kind"].as_str().unwrap().to_owned())
        .collect()
}

fn system_only() -> Vec<(AssertionKind, String)> {
    vec![(AssertionKind::System, ASSERTION_NAME.to_owned())]
}

fn summaries(rig: &Rig, kind: &str) -> Vec<String> {
    rig.kernel
        .recent_events(20)
        .unwrap()
        .into_iter()
        .filter(|event| event.kind == kind)
        .map(|event| event.summary)
        .collect()
}

#[tokio::test(start_paused = true)]
async fn tracking_is_on_with_nothing_stored() {
    let rig = Rig::new();
    let answer = rig.dispatch(json!("status")).await.unwrap();
    assert_eq!(answer["tracking"], true);
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn off_lets_the_mac_sleep_and_hides_the_session() {
    let rig = Rig::new();
    hook_chain(&rig);
    rig.dispatch(hold()).await.unwrap();
    assert_eq!(rig.power.held(), system_only());

    let answer = rig.dispatch(tracking(false)).await.unwrap();

    assert_eq!(answer["tracking"], false);
    assert_eq!(kinds(&answer), Vec::<String>::new());
    assert_eq!(answer["system_held"], false);
    assert_eq!(answer["since_ms"], Value::Null);
    assert_eq!(answer["problem"], Value::Null);
    assert!(rig.power.held().is_empty());
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn a_hold_while_off_holds_nothing_and_comes_back_when_on() {
    let rig = Rig::new();
    hook_chain(&rig);
    rig.dispatch(tracking(false)).await.unwrap();

    let answer = rig.dispatch(hold()).await.unwrap();
    assert_eq!(kinds(&answer), Vec::<String>::new());
    assert!(rig.power.held().is_empty());

    let answer = rig.dispatch(tracking(true)).await.unwrap();
    assert_eq!(kinds(&answer), ["session"]);
    assert_eq!(answer["system_held"], true);
    assert_eq!(rig.power.held(), system_only());
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn a_session_that_ended_while_off_does_not_come_back() {
    let rig = Rig::new();
    hook_chain(&rig);
    rig.dispatch(hold()).await.unwrap();
    rig.dispatch(tracking(false)).await.unwrap();
    rig.dispatch(json!({ "end_session": { "session_id": SESSION } }))
        .await
        .unwrap();

    let answer = rig.dispatch(tracking(true)).await.unwrap();

    assert_eq!(kinds(&answer), Vec::<String>::new());
    assert!(rig.power.held().is_empty());
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn a_harness_that_exits_while_off_does_not_come_back() {
    let rig = Rig::new();
    hook_chain(&rig);
    rig.dispatch(hold()).await.unwrap();
    rig.dispatch(tracking(false)).await.unwrap();
    rig.processes.exit(CLAUDE);
    rig.exits.fire(CLAUDE);
    rig.idle().await;

    let answer = rig.dispatch(tracking(true)).await.unwrap();

    assert_eq!(kinds(&answer), Vec::<String>::new());
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn grace_still_ends_a_stopped_session_while_off() {
    let rig = Rig::new();
    hook_chain(&rig);
    rig.dispatch(hold()).await.unwrap();
    rig.dispatch(tracking(false)).await.unwrap();
    rig.dispatch(json!({ "stop_session": { "session_id": SESSION } }))
        .await
        .unwrap();
    rig.advance(Duration::from_secs(121)).await;
    rig.idle().await;

    let answer = rig.dispatch(tracking(true)).await.unwrap();

    assert_eq!(kinds(&answer), Vec::<String>::new());
    assert!(rig.power.held().is_empty());
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn the_ttl_still_ends_a_session_while_off() {
    let rig = Rig::new();
    hook_chain(&rig);
    rig.dispatch(hold()).await.unwrap();
    rig.dispatch(tracking(false)).await.unwrap();
    rig.advance(Duration::from_secs(8 * 60 * 60 + 1)).await;
    rig.idle().await;

    let answer = rig.dispatch(tracking(true)).await.unwrap();

    assert_eq!(kinds(&answer), Vec::<String>::new());
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn the_setting_and_the_followed_session_survive_a_reopen() {
    let mut rig = Rig::new();
    hook_chain(&rig);
    rig.dispatch(hold()).await.unwrap();
    rig.dispatch(tracking(false)).await.unwrap();

    rig.reopen().await;

    assert_eq!(state(&rig)["tracking"], false);
    assert_eq!(kinds(&state(&rig)), Vec::<String>::new());
    assert!(rig.power.held().is_empty());

    let answer = rig.dispatch(tracking(true)).await.unwrap();
    assert_eq!(kinds(&answer), ["session"]);
    assert_eq!(rig.power.held(), system_only());
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn turning_on_is_stored_too() {
    let mut rig = Rig::new();
    rig.dispatch(tracking(false)).await.unwrap();
    rig.dispatch(tracking(true)).await.unwrap();
    rig.reopen().await;
    assert_eq!(state(&rig)["tracking"], true);
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn a_setting_that_cannot_be_stored_changes_nothing() {
    let rig = Rig::new();
    hook_chain(&rig);
    rig.dispatch(hold()).await.unwrap();
    rig.kernel.store().with_conn(|conn| {
        let _ = conn.execute("DROP TABLE kv", []);
    });

    let err = rig.dispatch(tracking(false)).await.unwrap_err();

    assert!(matches!(err, DispatchError::Action(ActionError::Failed(_))));
    assert_eq!(state(&rig)["tracking"], true);
    assert_eq!(rig.power.held(), system_only());
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn off_keeps_manual_and_command_leases() {
    let rig = Rig::new();
    hook_chain(&rig);
    rig.processes
        .add(FakeProcess::new(200, 1, 501, 200_000, "/bin/cargo"));
    rig.dispatch(hold()).await.unwrap();
    rig.dispatch(json!("keep_indefinitely")).await.unwrap();
    rig.dispatch(json!({ "hold_command": { "pid": 200 } }))
        .await
        .unwrap();

    let answer = rig.dispatch(tracking(false)).await.unwrap();

    assert_eq!(kinds(&answer), ["display", "command"]);
    assert_eq!(answer["system_held"], true);
    assert_eq!(answer["display_held"], true);
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn no_session_lease_event_while_off() {
    let rig = Rig::new();
    hook_chain(&rig);
    rig.dispatch(tracking(false)).await.unwrap();
    rig.dispatch(hold()).await.unwrap();
    rig.dispatch(json!({ "end_session": { "session_id": SESSION } }))
        .await
        .unwrap();

    assert!(summaries(&rig, "awake.lease_started").is_empty());
    assert!(summaries(&rig, "awake.lease_ended").is_empty());
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn a_change_emits_one_event_and_a_repeat_emits_none() {
    let rig = Rig::new();
    rig.dispatch(tracking(true)).await.unwrap();
    assert!(summaries(&rig, "awake.tracking_changed").is_empty());

    rig.dispatch(tracking(false)).await.unwrap();
    rig.dispatch(tracking(false)).await.unwrap();
    rig.dispatch(tracking(true)).await.unwrap();

    let mut got = summaries(&rig, "awake.tracking_changed");
    got.sort();
    assert_eq!(
        got,
        [
            "Tracking agents is off: agent sessions no longer keep this Mac awake.",
            "Tracking agents is on: agent sessions keep this Mac awake.",
        ]
    );
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn release_all_while_off_ends_followed_sessions_and_stays_off() {
    let rig = Rig::new();
    hook_chain(&rig);
    rig.dispatch(tracking(false)).await.unwrap();
    rig.dispatch(hold()).await.unwrap();

    let answer = rig.dispatch(json!("release_all")).await.unwrap();
    assert_eq!(answer["tracking"], false);
    assert!(
        summaries(&rig, "awake.lease_ended").is_empty(),
        "nothing visible ended, so no event claims it did"
    );

    let answer = rig.dispatch(tracking(true)).await.unwrap();
    assert_eq!(kinds(&answer), Vec::<String>::new());
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn release_all_while_on_does_not_block_the_next_hold() {
    let rig = Rig::new();
    hook_chain(&rig);
    rig.dispatch(hold()).await.unwrap();
    rig.dispatch(json!("release_all")).await.unwrap();
    let answer = rig.dispatch(hold()).await.unwrap();
    assert_eq!(kinds(&answer), ["session"]);
    assert_eq!(answer["tracking"], true);
    rig.shutdown().await;
}

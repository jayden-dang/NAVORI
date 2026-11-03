#![allow(clippy::unwrap_used)]

mod common;

use std::time::Duration;

use common::{ASSERTION_NAME, ORIGIN_MS, Rig};
use navori_kernel::{ActionError, DispatchError, Severity};
use navori_macos::fake::FakeProcess;
use navori_macos::{AssertionKind, ProcessSource};
use navori_mod_awake::Timing;
use serde_json::{Value, json};

const CLAUDE: i32 = 100;
const CHILD: i32 = 200;
const CLAUDE_START: u64 = 100_000;
const SESSION: &str = "sess-1";
const FIRST_HOLD_US: u64 = ORIGIN_MS * 1_000;
const GRACE: Duration = Duration::from_secs(120);
const RECHECK: Duration = Duration::from_secs(60);
const CAP: Duration = Duration::from_secs(2 * 60 * 60);
const TTL: Duration = Duration::from_secs(8 * 60 * 60);

fn claude(rig: &Rig) {
    rig.processes.add(FakeProcess::new(
        CLAUDE,
        1,
        501,
        CLAUDE_START,
        "/usr/local/bin/claude",
    ));
}

fn child(rig: &Rig, start_us: u64) {
    rig.processes
        .add(FakeProcess::new(CHILD, CLAUDE, 501, start_us, "/bin/sleep"));
}

fn hold_session(session_id: &str) -> Value {
    json!({ "hold_session": { "session_id": session_id, "pid": CLAUDE, "ppid": 1 } })
}

fn stop_session(session_id: &str) -> Value {
    json!({ "stop_session": { "session_id": session_id } })
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

async fn until_lists(rig: &Rig, calls: usize) {
    for _ in 0..50 {
        if rig.processes.list_calls() >= calls {
            rig.idle().await;
            return;
        }
        rig.idle().await;
    }
    panic!(
        "process list was read {} times, wanted at least {calls}",
        rig.processes.list_calls()
    );
}

fn ended(rig: &Rig) -> Vec<(Severity, String)> {
    rig.kernel
        .recent_events(20)
        .unwrap()
        .into_iter()
        .filter(|event| event.kind == "awake.lease_ended")
        .map(|event| (event.severity, event.summary))
        .collect()
}

fn started_count(rig: &Rig) -> usize {
    rig.kernel
        .recent_events(20)
        .unwrap()
        .into_iter()
        .filter(|event| event.kind == "awake.lease_started")
        .count()
}

async fn hold(rig: &Rig) -> Value {
    rig.dispatch(hold_session(SESSION)).await.unwrap()
}

fn assert_held(state: &Value) {
    assert_eq!(state["leases"].as_array().map(Vec::len), Some(1));
    assert_eq!(state["system_held"], true);
    assert_eq!(state["leases"][0]["harness"], "Claude Code");
    assert_eq!(state["leases"][0]["pid"], CLAUDE);
    assert_eq!(state["since_ms"], ORIGIN_MS as i64);
}

fn assert_released(rig: &Rig, state: &Value) {
    assert_eq!(state["leases"].as_array().map(Vec::len), Some(0));
    assert_eq!(state["system_held"], false);
    assert_eq!(state["since_ms"], Value::Null);
    assert!(rig.power.held().is_empty());
    assert_eq!(rig.power.releases(), 1);
    assert!(rig.exits.watched().is_empty());
}

#[tokio::test(start_paused = true)]
async fn only_old_descendants_end_the_lease_after_grace() {
    let rig = Rig::new();
    claude(&rig);
    child(&rig, FIRST_HOLD_US - 1);
    hold(&rig).await;
    let lists = rig.processes.list_calls();

    let stopped = rig.dispatch(stop_session(SESSION)).await.unwrap();

    assert_held(&stopped);
    assert_eq!(stopped["leases"][0]["stopped"], true);
    assert_eq!(rig.processes.list_calls(), lists);
    assert!(ended(&rig).is_empty());
    assert_eq!(
        rig.power.held(),
        vec![(AssertionKind::System, ASSERTION_NAME.to_owned())]
    );
    assert_eq!(rig.exits.watched(), vec![CLAUDE]);

    rig.advance(GRACE - Duration::from_secs(1)).await;
    rig.idle().await;
    let during = awake_state(&rig);
    assert_held(&during);
    assert_eq!(during["leases"][0]["stopped"], true);
    assert_eq!(rig.processes.list_calls(), lists);

    rig.advance(Duration::from_secs(1)).await;
    let state = until(&rig, |state| state["leases"].as_array().unwrap().is_empty()).await;
    assert_released(&rig, &state);
    assert_eq!(
        ended(&rig),
        vec![(
            Severity::Info,
            "Stopped keeping this Mac awake: Claude Code stopped.".to_owned()
        )]
    );
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn a_child_started_at_the_first_hold_is_not_busy() {
    let rig = Rig::new();
    claude(&rig);
    child(&rig, FIRST_HOLD_US);
    hold(&rig).await;

    rig.dispatch(stop_session(SESSION)).await.unwrap();
    rig.advance(GRACE).await;
    let state = until(&rig, |state| state["leases"].as_array().unwrap().is_empty()).await;

    assert_released(&rig, &state);
    assert_eq!(
        ended(&rig),
        vec![(
            Severity::Info,
            "Stopped keeping this Mac awake: Claude Code stopped.".to_owned()
        )]
    );
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn a_new_descendant_keeps_the_lease_and_it_is_read_again_60_seconds_on() {
    let rig = Rig::new();
    claude(&rig);
    child(&rig, FIRST_HOLD_US + 1);
    hold(&rig).await;
    let after_hold = rig.processes.list_calls();
    rig.dispatch(stop_session(SESSION)).await.unwrap();

    rig.advance(GRACE).await;
    until_lists(&rig, after_hold + 2).await;
    let after_grace = rig.processes.list_calls();
    assert_eq!(after_grace, after_hold + 2);
    let kept = awake_state(&rig);
    assert_held(&kept);
    assert_eq!(kept["leases"][0]["stopped"], true);
    assert_eq!(rig.power.releases(), 0);
    assert!(ended(&rig).is_empty());

    rig.advance(RECHECK - Duration::from_secs(1)).await;
    rig.idle().await;
    assert_eq!(rig.processes.list_calls(), after_grace);
    assert_held(&awake_state(&rig));

    rig.advance(Duration::from_secs(1)).await;
    until_lists(&rig, after_grace + 2).await;
    assert_eq!(rig.processes.list_calls(), after_grace + 2);
    assert_held(&awake_state(&rig));
    assert!(ended(&rig).is_empty());
    assert_eq!(rig.exits.watched(), vec![CLAUDE]);
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn the_lease_ends_at_the_next_read_after_the_new_descendant_exits() {
    let rig = Rig::new();
    claude(&rig);
    child(&rig, FIRST_HOLD_US + 1);
    hold(&rig).await;
    let after_hold = rig.processes.list_calls();
    rig.dispatch(stop_session(SESSION)).await.unwrap();

    rig.advance(GRACE).await;
    until_lists(&rig, after_hold + 2).await;
    assert_held(&awake_state(&rig));
    rig.processes.exit(CHILD);

    let after_grace = rig.processes.list_calls();
    rig.advance(RECHECK - Duration::from_secs(1)).await;
    rig.idle().await;
    assert_eq!(rig.processes.list_calls(), after_grace);
    assert_held(&awake_state(&rig));

    rig.advance(Duration::from_secs(1)).await;
    let state = until(&rig, |state| state["leases"].as_array().unwrap().is_empty()).await;
    assert_released(&rig, &state);
    assert_eq!(
        ended(&rig),
        vec![(
            Severity::Info,
            "Stopped keeping this Mac awake: Claude Code stopped.".to_owned()
        )]
    );
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn the_cap_ends_the_lease_with_a_new_descendant_alive() {
    let rig = Rig::new();
    claude(&rig);
    child(&rig, FIRST_HOLD_US + 1);
    hold(&rig).await;
    let after_hold = rig.processes.list_calls();
    rig.dispatch(stop_session(SESSION)).await.unwrap();

    rig.advance(GRACE).await;
    until_lists(&rig, after_hold + 2).await;
    assert_held(&awake_state(&rig));
    assert!(ended(&rig).is_empty());

    rig.advance(CAP - GRACE).await;
    let state = until(&rig, |state| state["leases"].as_array().unwrap().is_empty()).await;
    assert_released(&rig, &state);
    assert!(
        rig.processes.entry(CHILD).is_ok(),
        "the new descendant is still alive"
    );
    assert_eq!(
        ended(&rig),
        vec![(
            Severity::Warn,
            "Stopped keeping this Mac awake: Claude Code was still busy 2 hours after it stopped."
                .to_owned()
        )]
    );
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn the_cap_event_states_the_reason() {
    let rig = Rig::new();
    claude(&rig);
    child(&rig, FIRST_HOLD_US + 1);
    hold(&rig).await;
    rig.dispatch(stop_session(SESSION)).await.unwrap();

    rig.advance(CAP).await;
    let state = until(&rig, |state| state["leases"].as_array().unwrap().is_empty()).await;

    assert_released(&rig, &state);
    assert_eq!(
        ended(&rig),
        vec![(
            Severity::Warn,
            "Stopped keeping this Mac awake: Claude Code was still busy 2 hours after it stopped."
                .to_owned()
        )]
    );
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn a_repeated_stop_keeps_the_stop_mark() {
    let rig = Rig::new();
    claude(&rig);
    child(&rig, FIRST_HOLD_US - 1);
    hold(&rig).await;

    rig.dispatch(stop_session(SESSION)).await.unwrap();
    rig.advance(GRACE - Duration::from_secs(1)).await;
    rig.idle().await;
    let during = awake_state(&rig);
    assert_held(&during);
    assert_eq!(during["leases"][0]["stopped"], true);

    let again = rig.dispatch(stop_session(SESSION)).await.unwrap();
    assert_held(&again);
    assert_eq!(again["leases"][0]["stopped"], true);
    assert!(ended(&rig).is_empty());

    rig.advance(Duration::from_secs(1)).await;
    let state = until(&rig, |state| state["leases"].as_array().unwrap().is_empty()).await;
    assert_released(&rig, &state);
    assert_eq!(
        ended(&rig),
        vec![(
            Severity::Info,
            "Stopped keeping this Mac awake: Claude Code stopped.".to_owned()
        )]
    );
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn a_stop_for_an_unknown_session_changes_nothing() {
    let rig = Rig::new();
    let idle = rig.dispatch(stop_session(SESSION)).await.unwrap();
    assert_eq!(idle["leases"].as_array().map(Vec::len), Some(0));
    assert_eq!(idle["system_held"], false);
    assert!(rig.power.held().is_empty());
    assert_eq!(rig.processes.list_calls(), 0);
    assert!(ended(&rig).is_empty());

    claude(&rig);
    let held = hold(&rig).await;
    let lists = rig.processes.list_calls();
    let watched = rig.exits.watched();
    let releases = rig.power.releases();

    let after = rig.dispatch(stop_session("other-session")).await.unwrap();
    assert_eq!(after, held);
    assert_eq!(after["leases"][0]["stopped"], false);
    assert_eq!(awake_state(&rig), held);
    assert_eq!(rig.processes.list_calls(), lists);
    assert_eq!(rig.exits.watched(), watched);
    assert_eq!(rig.power.releases(), releases);
    assert!(ended(&rig).is_empty());
    assert_eq!(started_count(&rig), 1);

    for session_id in ["", "sess ion", "sess/ion"] {
        let err = rig.dispatch(stop_session(session_id)).await.unwrap_err();
        assert!(
            matches!(
                err,
                DispatchError::Action(ActionError::Invalid(ref message))
                    if message == "session_id must be 1 to 128 bytes of letters, digits, '.', '_', ':', or '-'"
            ),
            "{session_id:?} -> {err:?}"
        );
    }
    assert_eq!(awake_state(&rig), held);
    assert_eq!(rig.processes.list_calls(), lists);
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn a_hold_during_grace_clears_the_stop_mark_and_the_lease_stays() {
    let rig = Rig::new();
    claude(&rig);
    child(&rig, FIRST_HOLD_US + 1);
    hold(&rig).await;
    rig.dispatch(stop_session(SESSION)).await.unwrap();
    let stopped = awake_state(&rig);
    assert_eq!(stopped["leases"][0]["stopped"], true);

    rig.advance(Duration::from_secs(30)).await;
    let renewed = rig.dispatch(hold_session(SESSION)).await.unwrap();
    assert_held(&renewed);
    assert_eq!(renewed["leases"][0]["stopped"], false);
    assert_eq!(
        renewed["leases"][0]["ends_at_ms"],
        ORIGIN_MS as i64 + 30_000 + TTL.as_millis() as i64
    );
    assert_eq!(started_count(&rig), 1);
    assert!(ended(&rig).is_empty());
    assert_eq!(rig.power.releases(), 0);

    rig.advance(CAP).await;
    rig.idle().await;
    let stayed = awake_state(&rig);
    assert_held(&stayed);
    assert_eq!(stayed["leases"][0]["stopped"], false);
    assert!(ended(&rig).is_empty());
    assert_eq!(rig.power.releases(), 0);
    assert_eq!(
        rig.power.held(),
        vec![(AssertionKind::System, ASSERTION_NAME.to_owned())]
    );
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn a_stop_hold_stop_sequence_counts_the_cap_from_the_second_stop() {
    let rig = Rig::new();
    claude(&rig);
    child(&rig, FIRST_HOLD_US + 1);
    hold(&rig).await;

    rig.dispatch(stop_session(SESSION)).await.unwrap();
    rig.advance(Duration::from_secs(60)).await;
    let renewed = rig.dispatch(hold_session(SESSION)).await.unwrap();
    assert_eq!(renewed["leases"][0]["stopped"], false);

    rig.advance(CAP + Duration::from_secs(60)).await;
    rig.idle().await;
    assert_held(&awake_state(&rig));
    assert!(ended(&rig).is_empty());

    rig.dispatch(stop_session(SESSION)).await.unwrap();
    assert_eq!(awake_state(&rig)["leases"][0]["stopped"], true);
    let before_grace = rig.processes.list_calls();
    rig.advance(GRACE).await;
    until_lists(&rig, before_grace + 2).await;
    assert_held(&awake_state(&rig));
    assert!(ended(&rig).is_empty());

    rig.advance(CAP - GRACE).await;
    let state = until(&rig, |state| state["leases"].as_array().unwrap().is_empty()).await;
    assert_released(&rig, &state);
    assert_eq!(
        ended(&rig),
        vec![(
            Severity::Warn,
            "Stopped keeping this Mac awake: Claude Code was still busy 2 hours after it stopped."
                .to_owned()
        )]
    );
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn a_snapshot_that_cannot_be_taken_keeps_the_lease_until_the_cap() {
    let rig = Rig::new();
    claude(&rig);
    child(&rig, FIRST_HOLD_US + 1);
    hold(&rig).await;
    let after_hold = rig.processes.list_calls();
    rig.processes.fail_list(true);
    rig.dispatch(stop_session(SESSION)).await.unwrap();

    rig.advance(GRACE).await;
    until_lists(&rig, after_hold + 1).await;
    let during = awake_state(&rig);
    assert_held(&during);
    assert_eq!(during["leases"][0]["stopped"], true);
    assert!(ended(&rig).is_empty());
    assert_eq!(rig.power.releases(), 0);
    assert!(rig.processes.list_calls() > after_hold);

    rig.advance(CAP - GRACE).await;
    let state = until(&rig, |state| state["leases"].as_array().unwrap().is_empty()).await;
    assert_released(&rig, &state);
    assert_eq!(
        ended(&rig),
        vec![(
            Severity::Warn,
            "Stopped keeping this Mac awake: Claude Code was still busy 2 hours after it stopped."
                .to_owned()
        )]
    );
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn no_cpu_time_is_read() {
    let rig = Rig::new();
    claude(&rig);
    rig.processes.add(
        FakeProcess::new(CHILD, CLAUDE, 501, FIRST_HOLD_US + 1, "/bin/sleep")
            .cpu(Duration::from_secs(5)),
    );
    hold(&rig).await;
    let after_hold = rig.processes.list_calls();
    rig.dispatch(stop_session(SESSION)).await.unwrap();

    rig.advance(GRACE).await;
    until_lists(&rig, after_hold + 2).await;
    assert_held(&awake_state(&rig));
    assert!(
        rig.processes.reads().iter().all(|(_, kind)| *kind != "cpu"),
        "cpu was read: {:?}",
        rig.processes.reads()
    );
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn tests_can_pass_shorter_timing() {
    let rig = Rig::with_timing(Timing {
        grace: Duration::from_secs(1),
        recheck: Duration::from_secs(1),
        cap: Duration::from_secs(5),
        session_ttl: Duration::from_secs(30),
    });
    rig.kernel.scheduler().set_panel_visible(true);
    claude(&rig);
    child(&rig, FIRST_HOLD_US - 1);
    hold(&rig).await;

    let stopped = rig.dispatch(stop_session(SESSION)).await.unwrap();
    assert_held(&stopped);
    assert_eq!(stopped["leases"][0]["stopped"], true);

    rig.advance(Duration::from_secs(1)).await;
    let state = until(&rig, |state| state["leases"].as_array().unwrap().is_empty()).await;
    assert_released(&rig, &state);
    assert_eq!(
        ended(&rig),
        vec![(
            Severity::Info,
            "Stopped keeping this Mac awake: Claude Code stopped.".to_owned()
        )]
    );
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn the_fixed_timing_is_120_60_7200_and_28800_seconds() {
    assert_eq!(Timing::FIXED.grace, Duration::from_secs(120));
    assert_eq!(Timing::FIXED.recheck, Duration::from_secs(60));
    assert_eq!(Timing::FIXED.cap, Duration::from_secs(7_200));
    assert_eq!(Timing::FIXED.session_ttl, Duration::from_secs(28_800));

    let rig = Rig::new();
    claude(&rig);
    child(&rig, FIRST_HOLD_US - 1);
    let held = hold(&rig).await;
    assert_eq!(
        held["leases"][0]["ends_at_ms"],
        ORIGIN_MS as i64 + TTL.as_millis() as i64
    );

    rig.dispatch(stop_session(SESSION)).await.unwrap();
    rig.advance(Duration::from_secs(119)).await;
    rig.idle().await;
    assert_held(&awake_state(&rig));

    rig.advance(Duration::from_secs(1)).await;
    let state = until(&rig, |state| state["leases"].as_array().unwrap().is_empty()).await;
    assert_released(&rig, &state);
    assert_eq!(
        ended(&rig),
        vec![(
            Severity::Info,
            "Stopped keeping this Mac awake: Claude Code stopped.".to_owned()
        )]
    );
    rig.shutdown().await;
}

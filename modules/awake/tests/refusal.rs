#![allow(clippy::unwrap_used)]

mod common;

use std::time::Duration;

use common::{ASSERTION_NAME, Rig};
use navori_macos::AssertionKind;
use navori_macos::fake::FakeProcess;
use serde_json::{Value, json};

const SYSTEM_PROBLEM: &str = "macOS refused the request. Navori keeps trying.";
const DISPLAY_PROBLEM: &str = "macOS refused to keep the display on. Navori keeps trying.";
const RELEASE_PROBLEM: &str = "Releasing a power assertion failed.";
const STORE_PROBLEM: &str = "Awake could not save a lease. Navori retries in 60 seconds.";
const CLAUDE: i32 = 100;
const CLAUDE_START: u64 = 100_000;
const SESSION: &str = "sess-1";

fn hold(seconds: u64) -> Value {
    json!({ "hold_for": { "seconds": seconds } })
}

fn until_off() -> Value {
    json!("hold_until_off")
}

fn display(on: bool) -> Value {
    json!({ "display": { "on": on } })
}

fn release_all() -> Value {
    json!("release_all")
}

fn hold_session(session_id: &str, pid: i32) -> Value {
    json!({ "hold_session": { "session_id": session_id, "pid": pid, "ppid": 1 } })
}

fn lease_count(state: &Value) -> usize {
    state["leases"].as_array().map(Vec::len).unwrap_or(0)
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

fn system_only() -> Vec<(AssertionKind, String)> {
    vec![(AssertionKind::System, ASSERTION_NAME.to_owned())]
}

#[tokio::test(start_paused = true)]
async fn a_refused_system_keeps_the_lease_and_records_the_problem() {
    let rig = Rig::new();
    rig.power.refuse(4);

    let state = rig.dispatch(hold(3600)).await.unwrap();

    assert_eq!(lease_count(&state), 1);
    assert_eq!(state["leases"][0]["key"], "manual");
    assert_eq!(state["leases"][0]["kind"], "duration");
    assert_eq!(state["system_held"], false);
    assert_eq!(state["display_held"], false);
    assert_eq!(state["problem"], json!(SYSTEM_PROBLEM));
    assert!(rig.power.held().is_empty());
    assert_eq!(rig.power.releases(), 0);
    assert_eq!(state, awake_state(&rig));
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn a_hold_answers_with_system_held_false() {
    let rig = Rig::new();
    rig.power.refuse(4);

    let answer = rig.dispatch(until_off()).await.unwrap();

    assert_eq!(answer["system_held"], false);
    assert_eq!(answer["display_held"], false);
    assert_eq!(lease_count(&answer), 1);
    assert_eq!(answer["leases"][0]["kind"], "until_off");
    assert_eq!(answer["problem"], json!(SYSTEM_PROBLEM));
    assert!(rig.power.held().is_empty());
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn refusal_retries_about_every_60_seconds() {
    let rig = Rig::new();
    rig.power.refuse(4);
    let refused = rig.dispatch(until_off()).await.unwrap();
    assert_eq!(refused["system_held"], false);
    assert_eq!(refused["problem"], json!(SYSTEM_PROBLEM));
    assert_eq!(lease_count(&refused), 1);

    rig.power.allow();
    rig.advance(Duration::from_secs(59)).await;
    rig.idle().await;
    let early = awake_state(&rig);
    assert_eq!(early["system_held"], false);
    assert_eq!(early["problem"], json!(SYSTEM_PROBLEM));
    assert_eq!(lease_count(&early), 1);
    assert!(rig.power.held().is_empty());

    rig.advance(Duration::from_secs(1)).await;
    let retried = until(&rig, |state| state["system_held"] == true).await;
    assert_eq!(retried["problem"], Value::Null);
    assert_eq!(lease_count(&retried), 1);
    assert_eq!(rig.power.held(), system_only());
    assert_eq!(rig.power.releases(), 0);
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn a_successful_retry_clears_only_that_problem() {
    let rig = Rig::new();
    rig.power.refuse(4);

    let refused = rig.dispatch(display(true)).await.unwrap();

    assert_eq!(lease_count(&refused), 1);
    assert_eq!(refused["leases"][0]["kind"], "display");
    assert_eq!(refused["system_held"], false);
    assert_eq!(refused["display_held"], false);
    assert_eq!(refused["problem"], json!(SYSTEM_PROBLEM));
    assert!(rig.power.held().is_empty());

    rig.power.allow();
    rig.power.refuse_kind(AssertionKind::Display, 17);
    rig.advance(Duration::from_secs(59)).await;
    rig.idle().await;
    assert_eq!(awake_state(&rig)["problem"], json!(SYSTEM_PROBLEM));
    assert!(rig.power.held().is_empty());

    rig.advance(Duration::from_secs(1)).await;
    let retried = until(&rig, |state| state["system_held"] == true).await;
    assert_eq!(retried["display_held"], false);
    assert_eq!(retried["problem"], json!(DISPLAY_PROBLEM));
    assert_eq!(lease_count(&retried), 1);
    assert_eq!(rig.power.held(), system_only());
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn no_retry_with_no_lease() {
    let rig = Rig::new();
    rig.power.refuse(4);
    let held = rig.dispatch(hold(10)).await.unwrap();
    assert_eq!(held["problem"], json!(SYSTEM_PROBLEM));
    assert_eq!(held["system_held"], false);
    assert_eq!(lease_count(&held), 1);

    rig.advance(Duration::from_secs(10)).await;
    let ended = until(&rig, |state| lease_count(state) == 0).await;
    assert_eq!(ended["system_held"], false);
    assert_eq!(ended["problem"], Value::Null);
    assert!(rig.power.held().is_empty());
    assert_eq!(rig.power.releases(), 0);

    rig.power.allow();
    rig.advance(Duration::from_secs(60)).await;
    rig.idle().await;
    let later = awake_state(&rig);
    assert_eq!(lease_count(&later), 0);
    assert_eq!(later["system_held"], false);
    assert_eq!(later["problem"], Value::Null);
    assert!(rig.power.held().is_empty());
    assert_eq!(rig.power.releases(), 0);
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn a_failed_explicit_release_records_the_problem() {
    let rig = Rig::new();
    rig.dispatch(until_off()).await.unwrap();
    assert_eq!(rig.power.held(), system_only());

    rig.power.fail_release(9);
    let released = rig.dispatch(release_all()).await.unwrap();

    assert_eq!(lease_count(&released), 0);
    assert_eq!(released["system_held"], false);
    assert_eq!(released["display_held"], false);
    assert_eq!(released["problem"], json!(RELEASE_PROBLEM));
    assert!(rig.power.held().is_empty());
    assert_eq!(rig.power.releases(), 1);
    assert_eq!(released, awake_state(&rig));

    rig.idle().await;
    let still = rig.dispatch(json!("status")).await.unwrap();
    assert_eq!(still["problem"], json!(RELEASE_PROBLEM));
    assert_eq!(lease_count(&still), 0);
    assert!(rig.power.held().is_empty());

    rig.advance(Duration::from_secs(60)).await;
    rig.idle().await;
    let later = awake_state(&rig);
    assert_eq!(later["problem"], json!(RELEASE_PROBLEM));
    assert_eq!(lease_count(&later), 0);
    assert!(rig.power.held().is_empty());
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn a_store_problem_survives_a_system_recovery() {
    let rig = Rig::new();
    rig.processes.add(FakeProcess::new(
        CLAUDE,
        1,
        501,
        CLAUDE_START,
        "/usr/local/bin/claude",
    ));
    for _ in 0..50 {
        if table_exists(&rig) {
            break;
        }
        rig.idle().await;
    }
    assert!(table_exists(&rig), "the lease table exists before the drop");
    drop_leases(&rig);
    rig.power.refuse(4);

    let held = rig.dispatch(hold_session(SESSION, CLAUDE)).await.unwrap();
    assert_eq!(lease_count(&held), 1);
    assert_eq!(held["system_held"], false);
    assert_eq!(held["problem"], json!(SYSTEM_PROBLEM));
    assert!(rig.power.held().is_empty());

    rig.power.allow();
    rig.advance(Duration::from_secs(59)).await;
    rig.idle().await;
    let early = awake_state(&rig);
    assert_eq!(early["system_held"], false);
    assert_eq!(early["problem"], json!(SYSTEM_PROBLEM));
    assert!(rig.power.held().is_empty());

    rig.advance(Duration::from_secs(1)).await;
    let recovered = until(&rig, |state| state["system_held"] == true).await;
    assert_eq!(recovered["problem"], json!(STORE_PROBLEM));
    assert_eq!(lease_count(&recovered), 1);
    assert_eq!(rig.power.held(), system_only());
    assert!(rows_empty(&rig), "the store failure is still there");
    rig.shutdown().await;
}

fn table_exists(rig: &Rig) -> bool {
    rig.kernel.store().with_conn(|conn| {
        conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'awake_leases'",
            [],
            |row| row.get::<_, i64>(0),
        )
        .unwrap_or(0)
            > 0
    })
}

fn drop_leases(rig: &Rig) {
    rig.kernel.store().with_conn(|conn| {
        let _ = conn.execute("DROP TABLE awake_leases", []);
    });
}

fn rows_empty(rig: &Rig) -> bool {
    rig.kernel.store().with_conn(|conn| {
        let Ok(mut stmt) = conn.prepare("SELECT COUNT(*) FROM awake_leases") else {
            return true;
        };
        stmt.query_row([], |row| row.get::<_, i64>(0)).unwrap_or(0) == 0
    })
}

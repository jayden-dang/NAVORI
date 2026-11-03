#![allow(clippy::unwrap_used)]

mod common;

use std::time::Duration;

use common::{ASSERTION_NAME, Rig};
use navori_kernel::{DispatchError, ModuleState, RestartPolicy};
use navori_macos::AssertionKind;
use navori_macos::fake::FakeProcess;
use serde_json::{Value, json};

const CLAUDE: i32 = 100;
const CLAUDE_START: u64 = 100_000;
const COMMAND_PID: i32 = 42;
const COMMAND_START: u64 = 5_000;
const SESSION: &str = "sess-1";

fn restarts(max: u32) -> RestartPolicy {
    RestartPolicy {
        max_restarts: max,
        base_delay: Duration::from_millis(10),
        max_delay: Duration::from_millis(10),
    }
}

fn until_off() -> Value {
    json!("hold_until_off")
}

fn display_on() -> Value {
    json!({ "display": { "on": true } })
}

fn hold_session(session_id: &str, pid: i32) -> Value {
    json!({ "hold_session": { "session_id": session_id, "pid": pid, "ppid": 1 } })
}

fn hold_command(pid: i32) -> Value {
    json!({ "hold_command": { "pid": pid } })
}

fn system_only() -> Vec<(AssertionKind, String)> {
    vec![(AssertionKind::System, ASSERTION_NAME.to_owned())]
}

fn system_and_display() -> Vec<(AssertionKind, String)> {
    vec![
        (AssertionKind::System, ASSERTION_NAME.to_owned()),
        (AssertionKind::Display, ASSERTION_NAME.to_owned()),
    ]
}

fn lease_count(state: &Value) -> usize {
    state["leases"].as_array().map(Vec::len).unwrap_or(0)
}

fn awake_failed(rig: &Rig) -> bool {
    rig.kernel
        .modules()
        .into_iter()
        .any(|module| module.id == "awake" && matches!(module.state, ModuleState::Failed { .. }))
}

fn row_count(rig: &Rig) -> i64 {
    rig.kernel.store().with_conn(|conn| {
        conn.query_row("SELECT COUNT(*) FROM awake_leases", [], |row| row.get(0))
            .unwrap_or(-1)
    })
}

async fn until_held(rig: &Rig, pred: impl Fn(&Value) -> bool) -> Value {
    for _ in 0..40 {
        if let Some(state) = rig.kernel.state("awake")
            && pred(&state)
        {
            return state;
        }
        rig.advance(Duration::from_millis(10)).await;
    }
    panic!(
        "held state not reached: {:?} modules={:?}",
        rig.kernel.state("awake"),
        rig.kernel.modules()
    );
}

#[tokio::test(start_paused = true)]
async fn a_panic_keeps_the_leases_and_the_held_assertion() {
    let rig = Rig::with_policy(restarts(5));
    rig.power.panic_on_hold(1);

    let error = rig.dispatch(until_off()).await.unwrap_err();
    assert_eq!(error, DispatchError::Unavailable("awake".into()));
    assert!(rig.power.held().is_empty());

    let state = until_held(&rig, |state| {
        lease_count(state) == 1 && state["system_held"] == json!(true)
    })
    .await;

    assert_eq!(state["leases"][0]["key"], "manual");
    assert_eq!(state["leases"][0]["kind"], "until_off");
    assert_eq!(state["display_held"], false);
    assert_eq!(state["problem"], Value::Null);
    assert_eq!(rig.power.held(), system_only());
    assert_eq!(rig.power.releases(), 0);
    let info = rig
        .kernel
        .modules()
        .into_iter()
        .find(|module| module.id == "awake")
        .unwrap();
    assert_eq!(info.state, ModuleState::Running);
    assert!(info.restarts >= 1, "{info:?}");
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn a_panic_between_a_table_change_and_reconcile_is_repaired_on_reentry() {
    let rig = Rig::with_policy(restarts(5));
    rig.dispatch(until_off()).await.unwrap();
    assert_eq!(rig.power.held(), system_only());

    rig.power.panic_on_hold(1);
    let error = rig.dispatch(display_on()).await.unwrap_err();
    assert_eq!(error, DispatchError::Unavailable("awake".into()));
    assert_eq!(rig.power.held(), system_only());

    let state = until_held(&rig, |state| {
        lease_count(state) == 2
            && state["display_held"] == json!(true)
            && state["system_held"] == json!(true)
    })
    .await;

    assert_eq!(state["leases"][0]["kind"], "until_off");
    assert_eq!(state["leases"][1]["kind"], "display");
    assert_eq!(state["problem"], Value::Null);
    assert_eq!(rig.power.held(), system_and_display());
    assert_eq!(rig.power.releases(), 0);
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn a_panic_mid_restore_restores_every_row_on_reentry() {
    let mut rig = Rig::with_policy(restarts(5));
    rig.processes.add(FakeProcess::new(
        CLAUDE,
        1,
        501,
        CLAUDE_START,
        "/usr/local/bin/claude",
    ));
    rig.processes.add(FakeProcess::new(
        COMMAND_PID,
        1,
        501,
        COMMAND_START,
        "/tmp/feature-awake/sleep",
    ));
    rig.dispatch(hold_session(SESSION, CLAUDE)).await.unwrap();
    rig.dispatch(hold_command(COMMAND_PID)).await.unwrap();
    assert_eq!(row_count(&rig), 2);
    assert_eq!(rig.power.held(), system_only());

    assert_eq!(rig.exits.tokens(COMMAND_PID).len(), 1);
    assert_eq!(rig.exits.tokens(CLAUDE).len(), 1);

    rig.shutdown().await;
    assert!(rig.power.held().is_empty());
    let releases_after_close = rig.power.releases();
    rig.power.panic_on_hold(1);
    rig.reopen().await;

    // Time stays paused, so the restart sleep has not finished. The command
    // key sorts first: that row is watched, and the session row is not.
    for _ in 0..40 {
        if rig.exits.tokens(COMMAND_PID).len() >= 2 {
            break;
        }
        rig.idle().await;
    }
    assert_eq!(
        rig.exits.tokens(COMMAND_PID).len(),
        2,
        "the first stored row is watched before the panic"
    );
    assert_eq!(
        rig.exits.tokens(CLAUDE).len(),
        1,
        "the second stored row is not installed yet"
    );
    assert!(
        rig.power.held().is_empty(),
        "the panicking hold did not keep a guard: {:?}",
        rig.power.held()
    );
    assert_eq!(rig.power.releases(), releases_after_close);
    let crashed = rig
        .kernel
        .modules()
        .into_iter()
        .find(|module| module.id == "awake")
        .unwrap();
    assert!(
        matches!(crashed.state, ModuleState::Restarting { .. }),
        "{crashed:?}"
    );
    assert!(crashed.restarts >= 1, "{crashed:?}");

    let state = until_held(&rig, |state| {
        lease_count(state) == 2 && state["system_held"] == json!(true)
    })
    .await;

    assert_eq!(row_count(&rig), 2);
    assert_eq!(lease_count(&state), 2);
    assert_eq!(state["leases"][0]["kind"], "command");
    assert_eq!(state["leases"][0]["pid"], COMMAND_PID);
    assert_eq!(state["leases"][1]["kind"], "session");
    assert_eq!(state["leases"][1]["pid"], CLAUDE);
    assert_eq!(state["display_held"], false);
    assert_eq!(state["problem"], Value::Null);
    assert_eq!(rig.power.held(), system_only());
    assert_eq!(rig.power.releases(), releases_after_close);
    assert!(
        rig.exits.tokens(CLAUDE).len() >= 2,
        "re-entry watches the session row that the panic skipped"
    );
    let info = rig
        .kernel
        .modules()
        .into_iter()
        .find(|module| module.id == "awake")
        .unwrap();
    assert_eq!(info.state, ModuleState::Running);
    assert!(info.restarts >= 1, "{info:?}");
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn permanent_failure_releases_every_assertion() {
    let rig = Rig::with_policy(restarts(0));
    rig.dispatch(until_off()).await.unwrap();
    assert_eq!(rig.power.held(), system_only());
    assert_eq!(rig.power.releases(), 0);

    rig.power.panic_on_hold(1);
    let error = rig.dispatch(display_on()).await.unwrap_err();
    assert_eq!(error, DispatchError::Unavailable("awake".into()));

    for _ in 0..30 {
        if awake_failed(&rig) {
            break;
        }
        rig.idle().await;
    }
    assert!(awake_failed(&rig), "{:?}", rig.kernel.modules());
    assert!(rig.power.held().is_empty(), "{:?}", rig.power.held());
    assert!(rig.power.releases() >= 1);
    rig.shutdown().await;
}

#![allow(clippy::unwrap_used)]

mod common;

use common::Rig;
use std::time::Duration;

use navori_kernel::{DispatchError, RestartPolicy};
use navori_macos::PowerKind;
use navori_macos::fake::{FakeProcess, LidCall};
use serde_json::{Value, json};

fn state(rig: &Rig) -> Value {
    rig.kernel.state("awake").unwrap()
}

fn stored(rig: &Rig, key: &str) -> Option<Value> {
    rig.store.kv_get("awake", key).unwrap()
}

fn session_rig_with(policy: RestartPolicy) -> Rig {
    let rig = Rig::with_policy(policy);
    rig.power_source.set(PowerKind::Adapter);
    rig.processes.add(FakeProcess::new(
        100,
        1,
        501,
        100_000,
        "/usr/local/bin/claude",
    ));
    rig
}

fn session_rig() -> Rig {
    session_rig_with(RestartPolicy::default())
}

fn hold_session() -> Value {
    json!({ "hold_session": { "session_id": "sess-1", "pid": 100, "ppid": 1 } })
}

fn plant_marker(rig: &Rig) {
    rig.store.kv_set("awake", "lid_held", &json!(true)).unwrap();
}

async fn relaunch_after_a_crash_left_the_marker(rig: &mut Rig) {
    rig.shutdown().await;
    plant_marker(rig);
    rig.reopen().await;
}

#[tokio::test(start_paused = true)]
async fn a_stored_marker_at_start_gives_the_lid_back_then_deletes_it() {
    let mut rig = Rig::new();

    relaunch_after_a_crash_left_the_marker(&mut rig).await;

    assert_eq!(rig.lid.calls(), [LidCall::Clear]);
    assert_eq!(stored(&rig, "lid_held"), None);
    assert_eq!(state(&rig)["lid_marker"], false);
    assert_eq!(state(&rig)["lid_problem"], Value::Null);
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn no_marker_at_start_makes_no_lid_call() {
    let mut rig = Rig::new();

    rig.reopen().await;

    assert_eq!(rig.lid.calls(), []);
    assert_eq!(state(&rig)["lid_problem"], Value::Null);
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn the_restore_runs_before_any_hold_in_that_process() {
    let mut rig = session_rig();
    rig.dispatch(json!({ "lid": { "on": true } }))
        .await
        .unwrap();
    rig.dispatch(hold_session()).await.unwrap();
    rig.shutdown().await;
    plant_marker(&rig);

    rig.reopen().await;
    rig.idle().await;

    assert_eq!(
        rig.lid.calls(),
        [LidCall::Set, LidCall::Clear, LidCall::Clear, LidCall::Set]
    );
    assert_eq!(state(&rig)["lid_held"], true);
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn a_refused_restore_keeps_the_marker_raises_a_lid_problem_and_holds_nothing_until_it_succeeds()
 {
    let mut rig = session_rig();
    rig.dispatch(json!({ "lid": { "on": true } }))
        .await
        .unwrap();
    rig.dispatch(hold_session()).await.unwrap();
    rig.shutdown().await;
    plant_marker(&rig);
    rig.lid.refuse_clear(5);

    rig.reopen().await;
    rig.idle().await;

    assert_eq!(stored(&rig, "lid_held"), Some(json!(true)));
    assert_eq!(state(&rig)["lid_marker"], true);
    assert_eq!(state(&rig)["lid_held"], false);
    assert_ne!(state(&rig)["lid_problem"], Value::Null);
    let before = rig.lid.calls().len();

    rig.advance(Duration::from_secs(60)).await;
    rig.idle().await;
    assert_eq!(rig.lid.calls().len(), before + 1);
    assert_eq!(state(&rig)["lid_held"], false);

    rig.lid.allow();
    rig.advance(Duration::from_secs(60)).await;
    rig.idle().await;

    assert_eq!(state(&rig)["lid_held"], true);
    assert_eq!(state(&rig)["lid_problem"], Value::Null);
    assert_eq!(rig.lid.calls().last(), Some(&LidCall::Set));
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn a_supervised_restart_does_not_restore_again_and_keeps_a_live_hold() {
    let mut rig = session_rig_with(RestartPolicy {
        max_restarts: 5,
        base_delay: Duration::from_millis(10),
        max_delay: Duration::from_millis(10),
    });
    rig.dispatch(json!({ "lid": { "on": true } }))
        .await
        .unwrap();
    rig.shutdown().await;
    plant_marker(&rig);
    rig.reopen().await;
    rig.power.panic_on_hold(1);

    let err = rig.dispatch(hold_session()).await.unwrap_err();
    assert_eq!(err, DispatchError::Unavailable("awake".into()));
    for _ in 0..40 {
        if state_of(&rig)["lid_held"] == json!(true) {
            break;
        }
        rig.advance(Duration::from_millis(10)).await;
    }

    assert_eq!(state(&rig)["lid_held"], true);
    assert_eq!(rig.lid.calls(), [LidCall::Clear, LidCall::Set]);
    rig.shutdown().await;
}

fn state_of(rig: &Rig) -> Value {
    rig.kernel.state("awake").unwrap_or(Value::Null)
}

#[tokio::test(start_paused = true)]
async fn an_unreadable_marker_skips_the_restore_and_raises_a_lid_problem() {
    let mut rig = Rig::new();
    rig.shutdown().await;
    rig.store.with_conn(|conn| {
        conn.execute(
            "INSERT INTO kv (ns, key, value) VALUES ('awake', 'lid_held', '{broken')",
            [],
        )
        .unwrap();
    });

    rig.reopen().await;

    assert_eq!(rig.lid.calls(), []);
    assert_ne!(state(&rig)["lid_problem"], Value::Null);
    assert_eq!(state(&rig)["problem"], Value::Null);
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn an_unreadable_last_sleep_does_not_skip_the_restore() {
    let mut rig = Rig::new();
    rig.shutdown().await;
    plant_marker(&rig);
    rig.store.with_conn(|conn| {
        conn.execute(
            "INSERT INTO kv (ns, key, value) VALUES ('awake', 'last_sleep', '{broken')",
            [],
        )
        .unwrap();
    });

    rig.reopen().await;

    assert_eq!(rig.lid.calls(), [LidCall::Clear]);
    assert_eq!(stored(&rig, "lid_held"), None);
    rig.shutdown().await;
}

#![allow(clippy::unwrap_used)]

mod common;

use common::Rig;
use std::time::Duration;

use navori_kernel::{ActionError, DispatchError};
use navori_macos::PowerKind;
use serde_json::{Value, json};

fn lid(on: bool) -> Value {
    json!({ "lid": { "on": on } })
}

fn state(rig: &Rig) -> Value {
    rig.kernel.state("awake").unwrap()
}

fn stored(rig: &Rig, key: &str) -> Option<Value> {
    rig.store.kv_get("awake", key).unwrap()
}

fn events(rig: &Rig, kind: &str) -> Vec<(String, Value)> {
    rig.kernel
        .recent_events(20)
        .unwrap()
        .into_iter()
        .filter(|event| event.kind == kind)
        .map(|event| (event.summary, event.payload))
        .collect()
}

fn fail_writes_of(rig: &Rig, key: &str) {
    rig.store.with_conn(|conn| {
        conn.execute_batch(&format!(
            "CREATE TRIGGER refuse_{key} BEFORE INSERT ON kv WHEN NEW.key = '{key}' \
             BEGIN SELECT RAISE(ABORT, 'refused'); END;"
        ))
        .unwrap();
    });
}

#[tokio::test(start_paused = true)]
async fn lid_closed_is_off_on_a_fresh_store() {
    let rig = Rig::new();

    let answer = rig.dispatch(json!("status")).await.unwrap();

    assert_eq!(answer["lid"], false);
    assert_eq!(answer["lid_problem"], Value::Null);
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn a_stored_non_boolean_reads_as_off() {
    let mut rig = Rig::new();
    rig.store.kv_set("awake", "lid", &json!("yes")).unwrap();

    rig.reopen().await;

    assert_eq!(state(&rig)["lid"], false);
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn turning_lid_closed_on_stores_it_and_journals_the_change() {
    let rig = Rig::new();

    let answer = rig.dispatch(lid(true)).await.unwrap();

    assert_eq!(answer["lid"], true);
    assert_eq!(stored(&rig, "lid"), Some(json!(true)));
    assert_eq!(
        events(&rig, "awake.lid_changed"),
        [(
            "Lid closed is on: the lid may close without sleep while a Lease holds on the adapter."
                .to_owned(),
            json!({ "on": true })
        )]
    );

    rig.dispatch(lid(false)).await.unwrap();

    assert_eq!(stored(&rig, "lid"), Some(json!(false)));
    assert_eq!(
        events(&rig, "awake.lid_changed")[0],
        (
            "Lid closed is off: closing the lid sleeps this Mac.".to_owned(),
            json!({ "on": false })
        )
    );
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn asking_for_the_current_value_changes_nothing() {
    let rig = Rig::new();
    fail_writes_of(&rig, "lid");

    let answer = rig.dispatch(lid(false)).await.unwrap();

    assert_eq!(answer["lid"], false);
    assert_eq!(stored(&rig, "lid"), None);
    assert_eq!(events(&rig, "awake.lid_changed"), []);
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn a_failed_store_leaves_lid_closed_unchanged_and_fails_the_action() {
    let rig = Rig::new();
    fail_writes_of(&rig, "lid");

    let err = rig.dispatch(lid(true)).await.unwrap_err();

    assert!(matches!(err, DispatchError::Action(ActionError::Failed(_))));
    assert_eq!(state(&rig)["lid"], false);
    assert_eq!(events(&rig, "awake.lid_changed"), []);
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn the_stored_value_is_applied_after_a_relaunch() {
    let mut rig = Rig::new();
    rig.dispatch(lid(true)).await.unwrap();

    rig.reopen().await;

    assert_eq!(state(&rig)["lid"], true);
    assert_eq!(state(&rig)["lid_problem"], Value::Null);
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn an_unreadable_lid_value_reads_as_off_and_raises_a_lid_problem() {
    let mut rig = Rig::new();
    rig.store.with_conn(|conn| {
        conn.execute(
            "INSERT INTO kv (ns, key, value) VALUES ('awake', 'lid', '{broken')",
            [],
        )
        .unwrap();
    });

    rig.reopen().await;

    let state = state(&rig);
    assert_eq!(state["lid"], false);
    assert_eq!(
        state["lid_problem"],
        "Awake could not read the Lid closed setting, so it reads as off."
    );
    assert_eq!(state["problem"], Value::Null);
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn setting_tracking_still_stores_first_and_journals_tracking_changed() {
    let rig = Rig::new();

    rig.dispatch(json!({ "tracking": { "on": false } }))
        .await
        .unwrap();

    assert_eq!(stored(&rig, "tracking"), Some(json!(false)));
    assert_eq!(
        events(&rig, "awake.tracking_changed"),
        [(
            "Tracking agents is off: agent sessions no longer keep this Mac awake.".to_owned(),
            json!({ "on": false })
        )]
    );

    fail_writes_of(&rig, "tracking");
    let err = rig
        .dispatch(json!({ "tracking": { "on": true } }))
        .await
        .unwrap_err();
    assert!(matches!(err, DispatchError::Action(ActionError::Failed(_))));
    assert_eq!(state(&rig)["tracking"], false);
    rig.shutdown().await;
}

fn adapter_rig() -> Rig {
    let rig = Rig::new();
    rig.power_source.set(PowerKind::Adapter);
    rig
}

#[tokio::test(start_paused = true)]
async fn nothing_reads_the_power_source_while_lid_closed_is_off() {
    let rig = adapter_rig();
    rig.dispatch(json!("status")).await.unwrap();
    rig.idle().await;

    assert_eq!(rig.power_source.read_count(), 0);
    assert_eq!(rig.power_source.watch_count(), 0);
    assert_eq!(state(&rig)["on_adapter"], Value::Null);
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn turning_lid_closed_on_watches_first_then_reads_once() {
    let rig = adapter_rig();

    let answer = rig.dispatch(lid(true)).await.unwrap();

    assert_eq!(rig.power_source.watch_count(), 1);
    assert_eq!(rig.power_source.read_count(), 1);
    assert_eq!(answer["on_adapter"], true);
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn a_power_change_reads_the_source_once_and_publishes_only_a_real_change() {
    let rig = adapter_rig();
    rig.dispatch(lid(true)).await.unwrap();

    rig.power_source.fire();
    rig.idle().await;

    assert_eq!(rig.power_source.read_count(), 2);
    assert_eq!(state(&rig)["on_adapter"], true);
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn unplugging_publishes_not_on_the_adapter() {
    let rig = adapter_rig();
    rig.dispatch(lid(true)).await.unwrap();

    rig.power_source.set(PowerKind::Battery);
    rig.power_source.fire();
    rig.idle().await;

    assert_eq!(state(&rig)["on_adapter"], false);
    assert_eq!(state(&rig)["lid_problem"], Value::Null);
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn a_dead_watch_reads_as_battery_raises_a_lid_problem_and_retries() {
    let rig = adapter_rig();
    rig.dispatch(lid(true)).await.unwrap();

    rig.power_source.kill();
    rig.idle().await;

    assert_eq!(state(&rig)["on_adapter"], false);
    assert_ne!(state(&rig)["lid_problem"], Value::Null);
    assert_eq!(rig.power_source.watch_count(), 1);

    rig.advance(Duration::from_secs(60)).await;
    rig.idle().await;

    assert_eq!(rig.power_source.watch_count(), 2);
    assert_eq!(state(&rig)["on_adapter"], true);
    assert_eq!(state(&rig)["lid_problem"], Value::Null);
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn a_failed_watch_at_arming_reads_as_battery_and_retries() {
    let rig = adapter_rig();
    rig.power_source.fail_watch();

    let answer = rig.dispatch(lid(true)).await.unwrap();

    assert_eq!(answer["on_adapter"], false);
    assert_ne!(answer["lid_problem"], Value::Null);
    assert_eq!(rig.power_source.read_count(), 0);

    rig.advance(Duration::from_secs(60)).await;
    rig.idle().await;

    assert_eq!(rig.power_source.watch_count(), 2);
    assert_eq!(state(&rig)["on_adapter"], false);
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn turning_lid_closed_off_drops_the_watch() {
    let rig = adapter_rig();
    rig.dispatch(lid(true)).await.unwrap();

    let answer = rig.dispatch(lid(false)).await.unwrap();
    rig.power_source.fire();
    rig.idle().await;

    assert_eq!(answer["on_adapter"], Value::Null);
    assert_eq!(rig.power_source.read_count(), 1);
    assert_eq!(rig.power_source.drain_count(), 0);
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn lid_closed_stored_on_is_watched_after_a_relaunch() {
    let mut rig = adapter_rig();
    rig.dispatch(lid(true)).await.unwrap();

    rig.reopen().await;

    assert_eq!(rig.power_source.watch_count(), 2);
    assert_eq!(state(&rig)["on_adapter"], true);
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn an_idle_watch_does_not_spin() {
    let rig = adapter_rig();
    rig.dispatch(lid(true)).await.unwrap();

    for _ in 0..10 {
        rig.idle().await;
    }

    assert!(rig.power_source.drain_count() <= 1);
    rig.shutdown().await;
}

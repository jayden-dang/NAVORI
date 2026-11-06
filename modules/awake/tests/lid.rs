#![allow(clippy::unwrap_used)]

mod common;

use common::Rig;
use navori_kernel::{ActionError, DispatchError};
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

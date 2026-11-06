#![allow(clippy::unwrap_used)]

mod common;

use common::Rig;
use serde_json::{Value, json};

const SLEEP_AT_MS: i64 = 1_793_931_600_000;

fn slept(at_ms: i64) -> Value {
    json!({ "slept": { "at_ms": at_ms } })
}

fn state(rig: &Rig) -> Value {
    rig.kernel.state("awake").unwrap()
}

fn sleeps(rig: &Rig) -> Vec<(String, Value)> {
    rig.kernel
        .recent_events(20)
        .unwrap()
        .into_iter()
        .filter(|event| event.kind == "awake.slept")
        .map(|event| (event.summary, event.payload))
        .collect()
}

#[tokio::test(start_paused = true)]
async fn a_recorded_sleep_is_published_and_journaled_with_its_reason() {
    let rig = Rig::new();
    rig.lid.set_reason(Some("Clamshell Sleep"));

    rig.dispatch(slept(SLEEP_AT_MS)).await.unwrap();

    assert_eq!(state(&rig)["last_sleep_ms"], SLEEP_AT_MS);
    assert_eq!(
        sleeps(&rig),
        [(
            "The Mac went to sleep.".to_owned(),
            json!({ "at_ms": 1_793_931_600_000_i64, "reason": "Clamshell Sleep" })
        )]
    );
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn an_unreadable_reason_is_journaled_as_null() {
    let rig = Rig::new();
    rig.lid.set_reason(None);

    rig.dispatch(slept(SLEEP_AT_MS)).await.unwrap();

    assert_eq!(
        sleeps(&rig)[0].1,
        json!({ "at_ms": 1_793_931_600_000_i64, "reason": null })
    );
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn the_last_sleep_survives_a_relaunch() {
    let mut rig = Rig::new();
    rig.dispatch(slept(SLEEP_AT_MS)).await.unwrap();

    rig.reopen().await;

    assert_eq!(state(&rig)["last_sleep_ms"], SLEEP_AT_MS);
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn every_slept_call_counts() {
    let rig = Rig::new();

    rig.dispatch(slept(SLEEP_AT_MS)).await.unwrap();
    rig.dispatch(slept(SLEEP_AT_MS + 60_000)).await.unwrap();

    assert_eq!(sleeps(&rig).len(), 2);
    assert_eq!(state(&rig)["last_sleep_ms"], SLEEP_AT_MS + 60_000);
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn a_failed_store_still_publishes_the_sleep_and_raises_a_lid_problem() {
    let rig = Rig::new();
    rig.store.with_conn(|conn| {
        conn.execute_batch(
            "CREATE TRIGGER refuse_last_sleep BEFORE INSERT ON kv WHEN NEW.key = 'last_sleep' \
             BEGIN SELECT RAISE(ABORT, 'refused'); END;",
        )
        .unwrap();
    });

    rig.dispatch(slept(SLEEP_AT_MS)).await.unwrap();

    assert_eq!(state(&rig)["last_sleep_ms"], SLEEP_AT_MS);
    assert_ne!(state(&rig)["lid_problem"], Value::Null);
    assert_eq!(sleeps(&rig).len(), 1);
    rig.shutdown().await;
}

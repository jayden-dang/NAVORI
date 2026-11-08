//! Status by local calendar day, including midnight and wake.
#![allow(clippy::unwrap_used)]

mod common;

use std::time::Duration;

use common::{day_plus, harness, today, until_next_local_midnight};
use serde_json::json;

#[tokio::test(start_paused = true)]
async fn status_follows_the_local_day() {
    let cases = [
        (Some(day_plus(-1)), "overdue"),
        (Some(day_plus(0)), "today"),
        (Some(day_plus(1)), "soon"),
        (Some(day_plus(3)), "soon"),
        (Some(day_plus(4)), "later"),
        (None, "none"),
    ];

    for (due_on, status) in cases {
        let h = harness();
        h.idle().await;
        let due = due_on.as_deref();
        h.add("Item", due).await.unwrap();
        let state = h.state();
        assert_eq!(state["today"], today(), "due={due:?}");
        let open = &state["open"][0];
        assert_eq!(open["status"], status, "due={due:?} state={state}");
        match due {
            Some(d) => assert_eq!(open["due_on"], d, "{state}"),
            None => assert!(open["due_on"].is_null(), "{state}"),
        }
        h.shutdown().await;
    }
}

#[tokio::test(start_paused = true)]
async fn status_changes_at_local_midnight_and_after_a_wake() {
    let h = harness();
    h.idle().await;

    h.add("Due today", Some(today())).await.unwrap();
    assert_eq!(h.state()["open"][0]["status"], "today");
    assert_eq!(h.state()["today"], today());

    // Timer path: both clocks past the next local midnight.
    h.advance(until_next_local_midnight() + Duration::from_secs(1))
        .await;
    h.settle().await;

    let state = h.state();
    assert_eq!(state["today"], "2023-11-16", "{state}");
    assert_eq!(state["open"][0]["status"], "overdue", "{state}");

    // Wake path: wall clock jumps further (long sleep); pulse fires Due at once.
    h.add("Due on the new today", Some("2023-11-16"))
        .await
        .unwrap();
    let before = h.state();
    let new_id = before["open"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["title"] == "Due on the new today")
        .unwrap()["id"]
        .as_i64()
        .unwrap();
    assert_eq!(
        before["open"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["id"] == new_id)
            .unwrap()["status"],
        "today",
        "{before}"
    );

    // Wall-only jump of one full local day (Mac slept past midnight).
    h.clock.advance(Duration::from_secs(24 * 60 * 60));
    h.kernel.scheduler().wake_pulse();
    h.settle().await;

    let after = h.state();
    assert_eq!(after["today"], "2023-11-17", "{after}");
    let woke = after["open"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["id"] == json!(new_id))
        .unwrap();
    assert_eq!(woke["status"], "overdue", "{after}");

    h.shutdown().await;
}

//! Mark Updates viewed and clear them.
#![allow(clippy::unwrap_used)]

mod common;

use common::{ORIGIN_MS, harness};
use navori_kernel::Envelope;
use serde_json::json;
use std::time::Duration;

async fn expire_two_updates(h: &common::Harness) -> (i64, i64) {
    h.raise(json!({
        "raise": {
            "key": "a",
            "title": "Alpha",
            "reason": "first",
            "expires_at_ms": ORIGIN_MS + 1_000,
            "primary": { "label": "Yes" },
            "secondary": { "label": "No" }
        }
    }))
    .await
    .unwrap();
    h.raise(json!({
        "raise": {
            "key": "b",
            "title": "Beta",
            "reason": "second",
            "expires_at_ms": ORIGIN_MS + 1_000,
            "primary": { "label": "Yes" },
            "secondary": { "label": "No" }
        }
    }))
    .await
    .unwrap();

    h.advance(Duration::from_millis(1_000)).await;
    h.settle().await;

    let updates = h.state()["updates"].as_array().unwrap().clone();
    assert_eq!(updates.len(), 2, "{}", h.state());
    (
        updates[0]["id"].as_i64().unwrap(),
        updates[1]["id"].as_i64().unwrap(),
    )
}

#[tokio::test(start_paused = true)]
async fn marking_viewed_hides_exactly_those_updates() {
    let h = harness();
    h.idle().await;
    let (first, second) = expire_two_updates(&h).await;

    h.kernel
        .dispatch(Envelope {
            module: "needs_you".into(),
            action: json!({ "mark_viewed": { "ids": [first] } }),
        })
        .await
        .unwrap();

    let state = h.state();
    let updates = state["updates"].as_array().unwrap();
    assert_eq!(updates.len(), 1, "{state}");
    assert_eq!(updates[0]["id"], second);
    assert!(!h.kernel.tray(None).attention);

    h.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn an_update_never_marked_stays() {
    let h = harness();
    h.idle().await;
    let (first, second) = expire_two_updates(&h).await;

    h.kernel
        .dispatch(Envelope {
            module: "needs_you".into(),
            action: json!({ "mark_viewed": { "ids": [first] } }),
        })
        .await
        .unwrap();

    let state = h.state();
    let updates = state["updates"].as_array().unwrap();
    assert_eq!(updates.len(), 1, "{state}");
    assert_eq!(
        updates[0]["id"], second,
        "unmarked Update must stay: {state}"
    );

    h.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn clear_updates_resolves_every_update() {
    let h = harness();
    h.idle().await;
    let _ = expire_two_updates(&h).await;

    h.kernel
        .dispatch(Envelope {
            module: "needs_you".into(),
            action: json!("clear_updates"),
        })
        .await
        .unwrap();

    let state = h.state();
    assert!(
        state["updates"].as_array().unwrap().is_empty(),
        "cleared Updates must leave State: {state}"
    );
    assert!(!h.kernel.tray(None).attention);

    h.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn viewed_and_cleared_updates_stay_in_history_rows() {
    let h = harness();
    h.idle().await;
    let (first, second) = expire_two_updates(&h).await;

    h.kernel
        .dispatch(Envelope {
            module: "needs_you".into(),
            action: json!({ "mark_viewed": { "ids": [first] } }),
        })
        .await
        .unwrap();
    h.kernel
        .dispatch(Envelope {
            module: "needs_you".into(),
            action: json!("clear_updates"),
        })
        .await
        .unwrap();

    let rows: Vec<(i64, String, Option<i64>)> = h
        .store
        .with_conn(|conn| {
            let mut stmt = conn
                .prepare("SELECT id, resolution, resolved_at_ms FROM needs_you_updates ORDER BY id")
                .unwrap();
            let mapped = stmt
                .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
                .unwrap();
            mapped.collect::<Result<Vec<_>, _>>()
        })
        .unwrap();

    assert_eq!(rows.len(), 2);
    let viewed = rows.iter().find(|(id, _, _)| *id == first).unwrap();
    let cleared = rows.iter().find(|(id, _, _)| *id == second).unwrap();
    assert_eq!(viewed.1, "viewed");
    assert_eq!(cleared.1, "cleared");
    assert_eq!(viewed.2, Some(ORIGIN_MS as i64 + 1_000));
    assert_eq!(cleared.2, Some(ORIGIN_MS as i64 + 1_000));

    h.shutdown().await;
}

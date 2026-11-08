//! A Decision expires into an Update.
#![allow(clippy::unwrap_used)]

mod common;

use common::{ORIGIN_MS, harness};
use navori_kernel::ModuleAction;
use navori_mod_needs_you::NeedsYouAction;
use navori_mod_needs_you::stored::{self, RaiseRow};
use navori_store::{Migration, Store};
use serde_json::json;
use std::time::Duration;

#[tokio::test(start_paused = true)]
async fn a_decision_without_expiry_stays_open() {
    let h = harness();
    h.idle().await;

    h.raise(json!({
        "raise": {
            "key": "k1",
            "title": "No deadline",
            "reason": "stay",
            "primary": { "label": "Yes" },
            "secondary": { "label": "No" }
        }
    }))
    .await
    .unwrap();

    h.advance(Duration::from_secs(3600)).await;
    h.settle().await;

    let state = h.state();
    assert_eq!(state["decisions"].as_array().unwrap().len(), 1, "{state}");
    assert!(state["updates"].as_array().unwrap().is_empty(), "{state}");

    h.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn an_expired_decision_becomes_an_expired_update() {
    let h = harness();
    h.idle().await;

    let reply = h
        .raise(json!({
            "raise": {
                "key": "k1",
                "title": "Ship it",
                "reason": "window",
                "expires_at_ms": ORIGIN_MS + 1_000,
                "primary": { "label": "Yes" },
                "secondary": { "label": "No" }
            }
        }))
        .await
        .unwrap();
    let id = reply["id"].as_i64().unwrap();

    h.advance(Duration::from_millis(1_000)).await;
    h.settle().await;

    let state = h.state();
    assert!(
        state["decisions"].as_array().unwrap().is_empty(),
        "expired Decision must leave State: {state}"
    );
    let updates = state["updates"].as_array().unwrap();
    assert_eq!(updates.len(), 1, "{state}");
    assert_eq!(updates[0]["kind"], "decision_expired");
    assert_eq!(updates[0]["title"], "Expired: Ship it");
    assert_eq!(updates[0]["created_at_ms"], ORIGIN_MS + 1_000);

    let db_state: String = h
        .store
        .with_conn(|conn| {
            conn.query_row(
                "SELECT state FROM needs_you_decisions WHERE id = ?1",
                [id],
                |row| row.get(0),
            )
        })
        .unwrap();
    assert_eq!(db_state, "expired");

    h.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn a_decision_answering_does_not_expire() {
    let h = harness();
    h.idle().await;

    let reply = h
        .raise(json!({
            "raise": {
                "key": "k1",
                "title": "In flight",
                "reason": "answering",
                "expires_at_ms": ORIGIN_MS + 1_000,
                "primary": { "label": "Yes" },
                "secondary": { "label": "No" }
            }
        }))
        .await
        .unwrap();
    let id = reply["id"].as_i64().unwrap();
    stored::mark_answering(&h.store, id, "primary").unwrap();

    h.advance(Duration::from_millis(1_000)).await;
    h.settle().await;

    let db_state: String = h
        .store
        .with_conn(|conn| {
            conn.query_row(
                "SELECT state FROM needs_you_decisions WHERE id = ?1",
                [id],
                |row| row.get(0),
            )
        })
        .unwrap();
    assert_eq!(db_state, "answering");
    let updates: i64 = h
        .store
        .with_conn(|conn| {
            conn.query_row("SELECT COUNT(*) FROM needs_you_updates", [], |row| {
                row.get(0)
            })
        })
        .unwrap();
    assert_eq!(updates, 0);

    h.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn a_failed_decision_expires() {
    let h = harness();
    h.idle().await;

    let reply = h
        .raise(json!({
            "raise": {
                "key": "k1",
                "title": "Retry later",
                "reason": "failed",
                "expires_at_ms": ORIGIN_MS + 2_000,
                "primary": {
                    "label": "Poke",
                    "envelope": { "module": "target", "action": "fail" }
                },
                "secondary": { "label": "No" }
            }
        }))
        .await
        .unwrap();
    let id = reply["id"].as_i64().unwrap();
    let revision = reply["revision"].as_u64().unwrap() as u32;
    h.answer(id, revision, "primary").await.unwrap();
    h.settle().await;

    assert!(
        h.state()["decisions"][0]["state"].get("failed").is_some(),
        "{}",
        h.state()
    );

    h.advance(Duration::from_millis(2_000)).await;
    h.settle().await;

    let state = h.state();
    assert!(state["decisions"].as_array().unwrap().is_empty(), "{state}");
    let updates = state["updates"].as_array().unwrap();
    assert_eq!(updates.len(), 1, "{state}");
    assert_eq!(updates[0]["kind"], "decision_expired");
    assert_eq!(updates[0]["title"], "Expired: Retry later");

    h.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn a_past_deadline_expires_at_start() {
    let mut h = harness();
    h.idle().await;

    let reply = h
        .raise(json!({
            "raise": {
                "key": "k1",
                "title": "While away",
                "reason": "sleep",
                "expires_at_ms": ORIGIN_MS + 500,
                "primary": { "label": "Yes" },
                "secondary": { "label": "No" }
            }
        }))
        .await
        .unwrap();
    let id = reply["id"].as_i64().unwrap();

    h.clock.advance(Duration::from_millis(500));
    h.reopen().await;

    let state = h.state();
    assert!(state["decisions"].as_array().unwrap().is_empty(), "{state}");
    let updates = state["updates"].as_array().unwrap();
    assert_eq!(updates.len(), 1, "{state}");
    assert_eq!(updates[0]["title"], "Expired: While away");

    let db_state: String = h
        .store
        .with_conn(|conn| {
            conn.query_row(
                "SELECT state FROM needs_you_decisions WHERE id = ?1",
                [id],
                |row| row.get(0),
            )
        })
        .unwrap();
    assert_eq!(db_state, "expired");

    h.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn a_wake_after_the_deadline_expires_it_at_once() {
    let h = harness();
    h.idle().await;

    h.raise(json!({
        "raise": {
            "key": "k1",
            "title": "Wake me",
            "reason": "pulse",
            "expires_at_ms": ORIGIN_MS + 1_000,
            "primary": { "label": "Yes" },
            "secondary": { "label": "No" }
        }
    }))
    .await
    .unwrap();

    h.clock.advance(Duration::from_millis(1_000));
    h.kernel.scheduler().wake_pulse();
    h.settle().await;

    let state = h.state();
    assert!(state["decisions"].as_array().unwrap().is_empty(), "{state}");
    let updates = state["updates"].as_array().unwrap();
    assert_eq!(updates.len(), 1, "{state}");
    assert_eq!(updates[0]["title"], "Expired: Wake me");

    h.shutdown().await;
}

#[test]
fn no_action_posts_an_update() {
    let names: Vec<&str> = NeedsYouAction::catalog()
        .into_iter()
        .map(|doc| doc.name)
        .collect();
    assert_eq!(names, vec!["raise", "answer"]);
}

#[tokio::test(start_paused = true)]
async fn updates_never_turn_on_attention() {
    let h = harness();
    h.idle().await;

    h.raise(json!({
        "raise": {
            "key": "k1",
            "title": "Quiet",
            "reason": "update only",
            "expires_at_ms": ORIGIN_MS + 1_000,
            "primary": { "label": "Yes" },
            "secondary": { "label": "No" }
        }
    }))
    .await
    .unwrap();
    assert!(h.kernel.tray(None).attention);

    h.advance(Duration::from_millis(1_000)).await;
    h.settle().await;

    let state = h.state();
    assert!(state["decisions"].as_array().unwrap().is_empty(), "{state}");
    assert_eq!(state["updates"].as_array().unwrap().len(), 1, "{state}");
    assert!(!h.kernel.tray(None).attention);

    h.shutdown().await;
}

#[test]
fn migration_v2_keeps_v1_decisions_and_backs_up() {
    let root = std::env::temp_dir().join(format!(
        "navori-needs-you-mig-{}-{}",
        std::process::id(),
        ORIGIN_MS
    ));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let db = root.join("navori.db");
    let backups = root.join("backups");

    let store = Store::open(&db, &backups).unwrap();
    let v1: &[Migration] = &stored::MIGRATIONS[..1];
    store.migrate("needs_you", v1).unwrap();
    let (id, _) = stored::insert_open(
        &store,
        &RaiseRow {
            key: "keep".into(),
            title: "Keep me".into(),
            reason: "v1 row".into(),
            raised_at_ms: ORIGIN_MS as i64,
            expires_at_ms: None,
            primary_label: "A".into(),
            primary_envelope: None,
            primary_risk: None,
            secondary_label: "B".into(),
            secondary_envelope: None,
            secondary_risk: None,
        },
    )
    .unwrap();

    let report = stored::migrate(&store).unwrap();
    assert_eq!(report.applied, vec![2]);
    assert!(report.backup.is_some(), "upgrade from v1 must back up");

    let title: String = store
        .with_conn(|conn| {
            conn.query_row(
                "SELECT title FROM needs_you_decisions WHERE id = ?1",
                [id],
                |row| row.get(0),
            )
        })
        .unwrap();
    assert_eq!(title, "Keep me");

    let has_updates: bool =
        store.with_conn(|conn| conn.prepare("SELECT 1 FROM needs_you_updates").is_ok());
    assert!(has_updates);

    let _ = std::fs::remove_dir_all(&root);
}

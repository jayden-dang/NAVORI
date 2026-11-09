//! Answer a Decision once, from the Core.
#![allow(clippy::unwrap_used)]

mod common;

use common::{ORIGIN_MS, harness, harness_with_timeout};
use navori_kernel::DispatchError;
use navori_mod_needs_you::stored;
use serde_json::json;
use std::time::Duration;

const STOPPED_MID_ANSWER: &str = "Navori stopped before this finished — it may already have run";

async fn raise_poke(h: &common::Harness) -> (i64, u32) {
    let reply = h
        .raise(json!({
            "raise": {
                "key": "k1",
                "title": "Review",
                "reason": "needed",
                "primary": {
                    "label": "Poke",
                    "envelope": { "module": "target", "action": "poke" }
                },
                "secondary": { "label": "Later" }
            }
        }))
        .await
        .unwrap();
    (
        reply["id"].as_i64().unwrap(),
        reply["revision"].as_u64().unwrap() as u32,
    )
}

#[tokio::test(start_paused = true)]
async fn answering_runs_the_chosen_envelope_and_moves_it_to_history() {
    let h = harness();
    h.idle().await;

    let (id, revision) = raise_poke(&h).await;
    h.answer(id, revision, "primary").await.unwrap();
    h.settle().await;

    assert_eq!(h.target_state()["seen"], json!(["poke"]));
    assert!(h.state()["decisions"].as_array().unwrap().is_empty());

    let answered = h
        .store
        .with_conn(|conn| {
            conn.query_row(
                "SELECT state, chosen, resolved_at_ms FROM needs_you_decisions WHERE id = ?1",
                [id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, Option<String>>(1)?,
                        row.get::<_, Option<i64>>(2)?,
                    ))
                },
            )
        })
        .unwrap();
    assert_eq!(answered.0, "answered");
    assert_eq!(answered.1.as_deref(), Some("primary"));
    assert_eq!(answered.2, Some(ORIGIN_MS as i64));

    h.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn a_choice_without_an_envelope_closes_the_decision() {
    let h = harness();
    h.idle().await;

    let reply = h
        .raise(json!({
            "raise": {
                "key": "k1",
                "title": "Close me",
                "reason": "no run",
                "primary": { "label": "Done" },
                "secondary": { "label": "Later" }
            }
        }))
        .await
        .unwrap();
    let id = reply["id"].as_i64().unwrap();
    let revision = reply["revision"].as_u64().unwrap() as u32;

    h.answer(id, revision, "primary").await.unwrap();
    h.settle().await;

    assert!(h.state()["decisions"].as_array().unwrap().is_empty());
    assert_eq!(h.target_state()["seen"], json!([]));
    let state: String = h
        .store
        .with_conn(|conn| {
            conn.query_row(
                "SELECT state FROM needs_you_decisions WHERE id = ?1",
                [id],
                |row| row.get(0),
            )
        })
        .unwrap();
    assert_eq!(state, "answered");

    h.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn a_failed_dispatch_keeps_it_open_with_the_error() {
    let h = harness();
    h.idle().await;

    let reply = h
        .raise(json!({
            "raise": {
                "key": "k1",
                "title": "Fail",
                "reason": "boom",
                "primary": {
                    "label": "Fail",
                    "envelope": { "module": "target", "action": "fail" }
                },
                "secondary": { "label": "Later" }
            }
        }))
        .await
        .unwrap();
    let id = reply["id"].as_i64().unwrap();
    let revision = reply["revision"].as_u64().unwrap() as u32;

    h.answer(id, revision, "primary").await.unwrap();
    h.settle().await;

    let d = &h.state()["decisions"][0];
    assert_eq!(d["state"]["failed"]["choice"], "primary");
    let error = d["state"]["failed"]["error"].as_str().unwrap();
    assert!(error.contains("target refused"), "{error}");
    assert!(!error.contains("may already have run"), "{error}");

    h.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn a_timeout_says_it_may_already_have_run() {
    let h = harness_with_timeout(Duration::from_millis(50));
    h.idle().await;

    let reply = h
        .raise(json!({
            "raise": {
                "key": "k1",
                "title": "Stall",
                "reason": "hang",
                "primary": {
                    "label": "Stall",
                    "envelope": { "module": "target", "action": "stall" }
                },
                "secondary": { "label": "Later" }
            }
        }))
        .await
        .unwrap();
    let id = reply["id"].as_i64().unwrap();
    let revision = reply["revision"].as_u64().unwrap() as u32;

    h.answer(id, revision, "primary").await.unwrap();
    tokio::time::advance(Duration::from_millis(80)).await;
    h.settle().await;

    let error = h.state()["decisions"][0]["state"]["failed"]["error"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(error.contains("may already have run"), "{error}");

    h.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn retry_answers_again_with_the_failed_choice() {
    let mut h = harness();
    h.idle().await;

    let reply = h
        .raise(json!({
            "raise": {
                "key": "k1",
                "title": "Retry",
                "reason": "again",
                "primary": {
                    "label": "Poke",
                    "envelope": { "module": "target", "action": "poke" }
                },
                "secondary": { "label": "Later" }
            }
        }))
        .await
        .unwrap();
    let id = reply["id"].as_i64().unwrap();
    let revision = reply["revision"].as_u64().unwrap() as u32;

    stored::mark_failed(&h.store, id, "primary", "earlier failure").unwrap();
    // Reload published state after the direct store write.
    h.reopen().await;

    h.answer(id, revision, "primary").await.unwrap();
    h.settle().await;

    assert_eq!(h.target_state()["seen"], json!(["poke"]));
    assert!(h.state()["decisions"].as_array().unwrap().is_empty());

    h.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn refused_answers_dispatch_nothing() {
    // unknown id
    {
        let h = harness();
        h.idle().await;
        let err = h.answer(999, 1, "primary").await.expect_err("unknown id");
        assert!(matches!(err, DispatchError::Action(_)), "{err:?}");
        assert_eq!(h.target_state()["seen"], json!([]));
        h.shutdown().await;
    }

    // answered Decision
    {
        let h = harness();
        h.idle().await;
        let (id, revision) = raise_poke(&h).await;
        h.answer(id, revision, "secondary").await.unwrap();
        h.settle().await;
        let err = h
            .answer(id, revision, "primary")
            .await
            .expect_err("answered");
        assert!(matches!(err, DispatchError::Action(_)), "{err:?}");
        assert_eq!(h.target_state()["seen"], json!([]));
        h.shutdown().await;
    }

    // one already answering
    {
        let h = harness();
        h.idle().await;
        let (id, revision) = raise_poke(&h).await;
        stored::mark_answering(&h.store, id, "primary").unwrap();
        let err = h
            .answer(id, revision, "primary")
            .await
            .expect_err("answering");
        assert!(matches!(err, DispatchError::Action(_)), "{err:?}");
        assert_eq!(h.target_state()["seen"], json!([]));
        h.shutdown().await;
    }

    // stale revision after a re-raise
    {
        let h = harness();
        h.idle().await;
        let first = h
            .raise(json!({
                "raise": {
                    "key": "k1",
                    "title": "First",
                    "reason": "r",
                    "primary": {
                        "label": "Poke",
                        "envelope": { "module": "target", "action": "poke" }
                    },
                    "secondary": { "label": "Later" }
                }
            }))
            .await
            .unwrap();
        let id = first["id"].as_i64().unwrap();
        let stale = first["revision"].as_u64().unwrap() as u32;
        h.raise(json!({
            "raise": {
                "key": "k1",
                "title": "Second",
                "reason": "r2",
                "primary": {
                    "label": "Poke",
                    "envelope": { "module": "target", "action": "poke" }
                },
                "secondary": { "label": "Later" }
            }
        }))
        .await
        .unwrap();
        let err = h.answer(id, stale, "primary").await.expect_err("stale");
        assert!(matches!(err, DispatchError::Action(_)), "{err:?}");
        assert_eq!(h.target_state()["seen"], json!([]));
        h.shutdown().await;
    }

    // answer at or after expires_at_ms
    {
        let h = harness();
        h.idle().await;
        let reply = h
            .raise(json!({
                "raise": {
                    "key": "k1",
                    "title": "Expiring",
                    "reason": "soon",
                    "expires_at_ms": ORIGIN_MS + 1_000,
                    "primary": {
                        "label": "Poke",
                        "envelope": { "module": "target", "action": "poke" }
                    },
                    "secondary": { "label": "Later" }
                }
            }))
            .await
            .unwrap();
        let id = reply["id"].as_i64().unwrap();
        let revision = reply["revision"].as_u64().unwrap() as u32;
        h.clock.advance(Duration::from_millis(1_000));
        let err = h
            .answer(id, revision, "primary")
            .await
            .expect_err("expired");
        assert!(matches!(err, DispatchError::Action(_)), "{err:?}");
        assert_eq!(h.target_state()["seen"], json!([]));
        let state = h.state();
        assert!(
            state["decisions"].as_array().unwrap().is_empty(),
            "expired Decision must leave State: {state}"
        );
        let updates = state["updates"].as_array().unwrap();
        assert_eq!(updates.len(), 1, "{state}");
        assert_eq!(updates[0]["kind"], "decision_expired");
        assert_eq!(updates[0]["title"], "Expired: Expiring");
        h.shutdown().await;
    }
}

#[tokio::test(start_paused = true)]
async fn a_restart_mid_answer_marks_it_failed_and_never_reruns() {
    let mut h = harness_with_timeout(Duration::from_secs(5));
    h.idle().await;

    let reply = h
        .raise(json!({
            "raise": {
                "key": "k1",
                "title": "Mid",
                "reason": "restart",
                "primary": {
                    "label": "Stall",
                    "envelope": { "module": "target", "action": "stall" }
                },
                "secondary": { "label": "Later" }
            }
        }))
        .await
        .unwrap();
    let id = reply["id"].as_i64().unwrap();
    let revision = reply["revision"].as_u64().unwrap() as u32;

    // Answer returns after the answering write; Envelope is in flight on Stall.
    h.answer(id, revision, "primary").await.unwrap();
    let answering = h.state()["decisions"][0]["state"].clone();
    assert!(answering.get("answering").is_some(), "{answering}");

    h.reopen().await;

    let d = &h.state()["decisions"][0];
    assert_eq!(d["state"]["failed"]["error"], STOPPED_MID_ANSWER);
    assert_eq!(d["state"]["failed"]["choice"], "primary");
    assert_eq!(h.target_state()["seen"], json!([]));

    h.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn a_vanished_action_fails_the_answer() {
    let h = harness();
    h.idle().await;

    let (id, revision) = raise_poke(&h).await;
    h.store.with_conn(|conn| {
        conn.execute(
            "UPDATE needs_you_decisions
                 SET primary_envelope = ?1
                 WHERE id = ?2",
            rusqlite::params![r#"{"module":"target","action":"gone"}"#, id],
        )
        .unwrap();
    });

    h.answer(id, revision, "primary").await.unwrap();
    h.settle().await;

    let d = &h.state()["decisions"][0];
    assert_eq!(
        d["state"]["failed"]["error"],
        "this action no longer exists"
    );
    assert_eq!(h.target_state()["seen"], json!([]));

    h.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn risk_is_read_from_the_catalog_at_start() {
    let mut h = harness();
    h.idle().await;

    let (id, _) = raise_poke(&h).await;
    h.store.with_conn(|conn| {
        conn.execute(
            "UPDATE needs_you_decisions SET primary_risk = 'read' WHERE id = ?1",
            [id],
        )
        .unwrap();
    });

    h.reopen().await;

    assert_eq!(h.state()["decisions"][0]["primary"]["risk"], "write");

    h.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn attention_is_on_only_while_a_decision_is_open() {
    let h = harness();
    h.idle().await;

    assert!(!h.kernel.tray(None).attention);

    let (id, revision) = raise_poke(&h).await;
    assert!(h.kernel.tray(None).attention);
    assert!(h.kernel.tray(None).summary.contains("1 decisions waiting"));

    h.answer(id, revision, "secondary").await.unwrap();
    h.settle().await;

    assert!(!h.kernel.tray(None).attention);
    assert!(h.state()["decisions"].as_array().unwrap().is_empty());

    h.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn answering_still_runs_the_envelope_if_reload_fails() {
    let h = harness();
    h.idle().await;

    let (id, revision) = raise_poke(&h).await;
    h.store.with_conn(|conn| {
        conn.execute("DROP TABLE needs_you_updates", []).unwrap();
    });

    h.answer(id, revision, "primary").await.unwrap();
    h.settle().await;

    assert_eq!(h.target_state()["seen"], json!(["poke"]));

    h.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn answering_a_destructive_choice_runs_it() {
    let h = harness();
    h.idle().await;

    let reply = h
        .raise(json!({
            "raise": {
                "key": "wipe",
                "title": "Wipe",
                "reason": "needed",
                "primary": {
                    "label": "Wipe",
                    "envelope": { "module": "target", "action": "wipe" }
                },
                "secondary": { "label": "Later" }
            }
        }))
        .await
        .unwrap();
    let id = reply["id"].as_i64().unwrap();
    let revision = reply["revision"].as_u64().unwrap() as u32;

    h.answer(id, revision, "primary").await.unwrap();
    h.settle().await;

    assert_eq!(h.target_state()["seen"], json!(["wipe"]));
    assert!(h.state()["decisions"].as_array().unwrap().is_empty());

    h.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn answering_a_destructive_choice_without_a_grant_fails_before_it_runs() {
    let h = common::harness_ungranted();
    h.idle().await;

    let reply = h
        .raise(json!({
            "raise": {
                "key": "wipe",
                "title": "Wipe",
                "reason": "needed",
                "primary": {
                    "label": "Wipe",
                    "envelope": { "module": "target", "action": "wipe" }
                },
                "secondary": { "label": "Later" }
            }
        }))
        .await
        .unwrap();
    let id = reply["id"].as_i64().unwrap();
    let revision = reply["revision"].as_u64().unwrap() as u32;

    h.answer(id, revision, "primary").await.unwrap();
    h.settle().await;

    let d = &h.state()["decisions"][0];
    let error = d["state"]["failed"]["error"].as_str().unwrap();
    assert!(!error.contains("may already have run"), "{error}");
    assert!(
        error.contains("Refused") || error.contains("refused"),
        "{error}"
    );
    assert_eq!(h.target_state()["seen"], json!([]));

    h.shutdown().await;
}

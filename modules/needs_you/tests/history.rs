//! History of resolved Decisions and Updates, and its retention window.
#![allow(clippy::unwrap_used)]

mod common;

use std::sync::Arc;
use std::time::Duration;

use common::{FakeClock, ORIGIN_MS, Producer, Target, harness};
use navori_config::JournalSweep;
use navori_kernel::{Envelope, Kernel};
use navori_mod_needs_you::NeedsYou;
use navori_store::Store;
use serde_json::{Value, json};

const DAY: Duration = Duration::from_secs(24 * 60 * 60);

struct RetentionHarness {
    kernel: Kernel,
    clock: FakeClock,
    store: Store,
}

fn harness_keep_days(days: u32) -> RetentionHarness {
    let clock = FakeClock::at(ORIGIN_MS);
    let store = Store::open_in_memory().unwrap();
    let kernel = Kernel::builder_with_clock(store.clone(), Arc::new(clock.clone()))
        .pause_journal_sweep()
        .register(NeedsYou::new(JournalSweep::KeepDays(days)))
        .register(Target)
        .register(Producer)
        .grant_destructive("needs_you")
        .start();
    RetentionHarness {
        kernel,
        clock,
        store,
    }
}

impl RetentionHarness {
    async fn idle(&self) {
        for _ in 0..30 {
            tokio::task::yield_now().await;
        }
    }

    async fn settle(&self) {
        for _ in 0..100 {
            tokio::task::yield_now().await;
        }
    }

    async fn advance(&self, by: Duration) {
        self.clock.advance(by);
        tokio::time::advance(by).await;
    }

    async fn raise(&self, action: Value) -> Value {
        self.kernel
            .dispatch(Envelope {
                module: "needs_you".into(),
                action,
            })
            .await
            .unwrap()
    }

    async fn answer(&self, id: i64, revision: u32, choice: &str) {
        self.kernel
            .dispatch(Envelope {
                module: "needs_you".into(),
                action: json!({
                    "answer": {
                        "id": id,
                        "revision": revision,
                        "choice": choice,
                    }
                }),
            })
            .await
            .unwrap();
    }

    async fn history(&self) -> Value {
        self.kernel
            .dispatch(Envelope {
                module: "needs_you".into(),
                action: json!("history"),
            })
            .await
            .unwrap()
    }

    async fn reopen(&mut self, days: u32) {
        self.kernel.shutdown().await;
        self.kernel = Kernel::builder_with_clock(self.store.clone(), Arc::new(self.clock.clone()))
            .pause_journal_sweep()
            .register(NeedsYou::new(JournalSweep::KeepDays(days)))
            .register(Target)
            .register(Producer)
            .grant_destructive("needs_you")
            .start();
        self.idle().await;
    }

    async fn shutdown(self) {
        self.kernel.shutdown().await;
    }

    fn decision_count(&self) -> usize {
        self.store
            .with_conn(|conn| {
                conn.query_row("SELECT COUNT(*) FROM needs_you_decisions", [], |row| {
                    row.get::<_, i64>(0)
                })
            })
            .unwrap() as usize
    }

    fn update_count(&self) -> usize {
        self.store
            .with_conn(|conn| {
                conn.query_row("SELECT COUNT(*) FROM needs_you_updates", [], |row| {
                    row.get::<_, i64>(0)
                })
            })
            .unwrap() as usize
    }

    fn age_resolved(&self, table: &str, id: i64, resolved_at_ms: i64) {
        let sql = format!("UPDATE {table} SET resolved_at_ms = ?1 WHERE id = ?2");
        self.store
            .with_conn(|conn| conn.execute(&sql, rusqlite::params![resolved_at_ms, id]))
            .unwrap();
    }
}

async fn history(h: &common::Harness) -> Value {
    h.kernel
        .dispatch(Envelope {
            module: "needs_you".into(),
            action: json!("history"),
        })
        .await
        .unwrap()
}

#[tokio::test(start_paused = true)]
async fn history_lists_answered_decisions_and_resolved_updates_newest_first() {
    let h = harness();
    h.idle().await;

    let answered = h
        .raise(json!({
            "raise": {
                "key": "a",
                "title": "Ship it",
                "reason": "ready",
                "primary": { "label": "Ship" },
                "secondary": { "label": "Wait" }
            }
        }))
        .await
        .unwrap();
    h.answer(
        answered["id"].as_i64().unwrap(),
        answered["revision"].as_u64().unwrap() as u32,
        "primary",
    )
    .await
    .unwrap();
    h.settle().await;

    h.advance(Duration::from_millis(500)).await;

    h.raise(json!({
        "raise": {
            "key": "b",
            "title": "Late",
            "reason": "window",
            "expires_at_ms": ORIGIN_MS + 1_000,
            "primary": { "label": "Yes" },
            "secondary": { "label": "No" }
        }
    }))
    .await
    .unwrap();
    h.advance(Duration::from_millis(500)).await;
    h.settle().await;

    let update_id = h.state()["updates"][0]["id"].as_i64().unwrap();
    h.kernel
        .dispatch(Envelope {
            module: "needs_you".into(),
            action: json!({ "mark_viewed": { "ids": [update_id] } }),
        })
        .await
        .unwrap();

    let entries = history(&h).await;
    let list = entries.as_array().unwrap();
    assert_eq!(list.len(), 2, "{entries}");

    assert_eq!(list[0]["kind"], "update");
    assert_eq!(list[0]["title"], "Expired: Late");
    assert!(list[0].get("chosen_label").is_none());
    assert_eq!(list[0]["resolved_at_ms"], ORIGIN_MS + 1_000);

    assert_eq!(list[1]["kind"], "decision");
    assert_eq!(list[1]["title"], "Ship it");
    assert_eq!(list[1]["chosen_label"], "Ship");
    assert_eq!(list[1]["resolved_at_ms"], ORIGIN_MS);
    assert!(list[1].get("id").is_none());
    assert!(
        list.iter().all(|row| row["title"] != "Late"),
        "expired Decisions are the Update, not a History Decision: {entries}"
    );

    h.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn history_has_no_answer_for_a_decision() {
    let h = harness();
    h.idle().await;

    let reply = h
        .raise(json!({
            "raise": {
                "key": "k1",
                "title": "Done once",
                "reason": "close",
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

    h.answer(id, revision, "primary").await.unwrap();
    h.settle().await;
    assert_eq!(h.target_state()["seen"], json!(["poke"]));

    let entries = history(&h).await;
    let list = entries.as_array().unwrap();
    assert_eq!(list.len(), 1, "{entries}");
    assert!(
        list[0].get("id").is_none(),
        "History must carry no id to answer: {entries}"
    );
    assert!(list[0].get("answer_request").is_none(), "{entries}");

    let refused = h.answer(id, revision, "primary").await;
    assert!(
        refused.is_err(),
        "answering a History Decision must not run"
    );
    assert_eq!(h.target_state()["seen"], json!(["poke"]));

    h.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn items_older_than_retention_are_pruned_at_start_and_daily() {
    let mut h = harness_keep_days(2);
    h.idle().await;

    let reply = h
        .raise(json!({
            "raise": {
                "key": "old",
                "title": "Old ship",
                "reason": "ago",
                "primary": { "label": "Ship" },
                "secondary": { "label": "Wait" }
            }
        }))
        .await;
    let old_id = reply["id"].as_i64().unwrap();
    h.answer(
        old_id,
        reply["revision"].as_u64().unwrap() as u32,
        "primary",
    )
    .await;
    h.settle().await;

    h.raise(json!({
        "raise": {
            "key": "exp",
            "title": "Old update",
            "reason": "gone",
            "expires_at_ms": ORIGIN_MS + 100,
            "primary": { "label": "Yes" },
            "secondary": { "label": "No" }
        }
    }))
    .await;
    h.advance(Duration::from_millis(100)).await;
    h.settle().await;
    let update_id = h.kernel.state("needs_you").unwrap()["updates"][0]["id"]
        .as_i64()
        .unwrap();
    h.kernel
        .dispatch(Envelope {
            module: "needs_you".into(),
            action: json!({ "mark_viewed": { "ids": [update_id] } }),
        })
        .await
        .unwrap();

    assert_eq!(h.decision_count(), 2);
    assert_eq!(h.update_count(), 1);

    h.advance(DAY * 3).await;
    h.reopen(2).await;

    assert_eq!(h.decision_count(), 0, "start prune must drop aged History");
    assert_eq!(h.update_count(), 0);
    assert!(h.history().await.as_array().unwrap().is_empty());

    let fresh = h
        .raise(json!({
            "raise": {
                "key": "fresh",
                "title": "Keep me",
                "reason": "new",
                "primary": { "label": "Ok" },
                "secondary": { "label": "No" }
            }
        }))
        .await;
    let fresh_id = fresh["id"].as_i64().unwrap();
    h.answer(
        fresh_id,
        fresh["revision"].as_u64().unwrap() as u32,
        "secondary",
    )
    .await;
    h.settle().await;

    let doomed = h
        .raise(json!({
            "raise": {
                "key": "doomed",
                "title": "Drop tomorrow",
                "reason": "age",
                "primary": { "label": "Ok" },
                "secondary": { "label": "No" }
            }
        }))
        .await;
    let doomed_id = doomed["id"].as_i64().unwrap();
    h.answer(
        doomed_id,
        doomed["revision"].as_u64().unwrap() as u32,
        "primary",
    )
    .await;
    h.settle().await;

    let now_ms = h.clock.millis() as i64;
    h.age_resolved(
        "needs_you_decisions",
        doomed_id,
        now_ms - 3 * DAY.as_millis() as i64,
    );

    assert_eq!(h.decision_count(), 2);
    h.advance(DAY).await;
    h.settle().await;

    assert_eq!(h.decision_count(), 1, "daily prune must drop aged rows");
    let left: String = h
        .store
        .with_conn(|conn| {
            conn.query_row("SELECT title FROM needs_you_decisions", [], |row| {
                row.get(0)
            })
        })
        .unwrap();
    assert_eq!(left, "Keep me");

    h.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn a_paused_sweep_prunes_nothing() {
    let h = harness();
    h.idle().await;

    let reply = h
        .raise(json!({
            "raise": {
                "key": "keep",
                "title": "Stay",
                "reason": "paused",
                "primary": { "label": "Ok" },
                "secondary": { "label": "No" }
            }
        }))
        .await
        .unwrap();
    let id = reply["id"].as_i64().unwrap();
    h.answer(id, reply["revision"].as_u64().unwrap() as u32, "primary")
        .await
        .unwrap();
    h.settle().await;

    h.store
        .with_conn(|conn| {
            conn.execute(
                "UPDATE needs_you_decisions SET resolved_at_ms = ?1 WHERE id = ?2",
                rusqlite::params![ORIGIN_MS as i64 - 90 * DAY.as_millis() as i64, id],
            )
        })
        .unwrap();

    let mut h = h;
    h.reopen().await;
    h.advance(DAY).await;
    h.settle().await;

    let count: i64 = h
        .store
        .with_conn(|conn| {
            conn.query_row("SELECT COUNT(*) FROM needs_you_decisions", [], |row| {
                row.get(0)
            })
        })
        .unwrap();
    assert_eq!(count, 1, "Paused must delete no History");

    let entries = history(&h).await;
    assert_eq!(entries.as_array().unwrap().len(), 1, "{entries}");

    h.shutdown().await;
}

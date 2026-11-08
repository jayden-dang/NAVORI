//! Tick, undo, History, and retention prune for To-dos.
#![allow(clippy::unwrap_used)]

mod common;

use std::sync::Arc;
use std::time::Duration;

use common::{FakeClock, FixedCalendar, Harness, OFFSET_SECS, ORIGIN_MS, harness, today};
use navori_config::JournalSweep;
use navori_kernel::{Envelope, Kernel, ModuleAction};
use navori_mod_todo::{LocalCalendar, Todo, TodoAction, TodoEnv};
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
    let calendar: Arc<dyn LocalCalendar> = Arc::new(FixedCalendar::new(OFFSET_SECS));
    let kernel = Kernel::builder_with_clock(store.clone(), Arc::new(clock.clone()))
        .pause_journal_sweep()
        .register(Todo::new(
            TodoEnv { calendar },
            JournalSweep::KeepDays(days),
        ))
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

    async fn add(&self, title: &str, due_on: Option<&str>) -> Value {
        let mut body = json!({ "title": title });
        if let Some(due_on) = due_on {
            body["due_on"] = json!(due_on);
        }
        self.kernel
            .dispatch(Envelope {
                module: "todo".into(),
                action: json!({ "add": body }),
            })
            .await
            .unwrap()
    }

    async fn tick(&self, id: i64) {
        self.kernel
            .dispatch(Envelope {
                module: "todo".into(),
                action: json!({ "tick": { "id": id } }),
            })
            .await
            .unwrap();
    }

    async fn history(&self) -> Value {
        self.kernel
            .dispatch(Envelope {
                module: "todo".into(),
                action: json!("history"),
            })
            .await
            .unwrap()
    }

    async fn reopen(&mut self, days: u32) {
        self.kernel.shutdown().await;
        let calendar: Arc<dyn LocalCalendar> = Arc::new(FixedCalendar::new(OFFSET_SECS));
        self.kernel = Kernel::builder_with_clock(self.store.clone(), Arc::new(self.clock.clone()))
            .pause_journal_sweep()
            .register(Todo::new(
                TodoEnv { calendar },
                JournalSweep::KeepDays(days),
            ))
            .start();
        self.idle().await;
    }

    async fn shutdown(self) {
        self.kernel.shutdown().await;
    }

    fn done_count(&self) -> usize {
        self.store
            .with_conn(|conn| {
                conn.query_row(
                    "SELECT COUNT(*) FROM todo_items WHERE done_at_ms IS NOT NULL",
                    [],
                    |row| row.get::<_, i64>(0),
                )
            })
            .unwrap() as usize
    }

    fn open_count(&self) -> usize {
        self.store
            .with_conn(|conn| {
                conn.query_row(
                    "SELECT COUNT(*) FROM todo_items WHERE done_at_ms IS NULL",
                    [],
                    |row| row.get::<_, i64>(0),
                )
            })
            .unwrap() as usize
    }

    fn age_done(&self, id: i64, done_at_ms: i64) {
        self.store
            .with_conn(|conn| {
                conn.execute(
                    "UPDATE todo_items SET done_at_ms = ?1 WHERE id = ?2",
                    rusqlite::params![done_at_ms, id],
                )
            })
            .unwrap();
    }
}

impl Harness {
    async fn tick(&self, id: i64) -> Result<Value, navori_kernel::DispatchError> {
        self.kernel
            .dispatch(Envelope {
                module: "todo".into(),
                action: json!({ "tick": { "id": id } }),
            })
            .await
    }

    async fn undo(&self, id: i64) -> Result<Value, navori_kernel::DispatchError> {
        self.kernel
            .dispatch(Envelope {
                module: "todo".into(),
                action: json!({ "undo": { "id": id } }),
            })
            .await
    }

    async fn history(&self) -> Result<Value, navori_kernel::DispatchError> {
        self.kernel
            .dispatch(Envelope {
                module: "todo".into(),
                action: json!("history"),
            })
            .await
    }
}

#[tokio::test(start_paused = true)]
async fn ticking_moves_a_todo_to_history() {
    let h = harness();
    h.idle().await;

    let reply = h.add("Buy milk", Some(today())).await.unwrap();
    let id = reply["id"].as_i64().unwrap();

    h.tick(id).await.unwrap();

    let state = h.state();
    let open = state["open"].as_array().unwrap();
    assert!(
        open.is_empty(),
        "ticked To-do must leave open at once: {open:?}"
    );

    let entries = h.history().await.unwrap();
    let list = entries.as_array().unwrap();
    assert_eq!(list.len(), 1, "{entries}");
    assert_eq!(list[0]["id"], id);
    assert_eq!(list[0]["title"], "Buy milk");
    assert_eq!(list[0]["due_on"], today());
    assert_eq!(list[0]["done_at_ms"], ORIGIN_MS);

    h.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn undo_returns_it_with_title_and_date() {
    let h = harness();
    h.idle().await;

    let reply = h.add("Keep me", Some(today())).await.unwrap();
    let id = reply["id"].as_i64().unwrap();
    h.tick(id).await.unwrap();
    h.undo(id).await.unwrap();

    let state = h.state();
    let open = state["open"].as_array().unwrap();
    assert_eq!(open.len(), 1, "{open:?}");
    assert_eq!(open[0]["id"], id);
    assert_eq!(open[0]["title"], "Keep me");
    assert_eq!(open[0]["due_on"], today());
    assert_eq!(open[0]["status"], "today");

    let entries = h.history().await.unwrap();
    assert!(
        entries.as_array().unwrap().is_empty(),
        "undone To-do must leave History: {entries}"
    );

    h.shutdown().await;
}

#[test]
fn tick_and_undo_are_write_actions() {
    let _tick = TodoAction::Tick { id: 1 };
    let _undo = TodoAction::Undo { id: 1 };
    let docs = TodoAction::catalog();
    let tick = docs.iter().find(|d| d.name == "tick").expect("tick");
    let undo = docs.iter().find(|d| d.name == "undo").expect("undo");
    let history = docs.iter().find(|d| d.name == "history").expect("history");
    assert_eq!(tick.risk, navori_kernel::Risk::Write);
    assert_eq!(undo.risk, navori_kernel::Risk::Write);
    assert_eq!(history.risk, navori_kernel::Risk::Read);
}

#[tokio::test(start_paused = true)]
async fn old_done_todos_are_pruned() {
    let mut h = harness_keep_days(2);
    h.idle().await;

    let reply = h.add("Old one", Some(today())).await;
    let old_id = reply["id"].as_i64().unwrap();
    h.tick(old_id).await;
    h.age_done(old_id, ORIGIN_MS as i64 - 3 * DAY.as_millis() as i64);

    let open_id = h.add("Still open", None).await["id"].as_i64().unwrap();
    assert_eq!(h.done_count(), 1);
    assert_eq!(h.open_count(), 1);

    h.advance(DAY * 3).await;
    h.reopen(2).await;

    assert_eq!(h.done_count(), 0, "start prune must drop aged done To-dos");
    assert_eq!(h.open_count(), 1, "open rows must stay");
    assert!(h.history().await.as_array().unwrap().is_empty());
    let left_open: String = h
        .store
        .with_conn(|conn| {
            conn.query_row(
                "SELECT title FROM todo_items WHERE id = ?1",
                rusqlite::params![open_id],
                |row| row.get(0),
            )
        })
        .unwrap();
    assert_eq!(left_open, "Still open");

    let fresh = h.add("Keep me", Some(today())).await;
    let fresh_id = fresh["id"].as_i64().unwrap();
    h.tick(fresh_id).await;

    let doomed = h.add("Drop tomorrow", Some(today())).await;
    let doomed_id = doomed["id"].as_i64().unwrap();
    h.tick(doomed_id).await;

    let now_ms = h.clock.millis() as i64;
    h.age_done(doomed_id, now_ms - 3 * DAY.as_millis() as i64);

    assert_eq!(h.done_count(), 2);
    h.advance(DAY).await;
    h.settle().await;

    assert_eq!(h.done_count(), 1, "daily prune must drop aged done To-dos");
    let left: String = h
        .store
        .with_conn(|conn| {
            conn.query_row(
                "SELECT title FROM todo_items WHERE done_at_ms IS NOT NULL",
                [],
                |row| row.get(0),
            )
        })
        .unwrap();
    assert_eq!(left, "Keep me");

    h.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn a_paused_sweep_prunes_nothing() {
    let h = harness();
    h.idle().await;

    let reply = h.add("Stay", Some(today())).await.unwrap();
    let id = reply["id"].as_i64().unwrap();
    h.tick(id).await.unwrap();

    h.store
        .with_conn(|conn| {
            conn.execute(
                "UPDATE todo_items SET done_at_ms = ?1 WHERE id = ?2",
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
            conn.query_row(
                "SELECT COUNT(*) FROM todo_items WHERE done_at_ms IS NOT NULL",
                [],
                |row| row.get(0),
            )
        })
        .unwrap();
    assert_eq!(count, 1, "Paused must delete no History");

    let entries = h.history().await.unwrap();
    assert_eq!(entries.as_array().unwrap().len(), 1, "{entries}");
    assert_eq!(entries[0]["id"], id);
    assert_eq!(entries[0]["title"], "Stay");

    h.shutdown().await;
}

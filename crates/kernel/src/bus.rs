//! Event bus: redact → journal → broadcast.

use std::time::{SystemTime, UNIX_EPOCH};

use navori_protocol::{Event, Severity};
use navori_store::Store;
use serde_json::Value;
use tokio::sync::broadcast;

const CAPACITY: usize = 256;

#[derive(Debug, Clone)]
pub struct EventBus {
    tx: broadcast::Sender<Event>,
    store: Store,
}

impl EventBus {
    pub(crate) fn new(store: Store) -> Self {
        let (tx, _) = broadcast::channel(CAPACITY);
        Self { tx, store }
    }

    /// Live events. Slow subscribers lose the oldest events (`RecvError::Lagged`); the
    /// journal is the durable record.
    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.tx.subscribe()
    }

    /// Redacts, persists and broadcasts `event`.
    ///
    /// The insert is usually fast, but while another process holds the database's write lock
    /// it waits in SQLite's busy timeout. On a multi-thread runtime worker that wait would
    /// strand the tasks queued on that worker, `core.status` among them (issue #21), so the
    /// worker hands its queue to another thread first.
    pub fn publish(&self, mut event: Event) {
        event.summary = navori_redact::redact_text(&event.summary).text;
        navori_redact::redact_json(&mut event.payload);
        if let Err(e) = blocking(|| self.store.journal_append(&event)) {
            tracing::warn!(error = %e, kind = %event.kind, "failed to journal event");
        }
        let _ = self.tx.send(event);
    }

    pub fn emit(
        &self,
        module: &str,
        kind: &str,
        severity: Severity,
        summary: String,
        payload: Value,
    ) {
        self.publish(Event {
            id: ulid::Ulid::generate().to_string(),
            ts_ms: now_ms(),
            module: module.to_owned(),
            kind: kind.to_owned(),
            severity,
            summary,
            payload,
        });
    }
}

/// Runs `f`, which may block, without stranding a multi-thread runtime worker's queue.
/// `block_in_place` panics on a current-thread runtime, so anywhere else `f` runs inline.
fn blocking<T>(f: impl FnOnce() -> T) -> T {
    let multi_thread = tokio::runtime::Handle::try_current()
        .is_ok_and(|h| h.runtime_flavor() == tokio::runtime::RuntimeFlavor::MultiThread);
    if multi_thread {
        tokio::task::block_in_place(f)
    } else {
        f()
    }
}

pub(crate) fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

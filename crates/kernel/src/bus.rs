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

    /// Redacts, persists (a fast SQLite insert) and broadcasts `event`.
    pub fn publish(&self, mut event: Event) {
        event.summary = navori_redact::redact_text(&event.summary).text;
        navori_redact::redact_json(&mut event.payload);
        if let Err(e) = self.store.journal_append(&event) {
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

pub(crate) fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

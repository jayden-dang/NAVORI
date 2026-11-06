use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Mutex;

use navori_redact::redact_text;
use navori_store::{Migration, Store};

use crate::{AuditError, AuditRow, AuditSink, Outcome};

const AUDIT_OWNER: &str = "context";

const AUDIT_MIGRATIONS: &[Migration] = &[Migration {
    version: 1,
    name: "context_audit",
    sql: "
        CREATE TABLE context_audit (
          id              INTEGER PRIMARY KEY AUTOINCREMENT,
          ts_ms           INTEGER NOT NULL,
          provider        TEXT NOT NULL,
          kinds           TEXT NOT NULL,
          redacted_count  INTEGER NOT NULL,
          outcome         TEXT NOT NULL CHECK (outcome IN ('authorized','blocked'))
        );
    ",
}];

#[derive(Debug)]
pub struct StoreAudit {
    store: Store,
    migrated: Mutex<bool>,
}

impl StoreAudit {
    pub fn new(store: Store) -> Self {
        Self {
            store,
            migrated: Mutex::new(false),
        }
    }

    fn ensure_table(&self) -> Result<(), AuditError> {
        let mut migrated = self
            .migrated
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if !*migrated {
            self.store
                .migrate(AUDIT_OWNER, AUDIT_MIGRATIONS)
                .map_err(|_| AuditError::Migration)?;
            *migrated = true;
        }
        Ok(())
    }
}

impl AuditSink for StoreAudit {
    fn record(&self, row: AuditRow) -> Result<(), AuditError> {
        self.ensure_table()?;
        let (provider, kinds) = catch_unwind(AssertUnwindSafe(|| {
            (
                redact_text(&row.provider).text,
                redact_text(&row.kinds.join(",")).text,
            )
        }))
        .map_err(|_| AuditError::ShieldPass)?;
        let outcome = match row.outcome {
            Outcome::Authorized => "authorized",
            Outcome::Blocked => "blocked",
        };
        self.store
            .with_conn(|conn| {
                conn.execute(
                    "INSERT INTO context_audit (ts_ms, provider, kinds, redacted_count, outcome) VALUES (?1, ?2, ?3, ?4, ?5)",
                    (row.ts_ms, provider, kinds, row.redacted_count, outcome),
                )
            })
            .map(|_| ())
            .map_err(|_| AuditError::Write)
    }
}

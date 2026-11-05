//! SQLite storage shared by the kernel and every module.
//!
//! - One database file (WAL). Each *owner* (`core`, `github`, …) versions its own tables via
//!   [`Store::migrate`]; every migration runs in a transaction.
//! - Before upgrading an owner that already has data, the whole database is copied to
//!   `backups/` (`VACUUM INTO`); the five newest backups are kept.
//! - The event journal is redacted on write: secrets never reach the disk.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use navori_protocol::{Event, Severity};
use rusqlite::{Connection, OptionalExtension, params};
use serde_json::Value;

#[derive(Debug, Clone, Copy)]
pub struct Migration {
    pub version: u32,
    pub name: &'static str,
    pub sql: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MigrationReport {
    pub applied: Vec<u32>,
    pub backup: Option<PathBuf>,
}

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("migration {owner} v{version} ({name}) failed: {source}")]
    Migration {
        owner: String,
        version: u32,
        name: String,
        source: rusqlite::Error,
    },
    #[error("corrupt row: {0}")]
    Corrupt(String),
}

pub type Result<T> = std::result::Result<T, StoreError>;

const KEEP_BACKUPS: usize = 5;

const CORE_MIGRATIONS: &[Migration] = &[Migration {
    version: 1,
    name: "kv_and_events",
    sql: "
        CREATE TABLE kv (
            ns    TEXT NOT NULL,
            key   TEXT NOT NULL,
            value TEXT NOT NULL,
            PRIMARY KEY (ns, key)
        );
        CREATE TABLE events (
            id       TEXT PRIMARY KEY,
            ts_ms    INTEGER NOT NULL,
            module   TEXT NOT NULL,
            kind     TEXT NOT NULL,
            severity TEXT NOT NULL,
            summary  TEXT NOT NULL,
            payload  TEXT NOT NULL
        );
        CREATE INDEX events_ts ON events (ts_ms);
    ",
}];

/// Cheap to clone; all clones share one connection. Calls are blocking — async callers
/// should use `spawn_blocking`.
#[derive(Debug, Clone)]
pub struct Store {
    conn: Arc<Mutex<Connection>>,
    backups_dir: Option<PathBuf>,
}

impl Store {
    /// Opens (creating if needed) the database at `path` and applies core migrations.
    pub fn open(path: &Path, backups_dir: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        Self::init(conn, Some(backups_dir.to_owned()))
    }

    pub fn open_in_memory() -> Result<Self> {
        Self::init(Connection::open_in_memory()?, None)
    }

    fn init(conn: Connection, backups_dir: Option<PathBuf>) -> Result<Self> {
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS schema_migrations (
                owner         TEXT NOT NULL,
                version       INTEGER NOT NULL,
                name          TEXT NOT NULL,
                applied_at_ms INTEGER NOT NULL,
                PRIMARY KEY (owner, version)
            );",
        )?;
        let store = Self {
            conn: Arc::new(Mutex::new(conn)),
            backups_dir,
        };
        store.migrate("core", CORE_MIGRATIONS)?;
        Ok(store)
    }

    /// Runs `f` with exclusive access to the connection. Modules use this for their own tables.
    pub fn with_conn<T>(&self, f: impl FnOnce(&Connection) -> T) -> T {
        let guard = self
            .conn
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        f(&guard)
    }

    /// Applies every migration of `owner` newer than its current version, in version order.
    pub fn migrate(&self, owner: &str, migrations: &[Migration]) -> Result<MigrationReport> {
        let mut pending: Vec<Migration> = Vec::new();
        let current = self.schema_version(owner)?;
        for m in migrations {
            if m.version > current {
                pending.push(*m);
            }
        }
        pending.sort_by_key(|m| m.version);
        if pending.is_empty() {
            return Ok(MigrationReport::default());
        }

        let backup = if current > 0 {
            self.backup(owner, current)?
        } else {
            None
        };

        let mut applied = Vec::new();
        self.with_conn(|conn| -> Result<()> {
            for m in &pending {
                let fail = |source| StoreError::Migration {
                    owner: owner.to_owned(),
                    version: m.version,
                    name: m.name.to_owned(),
                    source,
                };
                let tx = conn.unchecked_transaction().map_err(fail)?;
                tx.execute_batch(m.sql).map_err(fail)?;
                tx.execute(
                    "INSERT INTO schema_migrations (owner, version, name, applied_at_ms) VALUES (?1, ?2, ?3, ?4)",
                    params![owner, m.version, m.name, now_ms()],
                )
                .map_err(fail)?;
                tx.commit().map_err(fail)?;
                applied.push(m.version);
            }
            Ok(())
        })?;
        Ok(MigrationReport { applied, backup })
    }

    pub fn schema_version(&self, owner: &str) -> Result<u32> {
        let v: Option<u32> = self.with_conn(|c| {
            c.query_row(
                "SELECT MAX(version) FROM schema_migrations WHERE owner = ?1",
                [owner],
                |r| r.get(0),
            )
        })?;
        Ok(v.unwrap_or(0))
    }

    pub fn integrity_ok(&self) -> Result<bool> {
        let res: String =
            self.with_conn(|c| c.query_row("PRAGMA quick_check", [], |r| r.get(0)))?;
        Ok(res == "ok")
    }

    fn backup(&self, owner: &str, from_version: u32) -> Result<Option<PathBuf>> {
        let Some(dir) = &self.backups_dir else {
            return Ok(None);
        };
        std::fs::create_dir_all(dir)?;
        let file = dir.join(format!("{:013}-{owner}-v{from_version:04}.db", now_ms()));
        let target = file.to_string_lossy().into_owned();
        self.with_conn(|c| c.execute("VACUUM INTO ?1", [target]))?;
        prune_backups(dir)?;
        Ok(Some(file))
    }

    pub fn kv_set(&self, ns: &str, key: &str, value: &Value) -> Result<()> {
        let json = value.to_string();
        self.with_conn(|c| {
            c.execute(
                "INSERT INTO kv (ns, key, value) VALUES (?1, ?2, ?3)
                 ON CONFLICT (ns, key) DO UPDATE SET value = excluded.value",
                params![ns, key, json],
            )
        })?;
        Ok(())
    }

    pub fn kv_get(&self, ns: &str, key: &str) -> Result<Option<Value>> {
        let raw: Option<String> = self.with_conn(|c| {
            c.query_row(
                "SELECT value FROM kv WHERE ns = ?1 AND key = ?2",
                [ns, key],
                |r| r.get(0),
            )
            .optional()
        })?;
        raw.map(|s| {
            serde_json::from_str(&s).map_err(|e| StoreError::Corrupt(format!("kv {ns}/{key}: {e}")))
        })
        .transpose()
    }

    /// Persists `event` after redacting its summary and payload.
    pub fn journal_append(&self, event: &Event) -> Result<()> {
        let summary = navori_redact::redact_text(&event.summary).text;
        let mut payload = event.payload.clone();
        navori_redact::redact_json(&mut payload);
        let severity = severity_str(event.severity);
        self.with_conn(|c| {
            c.execute(
                "INSERT OR REPLACE INTO events (id, ts_ms, module, kind, severity, summary, payload)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![event.id, event.ts_ms, event.module, event.kind, severity, summary, payload.to_string()],
            )
        })?;
        Ok(())
    }

    /// Newest first; events sharing a millisecond come back in the order they were written.
    pub fn journal_recent(&self, limit: usize) -> Result<Vec<Event>> {
        let limit = i64::try_from(limit).unwrap_or(i64::MAX);
        let rows: Vec<(String, i64, String, String, String, String, String)> =
            self.with_conn(|c| {
                let mut stmt = c.prepare(
                    "SELECT id, ts_ms, module, kind, severity, summary, payload
                 FROM events ORDER BY ts_ms DESC, rowid DESC LIMIT ?1",
                )?;
                let rows = stmt.query_map([limit], |r| {
                    Ok((
                        r.get(0)?,
                        r.get(1)?,
                        r.get(2)?,
                        r.get(3)?,
                        r.get(4)?,
                        r.get(5)?,
                        r.get(6)?,
                    ))
                })?;
                rows.collect::<rusqlite::Result<Vec<_>>>()
            })?;
        rows.into_iter()
            .map(|(id, ts_ms, module, kind, severity, summary, payload)| {
                Ok(Event {
                    severity: parse_severity(&severity).ok_or_else(|| {
                        StoreError::Corrupt(format!("event {id}: severity {severity}"))
                    })?,
                    payload: serde_json::from_str(&payload)
                        .map_err(|e| StoreError::Corrupt(format!("event {id}: {e}")))?,
                    id,
                    ts_ms,
                    module,
                    kind,
                    summary,
                })
            })
            .collect()
    }

    /// Deletes events with `ts_ms < older_than_ms`. Returns how many were removed.
    pub fn journal_prune(&self, older_than_ms: i64) -> Result<usize> {
        Ok(self.with_conn(|c| c.execute("DELETE FROM events WHERE ts_ms < ?1", [older_than_ms]))?)
    }
}

fn prune_backups(dir: &Path) -> Result<()> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|e| e == "db"))
        .collect();
    files.sort();
    let excess = files.len().saturating_sub(KEEP_BACKUPS);
    for old in files.into_iter().take(excess) {
        std::fs::remove_file(old)?;
    }
    Ok(())
}

fn severity_str(s: Severity) -> &'static str {
    match s {
        Severity::Debug => "debug",
        Severity::Info => "info",
        Severity::Warn => "warn",
        Severity::Error => "error",
    }
}

fn parse_severity(s: &str) -> Option<Severity> {
    Some(match s {
        "debug" => Severity::Debug,
        "info" => Severity::Info,
        "warn" => Severity::Warn,
        "error" => Severity::Error,
        _ => return None,
    })
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

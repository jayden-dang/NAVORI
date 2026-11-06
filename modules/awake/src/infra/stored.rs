use std::time::{Duration, SystemTime, UNIX_EPOCH};

use navori_procs::Harness;
use navori_store::{Migration, Store};
use rusqlite::params;

use crate::domain::harness_name;

const OWNER: &str = "awake";

const MIGRATIONS: &[Migration] = &[Migration {
    version: 1,
    name: "awake_leases",
    sql: "
        CREATE TABLE awake_leases (
          key               TEXT PRIMARY KEY,
          kind              TEXT NOT NULL CHECK (kind IN ('session','command')),
          session_id        TEXT,
          harness           TEXT,
          pid               INTEGER NOT NULL,
          process_start_us  INTEGER NOT NULL,
          first_hold_us     INTEGER,
          stop_mark_us      INTEGER,
          deadline_us       INTEGER
        );
    ",
}];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StoredKind {
    Session,
    Command,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StoredLease {
    pub key: String,
    pub kind: StoredKind,
    pub session_id: Option<String>,
    pub harness: Option<Harness>,
    pub pid: i32,
    pub process_start_us: u64,
    pub first_hold: Option<SystemTime>,
    pub stop_mark: Option<SystemTime>,
    pub deadline: Option<SystemTime>,
}

pub(crate) fn migrate(store: &Store) -> anyhow::Result<()> {
    store
        .migrate(OWNER, MIGRATIONS)
        .map_err(|err| anyhow::anyhow!("awake lease migration failed: {err}"))?;
    Ok(())
}

pub(crate) fn load(store: &Store) -> Result<Vec<StoredLease>, String> {
    store.with_conn(load_conn).map_err(|err| err.to_string())
}

pub(crate) fn upsert(store: &Store, row: &StoredLease) -> Result<(), String> {
    let kind = match row.kind {
        StoredKind::Session => "session",
        StoredKind::Command => "command",
    };
    let start = i64::try_from(row.process_start_us).map_err(|err| err.to_string())?;
    let first_hold = optional_us(row.first_hold)?;
    let stop_mark = optional_us(row.stop_mark)?;
    let deadline = optional_us(row.deadline)?;
    let harness = row.harness.map(harness_name);
    store
        .with_conn(|conn| {
            conn.execute(
                "INSERT INTO awake_leases (
                    key, kind, session_id, harness, pid, process_start_us,
                    first_hold_us, stop_mark_us, deadline_us
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
                 ON CONFLICT(key) DO UPDATE SET
                    kind = excluded.kind,
                    session_id = excluded.session_id,
                    harness = excluded.harness,
                    pid = excluded.pid,
                    process_start_us = excluded.process_start_us,
                    first_hold_us = excluded.first_hold_us,
                    stop_mark_us = excluded.stop_mark_us,
                    deadline_us = excluded.deadline_us",
                params![
                    row.key,
                    kind,
                    row.session_id,
                    harness,
                    row.pid,
                    start,
                    first_hold,
                    stop_mark,
                    deadline,
                ],
            )
        })
        .map(|_| ())
        .map_err(|err| err.to_string())
}

pub(crate) fn delete(store: &Store, key: &str) -> Result<(), String> {
    store
        .with_conn(|conn| conn.execute("DELETE FROM awake_leases WHERE key = ?1", [key]))
        .map(|_| ())
        .map_err(|err| err.to_string())
}

pub(crate) fn clear(store: &Store) -> Result<(), String> {
    store
        .with_conn(|conn| conn.execute("DELETE FROM awake_leases", []))
        .map(|_| ())
        .map_err(|err| err.to_string())
}

fn load_conn(conn: &rusqlite::Connection) -> rusqlite::Result<Vec<StoredLease>> {
    let mut stmt = conn.prepare(
        "SELECT key, kind, session_id, harness, pid, process_start_us,
                first_hold_us, stop_mark_us, deadline_us
         FROM awake_leases ORDER BY key",
    )?;
    let rows = stmt.query_map([], to_lease)?;
    rows.collect()
}

fn to_lease(row: &rusqlite::Row<'_>) -> rusqlite::Result<StoredLease> {
    let kind: String = row.get(1)?;
    let kind = match kind.as_str() {
        "session" => StoredKind::Session,
        "command" => StoredKind::Command,
        _ => {
            return Err(rusqlite::Error::InvalidColumnType(
                1,
                "kind".to_owned(),
                rusqlite::types::Type::Text,
            ));
        }
    };
    let start: i64 = row.get(5)?;
    let process_start_us = u64::try_from(start).map_err(|err| {
        rusqlite::Error::FromSqlConversionFailure(5, rusqlite::types::Type::Integer, Box::new(err))
    })?;
    Ok(StoredLease {
        key: row.get(0)?,
        kind,
        session_id: row.get(2)?,
        harness: row.get::<_, Option<String>>(3)?.and_then(parse_harness),
        pid: row.get(4)?,
        process_start_us,
        first_hold: optional_time(row.get(6)?)?,
        stop_mark: optional_time(row.get(7)?)?,
        deadline: optional_time(row.get(8)?)?,
    })
}

fn parse_harness(text: String) -> Option<Harness> {
    match text.as_str() {
        "Claude Code" => Some(Harness::ClaudeCode),
        "Codex" => Some(Harness::Codex),
        "Grok" => Some(Harness::Grok),
        _ => None,
    }
}

fn optional_time(us: Option<i64>) -> rusqlite::Result<Option<SystemTime>> {
    us.map(time_from_us).transpose()
}

fn time_from_us(us: i64) -> rusqlite::Result<SystemTime> {
    let us = u64::try_from(us).map_err(|err| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Integer, Box::new(err))
    })?;
    UNIX_EPOCH
        .checked_add(Duration::from_micros(us))
        .ok_or_else(|| {
            rusqlite::Error::FromSqlConversionFailure(
                0,
                rusqlite::types::Type::Integer,
                Box::new(std::io::Error::other("lease time overflowed the clock")),
            )
        })
}

fn optional_us(time: Option<SystemTime>) -> Result<Option<i64>, String> {
    time.map(required_us).transpose()
}

fn required_us(time: SystemTime) -> Result<i64, String> {
    let duration = time
        .duration_since(UNIX_EPOCH)
        .map_err(|err| err.to_string())?;
    i64::try_from(duration.as_micros()).map_err(|err| err.to_string())
}

const SETTINGS_NS: &str = "awake";
const TRACKING_KEY: &str = "tracking";

/// `None` when nothing is stored or the stored value is not a bool.
pub(crate) fn load_tracking(store: &Store) -> navori_store::Result<Option<bool>> {
    Ok(store
        .kv_get(SETTINGS_NS, TRACKING_KEY)?
        .and_then(|value| value.as_bool()))
}

pub(crate) fn save_tracking(store: &Store, on: bool) -> navori_store::Result<()> {
    store.kv_set(SETTINGS_NS, TRACKING_KEY, &serde_json::Value::Bool(on))
}

const LID_KEY: &str = "lid";
const LID_HELD_KEY: &str = "lid_held";
const LAST_SLEEP_KEY: &str = "last_sleep";

/// `None` when nothing is stored or the stored value is not a bool.
pub(crate) fn load_lid(store: &Store) -> navori_store::Result<Option<bool>> {
    Ok(store
        .kv_get(SETTINGS_NS, LID_KEY)?
        .and_then(|value| value.as_bool()))
}

pub(crate) fn save_lid(store: &Store, on: bool) -> navori_store::Result<()> {
    store.kv_set(SETTINGS_NS, LID_KEY, &serde_json::Value::Bool(on))
}

#[expect(dead_code, reason = "used by the lid restore / last sleep tasks")]
pub(crate) fn load_lid_held(store: &Store) -> navori_store::Result<bool> {
    Ok(store.kv_get(SETTINGS_NS, LID_HELD_KEY)?.is_some())
}

pub(crate) fn save_lid_held(store: &Store) -> navori_store::Result<()> {
    store.kv_set(SETTINGS_NS, LID_HELD_KEY, &serde_json::Value::Bool(true))
}

pub(crate) fn delete_lid_held(store: &Store) -> navori_store::Result<()> {
    store.kv_delete(SETTINGS_NS, LID_HELD_KEY)
}

#[expect(dead_code, reason = "used by the lid restore / last sleep tasks")]
pub(crate) fn load_last_sleep(store: &Store) -> navori_store::Result<Option<i64>> {
    Ok(store
        .kv_get(SETTINGS_NS, LAST_SLEEP_KEY)?
        .and_then(|value| value.as_i64()))
}

#[expect(dead_code, reason = "used by the lid restore / last sleep tasks")]
pub(crate) fn save_last_sleep(store: &Store, at_ms: i64) -> navori_store::Result<()> {
    store.kv_set(SETTINGS_NS, LAST_SLEEP_KEY, &serde_json::json!(at_ms))
}

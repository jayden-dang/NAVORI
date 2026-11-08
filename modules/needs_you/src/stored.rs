use navori_store::{Migration, Store};
use rusqlite::{OptionalExtension, params};

use crate::state::{ChoiceState, DecisionState, OpenDecision};

const OWNER: &str = "needs_you";

const MIGRATIONS: &[Migration] = &[Migration {
    version: 1,
    name: "needs_you_decisions",
    sql: "
        CREATE TABLE needs_you_decisions (
          id INTEGER PRIMARY KEY,
          key TEXT NOT NULL,
          revision INTEGER NOT NULL,
          state TEXT NOT NULL,
          title TEXT NOT NULL,
          reason TEXT NOT NULL,
          raised_at_ms INTEGER NOT NULL,
          expires_at_ms INTEGER,
          primary_label TEXT NOT NULL,
          primary_envelope TEXT,
          primary_risk TEXT,
          secondary_label TEXT NOT NULL,
          secondary_envelope TEXT,
          secondary_risk TEXT,
          chosen TEXT,
          error TEXT,
          resolved_at_ms INTEGER
        );
        CREATE UNIQUE INDEX needs_you_decisions_open_key
          ON needs_you_decisions(key)
          WHERE state IN ('open','answering','failed');
    ",
}];

#[derive(Debug, Clone)]
pub struct RaiseRow {
    pub key: String,
    pub title: String,
    pub reason: String,
    pub raised_at_ms: i64,
    pub expires_at_ms: Option<i64>,
    pub primary_label: String,
    pub primary_envelope: Option<String>,
    pub primary_risk: Option<String>,
    pub secondary_label: String,
    pub secondary_envelope: Option<String>,
    pub secondary_risk: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenKind {
    Open,
    Answering,
    Failed,
}

#[derive(Debug, Clone)]
pub struct OpenKey {
    pub id: i64,
    pub revision: u32,
    pub kind: OpenKind,
}

pub fn migrate(store: &Store) -> anyhow::Result<()> {
    store
        .migrate(OWNER, MIGRATIONS)
        .map_err(|err| anyhow::anyhow!("needs_you migration failed: {err}"))?;
    Ok(())
}

pub fn find_open_key(store: &Store, key: &str) -> Result<Option<OpenKey>, String> {
    store
        .with_conn(|conn| {
            conn.query_row(
                "SELECT id, revision, state FROM needs_you_decisions
                 WHERE key = ?1 AND state IN ('open','answering','failed')",
                [key],
                |row| {
                    let state: String = row.get(2)?;
                    let kind = match state.as_str() {
                        "open" => OpenKind::Open,
                        "answering" => OpenKind::Answering,
                        "failed" => OpenKind::Failed,
                        _ => {
                            return Err(rusqlite::Error::InvalidColumnType(
                                2,
                                "state".to_owned(),
                                rusqlite::types::Type::Text,
                            ));
                        }
                    };
                    Ok(OpenKey {
                        id: row.get(0)?,
                        revision: row.get(1)?,
                        kind,
                    })
                },
            )
            .optional()
        })
        .map_err(|err| err.to_string())
}

pub fn insert_open(store: &Store, row: &RaiseRow) -> Result<(i64, u32), String> {
    store.with_conn(|conn| {
        let tx = conn.unchecked_transaction().map_err(|e| e.to_string())?;
        tx.execute(
            "INSERT INTO needs_you_decisions (
                    key, revision, state, title, reason, raised_at_ms, expires_at_ms,
                    primary_label, primary_envelope, primary_risk,
                    secondary_label, secondary_envelope, secondary_risk
                 ) VALUES (?1, 1, 'open', ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                row.key,
                row.title,
                row.reason,
                row.raised_at_ms,
                row.expires_at_ms,
                row.primary_label,
                row.primary_envelope,
                row.primary_risk,
                row.secondary_label,
                row.secondary_envelope,
                row.secondary_risk,
            ],
        )
        .map_err(|e| e.to_string())?;
        let id = tx.last_insert_rowid();
        tx.commit().map_err(|e| e.to_string())?;
        Ok((id, 1))
    })
}

pub fn replace_open(
    store: &Store,
    id: i64,
    revision: u32,
    row: &RaiseRow,
) -> Result<(i64, u32), String> {
    let next = revision
        .checked_add(1)
        .ok_or_else(|| "revision overflow".to_owned())?;
    store.with_conn(|conn| {
        let tx = conn.unchecked_transaction().map_err(|e| e.to_string())?;
        let changed = tx
            .execute(
                "UPDATE needs_you_decisions SET
                        revision = ?1,
                        state = 'open',
                        title = ?2,
                        reason = ?3,
                        expires_at_ms = ?4,
                        primary_label = ?5,
                        primary_envelope = ?6,
                        primary_risk = ?7,
                        secondary_label = ?8,
                        secondary_envelope = ?9,
                        secondary_risk = ?10,
                        chosen = NULL,
                        error = NULL,
                        resolved_at_ms = NULL
                     WHERE id = ?11 AND state IN ('open','failed')",
                params![
                    next,
                    row.title,
                    row.reason,
                    row.expires_at_ms,
                    row.primary_label,
                    row.primary_envelope,
                    row.primary_risk,
                    row.secondary_label,
                    row.secondary_envelope,
                    row.secondary_risk,
                    id,
                ],
            )
            .map_err(|e| e.to_string())?;
        if changed != 1 {
            return Err("open Decision was not replaced".to_owned());
        }
        tx.commit().map_err(|e| e.to_string())?;
        Ok((id, next))
    })
}

/// Marks an open Decision as answering. Used by later answer work and raise tests.
pub fn mark_answering(store: &Store, id: i64, choice: &str) -> Result<(), String> {
    store.with_conn(|conn| {
        let changed = conn
            .execute(
                "UPDATE needs_you_decisions
                     SET state = 'answering', chosen = ?1
                     WHERE id = ?2 AND state IN ('open','failed')",
                params![choice, id],
            )
            .map_err(|e| e.to_string())?;
        if changed != 1 {
            return Err("Decision was not marked answering".to_owned());
        }
        Ok(())
    })
}

pub fn load_open_decisions(store: &Store) -> Result<Vec<OpenDecision>, String> {
    store.with_conn(|conn| {
        let mut stmt = conn
            .prepare(
                "SELECT id, key, revision, state, title, reason, raised_at_ms, expires_at_ms,
                            primary_label, primary_risk, secondary_label, secondary_risk,
                            chosen, error
                     FROM needs_you_decisions
                     WHERE state IN ('open','answering','failed')
                     ORDER BY id",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |row| {
                let state: String = row.get(3)?;
                let chosen: Option<String> = row.get(12)?;
                let error: Option<String> = row.get(13)?;
                let decision_state = match state.as_str() {
                    "open" => DecisionState::Open,
                    "answering" => DecisionState::Answering {
                        choice: chosen.unwrap_or_default(),
                    },
                    "failed" => DecisionState::Failed {
                        choice: chosen.unwrap_or_default(),
                        error: error.unwrap_or_default(),
                    },
                    _ => {
                        return Err(rusqlite::Error::InvalidColumnType(
                            3,
                            "state".to_owned(),
                            rusqlite::types::Type::Text,
                        ));
                    }
                };
                Ok(OpenDecision {
                    id: row.get(0)?,
                    key: row.get(1)?,
                    revision: row.get(2)?,
                    title: row.get(4)?,
                    reason: row.get(5)?,
                    raised_at_ms: row.get(6)?,
                    expires_at_ms: row.get(7)?,
                    primary: ChoiceState {
                        label: row.get(8)?,
                        risk: row.get(9)?,
                    },
                    secondary: ChoiceState {
                        label: row.get(10)?,
                        risk: row.get(11)?,
                    },
                    state: decision_state,
                })
            })
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())
    })
}

use navori_store::{Migration, MigrationReport, Store};
use rusqlite::{OptionalExtension, params};

use crate::state::{ChoiceState, DecisionState, OpenDecision, UnviewedUpdate};

const OWNER: &str = "needs_you";

pub const MIGRATIONS: &[Migration] = &[
    Migration {
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
    },
    Migration {
        version: 2,
        name: "needs_you_updates",
        sql: "
        CREATE TABLE needs_you_updates (
          id INTEGER PRIMARY KEY,
          kind TEXT NOT NULL,
          title TEXT NOT NULL,
          decision_id INTEGER,
          created_at_ms INTEGER NOT NULL,
          resolved_at_ms INTEGER,
          resolution TEXT
        );
    ",
    },
];

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

pub fn migrate(store: &Store) -> anyhow::Result<MigrationReport> {
    store
        .migrate(OWNER, MIGRATIONS)
        .map_err(|err| anyhow::anyhow!("needs_you migration failed: {err}"))
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
                     SET state = 'answering', chosen = ?1, error = NULL
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

#[derive(Debug, Clone)]
pub struct AnswerRow {
    pub id: i64,
    pub revision: u32,
    /// `None` when the row is already settled (answered/expired).
    pub kind: Option<OpenKind>,
    pub title: String,
    pub expires_at_ms: Option<i64>,
    pub primary_envelope: Option<String>,
    pub secondary_envelope: Option<String>,
}

pub fn find_for_answer(store: &Store, id: i64) -> Result<Option<AnswerRow>, String> {
    store
        .with_conn(|conn| {
            conn.query_row(
                "SELECT id, revision, state, title, expires_at_ms, primary_envelope, secondary_envelope
                 FROM needs_you_decisions WHERE id = ?1",
                [id],
                |row| {
                    let state: String = row.get(2)?;
                    let kind = match state.as_str() {
                        "open" => Some(OpenKind::Open),
                        "answering" => Some(OpenKind::Answering),
                        "failed" => Some(OpenKind::Failed),
                        _ => None,
                    };
                    Ok(AnswerRow {
                        id: row.get(0)?,
                        revision: row.get(1)?,
                        kind,
                        title: row.get(3)?,
                        expires_at_ms: row.get(4)?,
                        primary_envelope: row.get(5)?,
                        secondary_envelope: row.get(6)?,
                    })
                },
            )
            .optional()
        })
        .map_err(|err| err.to_string())
}

pub fn mark_answered(store: &Store, id: i64, resolved_at_ms: i64) -> Result<(), String> {
    store.with_conn(|conn| {
        let changed = conn
            .execute(
                "UPDATE needs_you_decisions
                     SET state = 'answered', error = NULL, resolved_at_ms = ?1
                     WHERE id = ?2 AND state = 'answering'",
                params![resolved_at_ms, id],
            )
            .map_err(|e| e.to_string())?;
        if changed != 1 {
            return Err("Decision was not marked answered".to_owned());
        }
        Ok(())
    })
}

pub fn mark_failed(store: &Store, id: i64, choice: &str, error: &str) -> Result<(), String> {
    store.with_conn(|conn| {
        let changed = conn
            .execute(
                "UPDATE needs_you_decisions
                     SET state = 'failed', chosen = ?1, error = ?2, resolved_at_ms = NULL
                     WHERE id = ?3 AND state IN ('open','answering','failed')",
                params![choice, error, id],
            )
            .map_err(|e| e.to_string())?;
        if changed != 1 {
            return Err("Decision was not marked failed".to_owned());
        }
        Ok(())
    })
}

pub fn answering_ids(store: &Store) -> Result<Vec<(i64, String)>, String> {
    store.with_conn(|conn| {
        let mut stmt = conn
            .prepare(
                "SELECT id, COALESCE(chosen, '') FROM needs_you_decisions WHERE state = 'answering'",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())
    })
}

pub fn update_choice_risks(
    store: &Store,
    id: i64,
    primary_risk: Option<&str>,
    secondary_risk: Option<&str>,
) -> Result<(), String> {
    store.with_conn(|conn| {
        conn.execute(
            "UPDATE needs_you_decisions
                 SET primary_risk = ?1, secondary_risk = ?2
                 WHERE id = ?3 AND state IN ('open','answering','failed')",
            params![primary_risk, secondary_risk, id],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    })
}

#[derive(Debug, Clone)]
pub struct OpenEnvelopes {
    pub id: i64,
    pub primary_envelope: Option<String>,
    pub secondary_envelope: Option<String>,
}

pub fn open_envelopes(store: &Store) -> Result<Vec<OpenEnvelopes>, String> {
    store.with_conn(|conn| {
        let mut stmt = conn
            .prepare(
                "SELECT id, primary_envelope, secondary_envelope FROM needs_you_decisions
                 WHERE state IN ('open','answering','failed')",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |row| {
                Ok(OpenEnvelopes {
                    id: row.get(0)?,
                    primary_envelope: row.get(1)?,
                    secondary_envelope: row.get(2)?,
                })
            })
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())
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

#[derive(Debug, Clone)]
pub struct DueExpiry {
    pub id: i64,
    pub title: String,
}

pub fn due_expiries(store: &Store, now_ms: i64) -> Result<Vec<DueExpiry>, String> {
    store.with_conn(|conn| {
        let mut stmt = conn
            .prepare(
                "SELECT id, title FROM needs_you_decisions
                 WHERE state IN ('open','failed')
                   AND expires_at_ms IS NOT NULL
                   AND expires_at_ms <= ?1
                 ORDER BY id",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([now_ms], |row| {
                Ok(DueExpiry {
                    id: row.get(0)?,
                    title: row.get(1)?,
                })
            })
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())
    })
}

pub fn soonest_expires_at_ms(store: &Store) -> Result<Option<i64>, String> {
    store.with_conn(|conn| {
        conn.query_row(
            "SELECT MIN(expires_at_ms) FROM needs_you_decisions
             WHERE state IN ('open','failed') AND expires_at_ms IS NOT NULL",
            [],
            |row| row.get::<_, Option<i64>>(0),
        )
        .map_err(|e| e.to_string())
    })
}

pub fn expire_to_update(
    store: &Store,
    id: i64,
    title: &str,
    created_at_ms: i64,
) -> Result<i64, String> {
    let update_title = format!("Expired: {title}");
    store.with_conn(|conn| {
        let tx = conn.unchecked_transaction().map_err(|e| e.to_string())?;
        let changed = tx
            .execute(
                "UPDATE needs_you_decisions
                     SET state = 'expired', resolved_at_ms = ?1
                     WHERE id = ?2 AND state IN ('open','failed')",
                params![created_at_ms, id],
            )
            .map_err(|e| e.to_string())?;
        if changed != 1 {
            return Err("Decision was not marked expired".to_owned());
        }
        tx.execute(
            "INSERT INTO needs_you_updates (kind, title, decision_id, created_at_ms)
             VALUES ('decision_expired', ?1, ?2, ?3)",
            params![update_title, id, created_at_ms],
        )
        .map_err(|e| e.to_string())?;
        let update_id = tx.last_insert_rowid();
        tx.commit().map_err(|e| e.to_string())?;
        Ok(update_id)
    })
}

pub fn load_unviewed_updates(store: &Store) -> Result<Vec<UnviewedUpdate>, String> {
    store.with_conn(|conn| {
        let mut stmt = conn
            .prepare(
                "SELECT id, kind, title, created_at_ms FROM needs_you_updates
                 WHERE resolution IS NULL
                 ORDER BY id",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |row| {
                Ok(UnviewedUpdate {
                    id: row.get(0)?,
                    kind: row.get(1)?,
                    title: row.get(2)?,
                    created_at_ms: row.get(3)?,
                })
            })
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())
    })
}

pub fn mark_updates_viewed(store: &Store, ids: &[i64], resolved_at_ms: i64) -> Result<(), String> {
    if ids.is_empty() {
        return Ok(());
    }
    store.with_conn(|conn| {
        let tx = conn.unchecked_transaction().map_err(|e| e.to_string())?;
        for id in ids {
            tx.execute(
                "UPDATE needs_you_updates
                     SET resolution = 'viewed', resolved_at_ms = ?1
                     WHERE id = ?2 AND resolution IS NULL",
                params![resolved_at_ms, id],
            )
            .map_err(|e| e.to_string())?;
        }
        tx.commit().map_err(|e| e.to_string())?;
        Ok(())
    })
}

pub fn clear_unresolved_updates(store: &Store, resolved_at_ms: i64) -> Result<(), String> {
    store.with_conn(|conn| {
        conn.execute(
            "UPDATE needs_you_updates
                 SET resolution = 'cleared', resolved_at_ms = ?1
                 WHERE resolution IS NULL",
            params![resolved_at_ms],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    })
}

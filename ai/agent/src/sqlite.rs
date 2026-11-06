use std::sync::Mutex;

use navori_context::Usage;
use navori_store::{Migration, Store};
use rusqlite::{Connection, OptionalExtension, Transaction, params};

use crate::record::{
    ConversationId, ConversationLog, ConversationStore, Entry, Origin, RecordError,
    RecordedMessage, TurnId,
};

const SESSION_OWNER: &str = "ai_session";

const SESSION_MIGRATIONS: &[Migration] = &[Migration {
    version: 1,
    name: "ai_session_tables",
    sql: "
        CREATE TABLE ai_session_conversation (
          id          TEXT PRIMARY KEY,
          created_ms  INTEGER NOT NULL
        );
        CREATE TABLE ai_session_turn (
          id               TEXT PRIMARY KEY,
          conversation_id  TEXT NOT NULL REFERENCES ai_session_conversation(id),
          seq              INTEGER NOT NULL,
          origin           TEXT NOT NULL,
          model            TEXT NOT NULL,
          started_ms       INTEGER NOT NULL,
          ended_ms         INTEGER,
          end_reason       TEXT,
          UNIQUE (conversation_id, seq)
        );
        CREATE TABLE ai_session_step (
          turn_id     TEXT NOT NULL REFERENCES ai_session_turn(id),
          seq         INTEGER NOT NULL,
          started_ms  INTEGER NOT NULL,
          ended_ms    INTEGER,
          PRIMARY KEY (turn_id, seq)
        );
        CREATE TABLE ai_session_message (
          turn_id      TEXT NOT NULL REFERENCES ai_session_turn(id),
          seq          INTEGER NOT NULL,
          step         INTEGER,
          kind         TEXT NOT NULL,
          call_id      TEXT,
          body         TEXT NOT NULL,
          redactions   INTEGER NOT NULL,
          created_ms   INTEGER NOT NULL,
          PRIMARY KEY (turn_id, seq)
        );
        CREATE UNIQUE INDEX ai_session_one_result_per_call
          ON ai_session_message (turn_id, call_id) WHERE kind = 'tool_result';
        CREATE TABLE ai_session_usage (
          turn_id             TEXT NOT NULL REFERENCES ai_session_turn(id),
          step                INTEGER NOT NULL,
          reported            INTEGER NOT NULL CHECK (reported IN (0,1)),
          input_tokens        INTEGER,
          output_tokens       INTEGER,
          cache_hit_tokens    INTEGER,
          cache_miss_tokens   INTEGER,
          PRIMARY KEY (turn_id, step)
        );
        CREATE INDEX ai_session_turn_by_conversation
          ON ai_session_turn (conversation_id, seq);
    ",
}];

#[derive(Debug)]
pub struct StoreConversations {
    store: Store,
    migrated: Mutex<bool>,
}

impl StoreConversations {
    pub fn new(store: Store) -> Self {
        Self {
            store,
            migrated: Mutex::new(false),
        }
    }

    fn ensure_table(&self) -> Result<(), RecordError> {
        let mut migrated = self
            .migrated
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if !*migrated {
            self.store
                .migrate(SESSION_OWNER, SESSION_MIGRATIONS)
                .map_err(|error| {
                    tracing::error!(owner = SESSION_OWNER, %error, "conversation tables could not be migrated");
                    RecordError::Migration
                })?;
            *migrated = true;
        }
        Ok(())
    }
}

fn count(value: u64) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

fn write_error(error: rusqlite::Error) -> RecordError {
    match &error {
        rusqlite::Error::SqliteFailure(failure, _)
            if failure.extended_code == rusqlite::ffi::SQLITE_CONSTRAINT_UNIQUE
                || failure.extended_code == rusqlite::ffi::SQLITE_CONSTRAINT_PRIMARYKEY =>
        {
            RecordError::Duplicate
        }
        _ => {
            tracing::warn!(code = sqlite_code(&error), "conversation write failed");
            RecordError::Write
        }
    }
}

fn sqlite_code(error: &rusqlite::Error) -> i32 {
    match error {
        rusqlite::Error::SqliteFailure(failure, _) => failure.extended_code,
        _ => -1,
    }
}

fn insert(tx: &Transaction<'_>, entry: &Entry) -> rusqlite::Result<()> {
    match entry {
        Entry::ConversationStarted { id, ts_ms } => tx.execute(
            "INSERT INTO ai_session_conversation (id, created_ms) VALUES (?1, ?2)",
            params![id.to_string(), ts_ms],
        ),
        Entry::TurnStarted {
            id,
            conversation,
            seq,
            origin,
            model,
            ts_ms,
        } => tx.execute(
            "INSERT INTO ai_session_turn (id, conversation_id, seq, origin, model, started_ms) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                id.to_string(),
                conversation.to_string(),
                seq,
                origin.as_str(),
                model,
                ts_ms
            ],
        ),
        Entry::StepStarted { turn, seq, ts_ms } => tx.execute(
            "INSERT INTO ai_session_step (turn_id, seq, started_ms) VALUES (?1, ?2, ?3)",
            params![turn.to_string(), seq, ts_ms],
        ),
        Entry::Message {
            turn,
            seq,
            step,
            body,
            redactions,
            ts_ms,
        } => {
            let (kind, call_id) = match body {
                RecordedMessage::Input { .. } => ("input", None),
                RecordedMessage::Assistant { .. } => ("assistant", None),
                RecordedMessage::ToolResult { call_id, .. } => ("tool_result", Some(call_id)),
            };
            tx.execute(
                "INSERT INTO ai_session_message (turn_id, seq, step, kind, call_id, body, redactions, created_ms) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    turn.to_string(),
                    seq,
                    step,
                    kind,
                    call_id,
                    body.to_body(),
                    redactions,
                    ts_ms
                ],
            )
        }
        Entry::Usage { turn, step, usage } => match usage {
            Some(Usage {
                input,
                output,
                cache_hit,
                cache_miss,
            }) => tx.execute(
                "INSERT INTO ai_session_usage (turn_id, step, reported, input_tokens, output_tokens, cache_hit_tokens, cache_miss_tokens) VALUES (?1, ?2, 1, ?3, ?4, ?5, ?6)",
                params![
                    turn.to_string(),
                    step,
                    count(*input),
                    count(*output),
                    cache_hit.map(count),
                    cache_miss.map(count)
                ],
            ),
            None => tx.execute(
                "INSERT INTO ai_session_usage (turn_id, step, reported) VALUES (?1, ?2, 0)",
                params![turn.to_string(), step],
            ),
        },
        Entry::StepEnded { turn, seq, ts_ms } => tx.execute(
            "UPDATE ai_session_step SET ended_ms = ?3 WHERE turn_id = ?1 AND seq = ?2",
            params![turn.to_string(), seq, ts_ms],
        ),
        Entry::TurnEnded { turn, end, ts_ms } => tx.execute(
            "UPDATE ai_session_turn SET ended_ms = ?2, end_reason = ?3 WHERE id = ?1",
            params![turn.to_string(), ts_ms, end],
        ),
    }
    .map(|_| ())
}

type TurnRow = (
    String,
    u32,
    String,
    String,
    i64,
    Option<i64>,
    Option<String>,
);

fn read_entries(
    conn: &Connection,
    conversation: &ConversationId,
) -> Result<Vec<Entry>, RecordError> {
    let read = |error: rusqlite::Error| {
        tracing::warn!(code = sqlite_code(&error), "conversation read failed");
        RecordError::Read
    };
    let id = conversation.to_string();
    let created: Option<i64> = conn
        .query_row(
            "SELECT created_ms FROM ai_session_conversation WHERE id = ?1",
            [&id],
            |row| row.get(0),
        )
        .optional()
        .map_err(read)?;
    let created = created.ok_or(RecordError::UnknownConversation)?;
    let mut entries = vec![Entry::ConversationStarted {
        id: *conversation,
        ts_ms: created,
    }];
    let mut turns = conn
        .prepare("SELECT id, seq, origin, model, started_ms, ended_ms, end_reason FROM ai_session_turn WHERE conversation_id = ?1 ORDER BY seq")
        .map_err(read)?;
    let turn_rows: Vec<TurnRow> = turns
        .query_map([&id], |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
                row.get(5)?,
                row.get(6)?,
            ))
        })
        .map_err(read)?
        .collect::<rusqlite::Result<_>>()
        .map_err(read)?;
    for (turn_id, seq, origin, model, started, ended, end_reason) in turn_rows {
        let turn: TurnId = turn_id.parse()?;
        let origin = Origin::parse(&origin).ok_or(RecordError::Read)?;
        entries.push(Entry::TurnStarted {
            id: turn,
            conversation: *conversation,
            seq,
            origin,
            model,
            ts_ms: started,
        });
        let mut steps = conn
            .prepare("SELECT seq, started_ms, ended_ms FROM ai_session_step WHERE turn_id = ?1 ORDER BY seq")
            .map_err(read)?;
        let step_rows: Vec<(u32, i64, Option<i64>)> = steps
            .query_map([&turn_id], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })
            .map_err(read)?
            .collect::<rusqlite::Result<_>>()
            .map_err(read)?;
        for (step, started, ended) in step_rows {
            entries.push(Entry::StepStarted {
                turn,
                seq: step,
                ts_ms: started,
            });
            if let Some(ended) = ended {
                entries.push(Entry::StepEnded {
                    turn,
                    seq: step,
                    ts_ms: ended,
                });
            }
        }
        let mut messages = conn
            .prepare("SELECT seq, step, body, redactions, created_ms FROM ai_session_message WHERE turn_id = ?1 ORDER BY seq")
            .map_err(read)?;
        let message_rows: Vec<(u32, Option<u32>, String, u32, i64)> = messages
            .query_map([&turn_id], |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            })
            .map_err(read)?
            .collect::<rusqlite::Result<_>>()
            .map_err(read)?;
        for (seq, step, body, redactions, created) in message_rows {
            entries.push(Entry::Message {
                turn,
                seq,
                step,
                body: RecordedMessage::from_body(&body)?,
                redactions,
                ts_ms: created,
            });
        }
        if let (Some(ended), Some(end)) = (ended, end_reason) {
            entries.push(Entry::TurnEnded {
                turn,
                end,
                ts_ms: ended,
            });
        }
    }
    Ok(entries)
}

impl ConversationStore for StoreConversations {
    fn append(&self, entries: &[Entry]) -> Result<(), RecordError> {
        self.ensure_table()?;
        self.store.with_conn(|conn| {
            let tx = conn.unchecked_transaction().map_err(write_error)?;
            for entry in entries {
                insert(&tx, entry).map_err(write_error)?;
            }
            tx.commit().map_err(write_error)
        })
    }

    fn load(&self, conversation: &ConversationId) -> Result<ConversationLog, RecordError> {
        self.ensure_table()?;
        let entries = self
            .store
            .with_conn(|conn| read_entries(conn, conversation))?;
        ConversationLog::from_entries(entries)
    }
}

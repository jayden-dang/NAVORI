use navori_store::{Migration, MigrationReport, Store};
use rusqlite::params;
use serde::Serialize;

use crate::state::OpenTodo;

const OWNER: &str = "todo";

pub const MIGRATIONS: &[Migration] = &[Migration {
    version: 1,
    name: "todo_items",
    sql: "
        CREATE TABLE todo_items (
          id INTEGER PRIMARY KEY,
          title TEXT NOT NULL,
          due_on TEXT,
          created_at_ms INTEGER NOT NULL,
          done_at_ms INTEGER
        );
    ",
}];

pub fn migrate(store: &Store) -> anyhow::Result<MigrationReport> {
    store
        .migrate(OWNER, MIGRATIONS)
        .map_err(|err| anyhow::anyhow!("todo migration failed: {err}"))
}

pub fn insert_open(
    store: &Store,
    title: &str,
    due_on: Option<&str>,
    created_at_ms: i64,
) -> Result<i64, String> {
    store.with_conn(|conn| {
        conn.execute(
            "INSERT INTO todo_items (title, due_on, created_at_ms, done_at_ms)
             VALUES (?1, ?2, ?3, NULL)",
            params![title, due_on, created_at_ms],
        )
        .map_err(|e| e.to_string())?;
        Ok(conn.last_insert_rowid())
    })
}

#[derive(Debug, Clone)]
pub struct OpenRow {
    pub id: i64,
    pub title: String,
    pub due_on: Option<String>,
}

pub fn load_open(store: &Store) -> Result<Vec<OpenRow>, String> {
    store.with_conn(|conn| {
        let mut stmt = conn
            .prepare(
                "SELECT id, title, due_on FROM todo_items
                 WHERE done_at_ms IS NULL
                 ORDER BY id",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |row| {
                Ok(OpenRow {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    due_on: row.get(2)?,
                })
            })
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())
    })
}

pub fn into_open_todos(
    rows: Vec<OpenRow>,
    status_for: impl Fn(Option<&str>) -> crate::state::TodoStatus,
) -> Vec<OpenTodo> {
    rows.into_iter()
        .map(|row| OpenTodo {
            id: row.id,
            status: status_for(row.due_on.as_deref()),
            title: row.title,
            due_on: row.due_on,
        })
        .collect()
}

pub fn mark_done(store: &Store, id: i64, done_at_ms: i64) -> Result<(), String> {
    store.with_conn(|conn| {
        let changed = conn
            .execute(
                "UPDATE todo_items SET done_at_ms = ?1
                 WHERE id = ?2 AND done_at_ms IS NULL",
                params![done_at_ms, id],
            )
            .map_err(|e| e.to_string())?;
        if changed != 1 {
            return Err("no open To-do with that id".into());
        }
        Ok(())
    })
}

pub fn clear_done(store: &Store, id: i64) -> Result<(), String> {
    store.with_conn(|conn| {
        let changed = conn
            .execute(
                "UPDATE todo_items SET done_at_ms = NULL
                 WHERE id = ?1 AND done_at_ms IS NOT NULL",
                params![id],
            )
            .map_err(|e| e.to_string())?;
        if changed != 1 {
            return Err("no ticked To-do with that id".into());
        }
        Ok(())
    })
}

#[derive(Debug, Clone, Serialize)]
pub struct HistoryRow {
    pub id: i64,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub due_on: Option<String>,
    pub done_at_ms: i64,
}

pub fn load_history(store: &Store, cutoff: i64) -> Result<Vec<HistoryRow>, String> {
    store.with_conn(|conn| {
        let mut stmt = conn
            .prepare(
                "SELECT id, title, due_on, done_at_ms FROM todo_items
                 WHERE done_at_ms IS NOT NULL
                   AND done_at_ms >= ?1
                 ORDER BY done_at_ms DESC, id DESC",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([cutoff], |row| {
                Ok(HistoryRow {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    due_on: row.get(2)?,
                    done_at_ms: row.get(3)?,
                })
            })
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())
    })
}

pub fn prune_older_than(store: &Store, cutoff: i64) -> Result<(), String> {
    store.with_conn(|conn| {
        conn.execute(
            "DELETE FROM todo_items
             WHERE done_at_ms IS NOT NULL
               AND done_at_ms < ?1",
            params![cutoff],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    })
}

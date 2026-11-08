use navori_store::{Migration, MigrationReport, Store};
use rusqlite::params;

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

use std::fs;

use navori_store::{Migration, Store, StoreError};

const V1: Migration = Migration {
    version: 1,
    name: "notes",
    sql: "CREATE TABLE notes (id INTEGER PRIMARY KEY, body TEXT NOT NULL);",
};
const V2: Migration = Migration {
    version: 2,
    name: "notes_pinned",
    sql: "ALTER TABLE notes ADD COLUMN pinned INTEGER NOT NULL DEFAULT 0;",
};
const BROKEN: Migration = Migration {
    version: 3,
    name: "broken",
    sql: "CREATE TABLE ok_part (x INTEGER); THIS IS NOT SQL;",
};

#[test]
fn opening_creates_the_database_and_parent_directories_in_wal_mode() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("nested/deeper/navori.db");
    let store = Store::open(&db, &dir.path().join("backups")).unwrap();
    assert!(db.exists());
    assert!(store.integrity_ok().unwrap());
    assert!(store.schema_version("core").unwrap() >= 1);
    let mode: String = store
        .with_conn(|c| c.query_row("PRAGMA journal_mode", [], |r| r.get(0)))
        .unwrap();
    assert_eq!(mode, "wal");
}

#[test]
fn owner_migrations_apply_in_order_once() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("navori.db");
    let store = Store::open(&db, &dir.path().join("backups")).unwrap();
    let first = store.migrate("notes", &[V2, V1]).unwrap();
    assert_eq!(first.applied, vec![1, 2]);
    assert_eq!(store.schema_version("notes").unwrap(), 2);
    drop(store);

    let store = Store::open(&db, &dir.path().join("backups")).unwrap();
    let again = store.migrate("notes", &[V1, V2]).unwrap();
    assert!(again.applied.is_empty());
    assert!(again.backup.is_none());
}

#[test]
fn owners_are_versioned_independently() {
    let store = Store::open_in_memory().unwrap();
    store.migrate("notes", &[V1]).unwrap();
    assert_eq!(store.schema_version("notes").unwrap(), 1);
    assert_eq!(store.schema_version("github").unwrap(), 0);
}

#[test]
fn existing_data_is_backed_up_before_a_new_migration_runs() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("navori.db");
    let backups = dir.path().join("backups");
    let store = Store::open(&db, &backups).unwrap();
    store.migrate("notes", &[V1]).unwrap();
    store
        .with_conn(|c| c.execute("INSERT INTO notes (body) VALUES ('keep me')", []))
        .unwrap();

    let report = store.migrate("notes", &[V1, V2]).unwrap();
    assert_eq!(report.applied, vec![2]);
    let backup = report.backup.expect("a backup must be taken");
    assert!(backup.starts_with(&backups));

    let old = rusqlite::Connection::open(&backup).unwrap();
    let body: String = old
        .query_row("SELECT body FROM notes", [], |r| r.get(0))
        .unwrap();
    assert_eq!(body, "keep me");
    let has_pinned = old.prepare("SELECT pinned FROM notes").is_ok();
    assert!(!has_pinned, "backup must hold the pre-migration schema");
}

#[test]
fn a_failing_migration_rolls_back_and_reports_which_one() {
    let store = Store::open_in_memory().unwrap();
    store.migrate("notes", &[V1]).unwrap();
    let err = store.migrate("notes", &[V1, BROKEN]).unwrap_err();
    assert!(
        matches!(err, StoreError::Migration { version: 3, .. }),
        "{err}"
    );
    assert!(err.to_string().contains("broken"));
    assert_eq!(store.schema_version("notes").unwrap(), 1);
    let partial = store.with_conn(|c| c.prepare("SELECT x FROM ok_part").is_ok());
    assert!(
        !partial,
        "statements before the failure must be rolled back"
    );
}

#[test]
fn only_the_five_newest_backups_are_kept() {
    let dir = tempfile::tempdir().unwrap();
    let backups = dir.path().join("backups");
    let store = Store::open(&dir.path().join("navori.db"), &backups).unwrap();
    store
        .migrate(
            "t",
            &[Migration {
                version: 1,
                name: "v1",
                sql: "CREATE TABLE t1 (x INTEGER);",
            }],
        )
        .unwrap();
    let extra: Vec<Migration> = (2..=8)
        .map(|v| Migration {
            version: v,
            name: "step",
            sql: "SELECT 1;",
        })
        .collect();
    let mut all = vec![Migration {
        version: 1,
        name: "v1",
        sql: "CREATE TABLE t1 (x INTEGER);",
    }];
    for m in extra {
        all.push(m);
        store.migrate("t", &all).unwrap();
    }
    let count = fs::read_dir(&backups).unwrap().count();
    assert_eq!(count, 5);
}

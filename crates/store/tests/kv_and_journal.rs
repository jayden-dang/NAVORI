use navori_protocol::{Event, Severity};
use navori_store::Store;
use serde_json::json;

fn event(id: &str, ts_ms: i64, summary: &str) -> Event {
    Event {
        id: id.to_owned(),
        ts_ms,
        module: "core".to_owned(),
        kind: "test".to_owned(),
        severity: Severity::Info,
        summary: summary.to_owned(),
        payload: json!({ "n": ts_ms }),
    }
}

#[test]
fn kv_values_round_trip_and_overwrite() {
    let store = Store::open_in_memory().unwrap();
    assert_eq!(store.kv_get("ui", "theme").unwrap(), None);
    store.kv_set("ui", "theme", &json!("dark")).unwrap();
    assert_eq!(store.kv_get("ui", "theme").unwrap(), Some(json!("dark")));
    store
        .kv_set("ui", "theme", &json!({ "mode": "light" }))
        .unwrap();
    assert_eq!(
        store.kv_get("ui", "theme").unwrap(),
        Some(json!({ "mode": "light" }))
    );
}

#[test]
fn kv_namespaces_do_not_collide() {
    let store = Store::open_in_memory().unwrap();
    store.kv_set("github", "cursor", &json!(1)).unwrap();
    store.kv_set("ports", "cursor", &json!(2)).unwrap();
    assert_eq!(store.kv_get("github", "cursor").unwrap(), Some(json!(1)));
    assert_eq!(store.kv_get("ports", "cursor").unwrap(), Some(json!(2)));
}

#[test]
fn a_deleted_key_reads_as_absent() {
    let store = Store::open_in_memory().unwrap();
    store.kv_set("awake", "lid_held", &json!(true)).unwrap();
    store.kv_delete("awake", "lid_held").unwrap();
    assert_eq!(store.kv_get("awake", "lid_held").unwrap(), None);
}

#[test]
fn deleting_a_missing_key_is_not_an_error() {
    let store = Store::open_in_memory().unwrap();
    store.kv_delete("awake", "lid_held").unwrap();
    assert_eq!(store.kv_get("awake", "lid_held").unwrap(), None);
}

#[test]
fn deleting_one_key_leaves_its_neighbours() {
    let store = Store::open_in_memory().unwrap();
    store.kv_set("awake", "lid_held", &json!(true)).unwrap();
    store.kv_set("awake", "tracking", &json!(1)).unwrap();
    store.kv_set("other", "lid_held", &json!(2)).unwrap();
    store.kv_delete("awake", "lid_held").unwrap();
    assert_eq!(store.kv_get("awake", "tracking").unwrap(), Some(json!(1)));
    assert_eq!(store.kv_get("other", "lid_held").unwrap(), Some(json!(2)));
}

#[test]
fn journal_returns_newest_events_first_up_to_the_limit() {
    let store = Store::open_in_memory().unwrap();
    for (i, ts) in [100, 300, 200].into_iter().enumerate() {
        store
            .journal_append(&event(&format!("e{i}"), ts, "x"))
            .unwrap();
    }
    let recent = store.journal_recent(2).unwrap();
    assert_eq!(
        recent.iter().map(|e| e.ts_ms).collect::<Vec<_>>(),
        vec![300, 200]
    );
    assert_eq!(recent[0].payload, json!({ "n": 300 }));
}

#[test]
fn journal_breaks_same_millisecond_ties_by_write_order_not_id() {
    let store = Store::open_in_memory().unwrap();
    // The first written event has the larger id string, as random ULIDs may.
    store.journal_append(&event("zzz", 100, "first")).unwrap();
    store.journal_append(&event("aaa", 100, "second")).unwrap();
    let recent = store.journal_recent(2).unwrap();
    assert_eq!(
        recent
            .iter()
            .map(|e| e.summary.as_str())
            .collect::<Vec<_>>(),
        vec!["second", "first"]
    );
}

#[test]
fn journal_orders_by_timestamp_before_write_order() {
    let store = Store::open_in_memory().unwrap();
    store.journal_append(&event("a", 300, "newest ts")).unwrap();
    store.journal_append(&event("b", 100, "oldest ts")).unwrap();
    store.journal_append(&event("c", 200, "middle ts")).unwrap();
    let recent = store.journal_recent(3).unwrap();
    assert_eq!(
        recent
            .iter()
            .map(|e| e.summary.as_str())
            .collect::<Vec<_>>(),
        vec!["newest ts", "middle ts", "oldest ts"]
    );
}

#[test]
fn journal_never_persists_secrets() {
    let store = Store::open_in_memory().unwrap();
    let mut e = event(
        "e1",
        1,
        "pushed with ghp_A1b2C3d4E5f6G7h8I9j0K1l2M3n4O5p6Q7r8",
    );
    e.payload = json!({ "api_key": "abc123", "note": "AKIAIOSFODNN7EXAMPLE" });
    store.journal_append(&e).unwrap();

    let raw: String = store
        .with_conn(|c| c.query_row("SELECT summary || payload FROM events", [], |r| r.get(0)))
        .unwrap();
    assert!(!raw.contains("ghp_A1b2"), "{raw}");
    assert!(!raw.contains("abc123"), "{raw}");
    assert!(!raw.contains("AKIAIOSFODNN7EXAMPLE"), "{raw}");
}

#[test]
fn pruning_removes_only_events_older_than_the_cutoff() {
    let store = Store::open_in_memory().unwrap();
    for (i, ts) in [10, 20, 30].into_iter().enumerate() {
        store
            .journal_append(&event(&format!("e{i}"), ts, "x"))
            .unwrap();
    }
    assert_eq!(store.journal_prune(25).unwrap(), 2);
    let left = store.journal_recent(10).unwrap();
    assert_eq!(left.len(), 1);
    assert_eq!(left[0].ts_ms, 30);
}

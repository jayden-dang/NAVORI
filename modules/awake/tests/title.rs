#![allow(clippy::unwrap_used)]

mod common;

use common::Rig;
use navori_macos::fake::FakeProcess;
use serde_json::{Value, json};

const CLAUDE: i32 = 100;
const HOOK: i32 = 102;
const SESSION: &str = "sess-1";

fn hook_chain(rig: &Rig) {
    rig.processes.add(FakeProcess::new(
        CLAUDE,
        1,
        501,
        100_000,
        "/usr/local/bin/claude",
    ));
    rig.processes.add(FakeProcess::new(
        HOOK,
        CLAUDE,
        501,
        102_000,
        "/usr/local/bin/navori",
    ));
}

fn hold(title: Option<&str>) -> Value {
    let mut fields = json!({ "session_id": SESSION, "pid": HOOK, "ppid": CLAUDE });
    if let Some(title) = title {
        fields["title"] = json!(title);
    }
    json!({ "hold_session": fields })
}

fn stop(title: &str) -> Value {
    json!({ "stop_session": { "session_id": SESSION, "title": title } })
}

fn title_of(rig: &Rig) -> Value {
    rig.kernel.state("awake").unwrap()["leases"][0]["title"].clone()
}

#[tokio::test(start_paused = true)]
async fn a_session_has_no_title_until_a_hook_sends_one() {
    let rig = Rig::new();
    hook_chain(&rig);
    rig.dispatch(hold(None)).await.unwrap();
    assert_eq!(title_of(&rig), Value::Null);
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn a_hold_sets_the_title_and_a_later_hold_without_one_keeps_it() {
    let rig = Rig::new();
    hook_chain(&rig);
    rig.dispatch(hold(Some("Road-128 ticket shaping")))
        .await
        .unwrap();
    assert_eq!(title_of(&rig), "Road-128 ticket shaping");
    rig.dispatch(hold(None)).await.unwrap();
    assert_eq!(title_of(&rig), "Road-128 ticket shaping");
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn a_stop_updates_the_title() {
    let rig = Rig::new();
    hook_chain(&rig);
    rig.dispatch(hold(None)).await.unwrap();
    rig.dispatch(stop("First title")).await.unwrap();
    assert_eq!(title_of(&rig), "First title");
    rig.dispatch(stop("Second title")).await.unwrap();
    assert_eq!(title_of(&rig), "Second title");
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn a_title_is_cleaned_of_control_characters_and_kept_short() {
    let rig = Rig::new();
    hook_chain(&rig);
    rig.dispatch(hold(Some("  Fix\nthe\u{1b}[31m menu\t ")))
        .await
        .unwrap();
    assert_eq!(title_of(&rig), "Fix the[31m menu");

    let long = "é".repeat(300);
    rig.dispatch(hold(Some(&long))).await.unwrap();
    let kept = title_of(&rig);
    assert_eq!(kept.as_str().unwrap().chars().count(), 120);
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn a_blank_title_changes_nothing() {
    let rig = Rig::new();
    hook_chain(&rig);
    rig.dispatch(hold(Some("Kept"))).await.unwrap();
    rig.dispatch(hold(Some(" \n\t "))).await.unwrap();
    assert_eq!(title_of(&rig), "Kept");
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn a_title_is_not_stored_and_stays_out_of_events() {
    let mut rig = Rig::new();
    hook_chain(&rig);
    rig.dispatch(hold(Some("SECRET-TITLE"))).await.unwrap();
    rig.dispatch(stop("SECRET-TITLE")).await.unwrap();

    let stored: String = rig.kernel.store().with_conn(|conn| {
        conn.query_row(
            "SELECT group_concat(key || coalesce(session_id, '') || coalesce(harness, '')) FROM awake_leases",
            [],
            |row| row.get(0),
        )
        .unwrap()
    });
    assert!(!stored.contains("SECRET-TITLE"));
    for event in rig.kernel.recent_events(20).unwrap() {
        assert!(!event.summary.contains("SECRET-TITLE"), "{}", event.summary);
        assert!(!event.payload.to_string().contains("SECRET-TITLE"));
    }

    rig.reopen().await;
    assert_eq!(title_of(&rig), Value::Null, "the next hook sends it again");
    rig.shutdown().await;
}

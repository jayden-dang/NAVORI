#![allow(clippy::unwrap_used)]

mod common;

use common::{ASSERTION_NAME, ORIGIN_MS, Rig};
use navori_kernel::{ModuleAction, Severity};
use navori_macos::AssertionKind;
use navori_mod_awake::AwakeAction;
use serde_json::{Value, json};

fn display(on: bool) -> Value {
    json!({ "display": { "on": on } })
}

fn hold(seconds: u64) -> Value {
    json!({ "hold_for": { "seconds": seconds } })
}

fn until_off() -> Value {
    json!("hold_until_off")
}

fn release_all() -> Value {
    json!("release_all")
}

fn lease_count(state: &Value) -> usize {
    state["leases"].as_array().map(Vec::len).unwrap_or(0)
}

fn system_only() -> Vec<(AssertionKind, String)> {
    vec![(AssertionKind::System, ASSERTION_NAME.to_owned())]
}

fn system_and_display() -> Vec<(AssertionKind, String)> {
    vec![
        (AssertionKind::System, ASSERTION_NAME.to_owned()),
        (AssertionKind::Display, ASSERTION_NAME.to_owned()),
    ]
}

fn started_summaries(rig: &Rig) -> Vec<String> {
    rig.kernel
        .recent_events(10)
        .unwrap()
        .into_iter()
        .filter(|event| event.kind == "awake.lease_started")
        .map(|event| event.summary)
        .collect()
}

fn ended_summaries(rig: &Rig) -> Vec<String> {
    rig.kernel
        .recent_events(10)
        .unwrap()
        .into_iter()
        .filter(|event| event.kind == "awake.lease_ended")
        .map(|event| event.summary)
        .collect()
}

fn lease_row_count(rig: &Rig) -> i64 {
    rig.kernel.store().with_conn(|conn| {
        conn.query_row("SELECT COUNT(*) FROM awake_leases", [], |row| row.get(0))
            .unwrap_or(-1)
    })
}

#[tokio::test(start_paused = true)]
async fn display_on_holds_one_display_and_one_system_assertion() {
    let rig = Rig::new();
    rig.dispatch(until_off()).await.unwrap();
    assert_eq!(rig.power.held(), system_only());

    let state = rig.dispatch(display(true)).await.unwrap();

    assert_eq!(rig.power.held(), system_and_display());
    assert_eq!(rig.power.releases(), 0);
    assert_eq!(rig.processes.list_calls(), 0);
    assert_eq!(lease_count(&state), 2);
    assert_eq!(state["leases"][0]["kind"], "until_off");
    assert_eq!(
        state["leases"][1],
        json!({
            "key": "display",
            "kind": "display",
            "harness": null,
            "label": "Display",
            "pid": null,
            "stopped": false,
            "ends_at_ms": null,
            "title": null
        })
    );
    assert_eq!(state["system_held"], true);
    assert_eq!(state["display_held"], true);
    assert_eq!(state["since_ms"], ORIGIN_MS as i64);
    assert_eq!(state["problem"], Value::Null);
    assert_eq!(state, rig.kernel.state("awake").unwrap());
    assert_eq!(
        started_summaries(&rig),
        vec![
            "Keeping this Mac awake: display.".to_owned(),
            "Keeping this Mac awake: until off.".to_owned(),
        ]
    );
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn display_off_with_another_lease_keeps_system_with_no_gap() {
    let rig = Rig::new();
    rig.dispatch(until_off()).await.unwrap();
    rig.dispatch(display(true)).await.unwrap();
    assert_eq!(rig.power.held(), system_and_display());
    assert_eq!(rig.power.releases(), 0);

    let state = rig.dispatch(display(false)).await.unwrap();

    assert_eq!(rig.power.releases(), 1);
    assert_eq!(rig.power.held(), system_only());
    assert_eq!(lease_count(&state), 1);
    assert_eq!(state["leases"][0]["kind"], "until_off");
    assert_eq!(state["system_held"], true);
    assert_eq!(state["display_held"], false);
    assert_eq!(state["since_ms"], ORIGIN_MS as i64);
    assert_eq!(
        ended_summaries(&rig),
        vec!["Stopped keeping this Mac awake: display.".to_owned()]
    );
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn display_on_with_no_other_lease_holds_system_too() {
    let rig = Rig::new();
    let state = rig.dispatch(display(true)).await.unwrap();

    assert_eq!(rig.power.held(), system_and_display());
    assert_eq!(rig.power.releases(), 0);
    assert_eq!(
        state,
        json!({
            "leases": [{
                "key": "display",
                "kind": "display",
                "harness": null,
                "label": "Display",
                "pid": null,
                "stopped": false,
                "ends_at_ms": null,
                "title": null
            }],
            "system_held": true,
            "display_held": true,
            "since_ms": ORIGIN_MS as i64,
            "problem": null,
            "tracking": true
        })
    );
    let events = rig.kernel.recent_events(10).unwrap();
    let started: Vec<_> = events
        .iter()
        .filter(|event| event.kind == "awake.lease_started")
        .collect();
    assert_eq!(started.len(), 1);
    assert_eq!(started[0].module, "awake");
    assert_eq!(started[0].severity, Severity::Info);
    assert_eq!(started[0].summary, "Keeping this Mac awake: display.");
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn display_off_with_no_other_lease_releases_both() {
    let rig = Rig::new();
    rig.dispatch(display(true)).await.unwrap();
    assert_eq!(rig.power.held(), system_and_display());

    let state = rig.dispatch(display(false)).await.unwrap();

    assert!(rig.power.held().is_empty());
    assert_eq!(rig.power.releases(), 2);
    assert_eq!(lease_count(&state), 0);
    assert_eq!(state["system_held"], false);
    assert_eq!(state["display_held"], false);
    assert_eq!(state["since_ms"], Value::Null);
    assert_eq!(
        ended_summaries(&rig),
        vec!["Stopped keeping this Mac awake: display.".to_owned()]
    );
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn a_refused_display_keeps_system_and_reports_display_not_held() {
    let rig = Rig::new();
    rig.power.refuse_kind(AssertionKind::Display, 17);

    let state = rig.dispatch(display(true)).await.unwrap();

    assert_eq!(rig.power.held(), system_only());
    assert_eq!(rig.power.releases(), 0);
    assert_eq!(lease_count(&state), 1);
    assert_eq!(state["leases"][0]["key"], "display");
    assert_eq!(state["system_held"], true);
    assert_eq!(state["display_held"], false);
    assert_eq!(state, rig.kernel.state("awake").unwrap());
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn a_refused_system_still_tries_display() {
    let rig = Rig::new();
    rig.power.refuse_kind(AssertionKind::System, 19);

    let state = rig.dispatch(display(true)).await.unwrap();

    assert_eq!(
        rig.power.held(),
        vec![(AssertionKind::Display, ASSERTION_NAME.to_owned())]
    );
    assert_eq!(rig.power.releases(), 0);
    assert_eq!(lease_count(&state), 1);
    assert_eq!(state["leases"][0]["kind"], "display");
    assert_eq!(state["system_held"], false);
    assert_eq!(state["display_held"], true);
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn release_all_ends_the_display_lease() {
    let rig = Rig::new();
    rig.dispatch(hold(3600)).await.unwrap();
    rig.dispatch(display(true)).await.unwrap();
    assert_eq!(rig.power.held(), system_and_display());

    let state = rig.dispatch(release_all()).await.unwrap();

    assert_eq!(lease_count(&state), 0);
    assert_eq!(state["system_held"], false);
    assert_eq!(state["display_held"], false);
    assert!(rig.power.held().is_empty());
    assert_eq!(rig.power.releases(), 2);
    let events = rig.kernel.recent_events(10).unwrap();
    let ended: Vec<_> = events
        .iter()
        .filter(|event| event.kind == "awake.lease_ended")
        .collect();
    assert_eq!(ended.len(), 1);
    assert_eq!(ended[0].module, "awake");
    assert_eq!(ended[0].severity, Severity::Info);
    assert_eq!(
        ended[0].summary,
        "Stopped keeping this Mac awake: Release all."
    );
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn display_writes_no_row() {
    let rig = Rig::new();
    let state = rig.dispatch(display(true)).await.unwrap();
    assert_eq!(lease_count(&state), 1);
    assert_eq!(state["leases"][0]["kind"], "display");
    assert_eq!(rig.power.held(), system_and_display());
    assert_eq!(lease_row_count(&rig), 0);

    rig.dispatch(hold(3600)).await.unwrap();
    assert_eq!(lease_row_count(&rig), 0);
    rig.shutdown().await;
}

#[test]
fn only_display_release_all_and_the_keep_actions_end_a_display_lease() {
    let AwakeAction::Display { on } = (AwakeAction::Display { on: false }) else {
        unreachable!("display off");
    };
    assert!(!on);

    let names: Vec<&str> = AwakeAction::catalog().iter().map(|doc| doc.name).collect();
    let not_ending_manual = [
        "status",
        "hold_for",
        "hold_until_off",
        "hold_command",
        "hold_session",
        "stop_session",
        "end_session",
        "tracking",
        "subagent_start",
        "subagent_stop",
    ];
    let ending: Vec<&str> = names
        .into_iter()
        .filter(|name| !not_ending_manual.contains(name))
        .collect();
    assert_eq!(
        ending,
        vec![
            "display",
            "release_all",
            "keep_for",
            "keep_indefinitely",
            "keep_off"
        ]
    );
}

mod common;

use std::time::Duration;

use common::{ASSERTION_NAME, ORIGIN_MS, Rig};
use navori_kernel::{ModuleAction, Severity};
use navori_macos::AssertionKind;
use navori_mod_awake::AwakeAction;
use serde_json::{Value, json};

fn until_off() -> Value {
    json!("hold_until_off")
}

fn lease_count(state: &Value) -> usize {
    state["leases"].as_array().map(Vec::len).unwrap_or(0)
}

#[tokio::test(start_paused = true)]
async fn an_until_off_lease_has_no_deadline_and_holds_past_any_hour() {
    let rig = Rig::new();
    let state = rig.dispatch(until_off()).await.unwrap();

    assert_eq!(
        rig.power.held(),
        vec![(AssertionKind::System, ASSERTION_NAME.to_owned())]
    );
    assert_eq!(rig.power.releases(), 0);
    assert_eq!(rig.processes.list_calls(), 0);
    assert_eq!(
        state,
        json!({
            "leases": [{
                "key": "manual",
                "kind": "until_off",
                "harness": null,
                "label": "Until off",
                "pid": null,
                "stopped": false,
                "ends_at_ms": null
            }],
            "system_held": true,
            "display_held": false,
            "since_ms": ORIGIN_MS as i64,
            "problem": null
        })
    );
    assert_eq!(state, rig.kernel.state("awake").unwrap());

    let events = rig.kernel.recent_events(10).unwrap();
    let started: Vec<_> = events
        .iter()
        .filter(|event| event.kind == "awake.lease_started")
        .collect();
    assert_eq!(started.len(), 1);
    assert_eq!(started[0].module, "awake");
    assert_eq!(started[0].severity, Severity::Info);
    assert_eq!(started[0].summary, "Keeping this Mac awake: until off.");

    rig.advance(Duration::from_secs(3600)).await;
    rig.idle().await;
    let after_an_hour = rig.dispatch(json!("status")).await.unwrap();
    assert_eq!(lease_count(&after_an_hour), 1);
    assert_eq!(after_an_hour["leases"][0]["kind"], "until_off");
    assert_eq!(after_an_hour["leases"][0]["ends_at_ms"], Value::Null);
    assert_eq!(after_an_hour["system_held"], true);
    assert_eq!(rig.power.held().len(), 1);
    assert_eq!(rig.power.releases(), 0);

    rig.advance(Duration::from_secs(8 * 3600)).await;
    rig.idle().await;
    let after_any_hour = rig.dispatch(json!("status")).await.unwrap();
    assert_eq!(lease_count(&after_any_hour), 1);
    assert_eq!(after_any_hour["leases"][0]["ends_at_ms"], Value::Null);
    assert_eq!(after_any_hour["system_held"], true);
    assert_eq!(
        rig.power.held(),
        vec![(AssertionKind::System, ASSERTION_NAME.to_owned())]
    );
    assert_eq!(rig.power.releases(), 0);
    rig.shutdown().await;
}

fn hold(seconds: u64) -> Value {
    json!({ "hold_for": { "seconds": seconds } })
}

#[tokio::test(start_paused = true)]
async fn until_off_replaces_a_duration_lease() {
    let rig = Rig::new();
    let first = rig.dispatch(hold(3600)).await.unwrap();
    assert_eq!(lease_count(&first), 1);
    assert_eq!(first["leases"][0]["kind"], "duration");
    assert_eq!(rig.power.held().len(), 1);

    rig.advance(Duration::from_secs(10 * 60)).await;
    let second = rig.dispatch(until_off()).await.unwrap();

    assert_eq!(lease_count(&second), 1);
    assert_eq!(second["leases"][0]["key"], "manual");
    assert_eq!(second["leases"][0]["kind"], "until_off");
    assert_eq!(second["leases"][0]["ends_at_ms"], Value::Null);
    assert_eq!(
        second["since_ms"].as_i64().unwrap(),
        ORIGIN_MS as i64 + 600_000
    );
    assert_eq!(
        rig.power.held(),
        vec![(AssertionKind::System, ASSERTION_NAME.to_owned())]
    );
    assert_eq!(rig.power.releases(), 0);

    let events = rig.kernel.recent_events(10).unwrap();
    let started: Vec<_> = events
        .iter()
        .filter(|event| event.kind == "awake.lease_started")
        .collect();
    let ended: Vec<_> = events
        .iter()
        .filter(|event| event.kind == "awake.lease_ended")
        .collect();
    assert_eq!(started.len(), 2);
    assert_eq!(started[0].summary, "Keeping this Mac awake: until off.");
    assert_eq!(started[1].summary, "Keeping this Mac awake: 1 hour.");
    assert_eq!(ended.len(), 1);
    assert_eq!(ended[0].summary, "Stopped keeping this Mac awake: 1 hour.");
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn a_duration_request_replaces_an_until_off_lease() {
    let rig = Rig::new();
    let first = rig.dispatch(until_off()).await.unwrap();
    assert_eq!(lease_count(&first), 1);
    assert_eq!(first["leases"][0]["kind"], "until_off");
    assert_eq!(rig.power.held().len(), 1);

    rig.advance(Duration::from_secs(10 * 60)).await;
    let second = rig.dispatch(hold(3600)).await.unwrap();

    assert_eq!(lease_count(&second), 1);
    assert_eq!(second["leases"][0]["key"], "manual");
    assert_eq!(second["leases"][0]["kind"], "duration");
    assert_eq!(
        second["leases"][0]["ends_at_ms"].as_i64().unwrap(),
        ORIGIN_MS as i64 + 600_000 + 3_600_000
    );
    assert_eq!(
        second["since_ms"].as_i64().unwrap(),
        ORIGIN_MS as i64 + 600_000
    );
    assert_eq!(
        rig.power.held(),
        vec![(AssertionKind::System, ASSERTION_NAME.to_owned())]
    );
    assert_eq!(rig.power.releases(), 0);

    let events = rig.kernel.recent_events(10).unwrap();
    let started: Vec<_> = events
        .iter()
        .filter(|event| event.kind == "awake.lease_started")
        .collect();
    let ended: Vec<_> = events
        .iter()
        .filter(|event| event.kind == "awake.lease_ended")
        .collect();
    assert_eq!(started.len(), 2);
    assert_eq!(started[0].summary, "Keeping this Mac awake: 1 hour.");
    assert_eq!(started[1].summary, "Keeping this Mac awake: until off.");
    assert_eq!(ended.len(), 1);
    assert_eq!(
        ended[0].summary,
        "Stopped keeping this Mac awake: until off."
    );
    rig.shutdown().await;
}

fn release_all() -> Value {
    json!("release_all")
}

#[tokio::test(start_paused = true)]
async fn release_all_ends_every_lease_and_releases_the_assertion() {
    let rig = Rig::new();
    rig.dispatch(until_off()).await.unwrap();
    assert_eq!(rig.power.held().len(), 1);

    let state = rig.dispatch(release_all()).await.unwrap();
    assert_eq!(lease_count(&state), 0);
    assert_eq!(state["system_held"], false);
    assert_eq!(state["since_ms"], Value::Null);
    assert!(rig.power.held().is_empty());
    assert_eq!(rig.power.releases(), 1);
    assert_eq!(state, rig.kernel.state("awake").unwrap());

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

    let again = rig.dispatch(release_all()).await.unwrap();
    assert_eq!(lease_count(&again), 0);
    assert!(rig.power.held().is_empty());
    assert_eq!(rig.power.releases(), 1);
    let ended_again = rig
        .kernel
        .recent_events(10)
        .unwrap()
        .into_iter()
        .filter(|event| event.kind == "awake.lease_ended")
        .count();
    assert_eq!(ended_again, 1);
    rig.shutdown().await;
}

fn catalog_names() -> Vec<&'static str> {
    AwakeAction::catalog().iter().map(|doc| doc.name).collect()
}

#[test]
fn the_action_catalog_has_no_pause_and_no_sleep_action() {
    let names = catalog_names();
    assert!(
        names.contains(&"hold_until_off"),
        "until-off must be a catalog action: {names:?}"
    );
    for name in &names {
        assert!(
            !name.contains("pause"),
            "catalog must not offer a pause action: {name}"
        );
        assert!(
            !name.contains("sleep"),
            "catalog must not offer a sleep action: {name}"
        );
    }
}

#[test]
fn release_all_is_the_only_catalog_action_for_ending_manual_leases() {
    let names = catalog_names();
    assert!(
        names.contains(&"release_all"),
        "release_all must be a catalog action: {names:?}"
    );
    let not_ending_manual = [
        "status",
        "hold_for",
        "hold_until_off",
        "hold_command",
        "hold_session",
        "stop_session",
        "end_session",
    ];
    let ending: Vec<&str> = names
        .into_iter()
        .filter(|name| !not_ending_manual.contains(name))
        .collect();
    assert_eq!(ending, vec!["display", "release_all"]);
}

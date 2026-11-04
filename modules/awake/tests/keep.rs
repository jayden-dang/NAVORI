#![allow(clippy::unwrap_used)]

mod common;

use common::{ASSERTION_NAME, ORIGIN_MS, Rig};
use navori_macos::AssertionKind;
use serde_json::{Value, json};

fn keep_for(seconds: u64) -> Value {
    json!({ "keep_for": { "seconds": seconds } })
}

fn kinds(state: &Value) -> Vec<String> {
    state["leases"]
        .as_array()
        .unwrap()
        .iter()
        .map(|lease| lease["kind"].as_str().unwrap().to_owned())
        .collect()
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

#[tokio::test(start_paused = true)]
async fn each_menu_length_is_a_duration_lease_named_for_that_length() {
    let rig = Rig::new();
    for (seconds, label) in [
        (300, "5 minutes"),
        (600, "10 minutes"),
        (900, "15 minutes"),
        (1800, "30 minutes"),
        (3600, "1 hour"),
        (7200, "2 hours"),
        (18000, "5 hours"),
    ] {
        let state = rig.dispatch(keep_for(seconds)).await.unwrap();
        assert_eq!(kinds(&state), ["duration"]);
        assert_eq!(state["leases"][0]["label"], label);
        assert_eq!(
            state["leases"][0]["ends_at_ms"],
            i64::try_from(ORIGIN_MS).unwrap() + i64::try_from(seconds * 1000).unwrap()
        );
    }
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn indefinitely_holds_the_display_as_the_only_keep_awake_lease() {
    let rig = Rig::new();
    rig.dispatch(keep_for(300)).await.unwrap();
    let state = rig.dispatch(json!("keep_indefinitely")).await.unwrap();
    assert_eq!(kinds(&state), ["display"]);
    assert_eq!(state["display_held"], true);
    assert_eq!(rig.power.held(), system_and_display());
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn a_length_replaces_indefinitely_and_lets_the_display_go() {
    let rig = Rig::new();
    rig.dispatch(json!("keep_indefinitely")).await.unwrap();
    let state = rig.dispatch(keep_for(300)).await.unwrap();
    assert_eq!(kinds(&state), ["duration"]);
    assert_eq!(state["display_held"], false);
    assert_eq!(rig.power.held(), system_only());
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn replacing_a_choice_never_drops_the_system_assertion() {
    let rig = Rig::new();
    rig.dispatch(keep_for(300)).await.unwrap();
    rig.dispatch(json!("keep_indefinitely")).await.unwrap();
    assert_eq!(rig.power.releases(), 0);
    rig.dispatch(keep_for(600)).await.unwrap();
    // The one release is the Display assertion.
    assert_eq!(rig.power.releases(), 1);
    assert_eq!(rig.power.held(), system_only());
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn keep_off_ends_keep_awake_leases_and_no_other() {
    let rig = Rig::new();
    let me = i32::try_from(std::process::id()).unwrap();
    rig.processes.add(navori_macos::fake::FakeProcess::new(
        me,
        1,
        501,
        1,
        "/bin/cargo",
    ));
    rig.dispatch(json!({ "hold_command": { "pid": me } }))
        .await
        .unwrap();
    rig.dispatch(json!("hold_until_off")).await.unwrap();
    rig.dispatch(json!({ "display": { "on": true } }))
        .await
        .unwrap();
    let state = rig.dispatch(json!("keep_off")).await.unwrap();
    assert_eq!(kinds(&state), ["command"]);
    assert_eq!(rig.power.held(), system_only());
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn keep_off_with_nothing_held_changes_nothing() {
    let rig = Rig::new();
    let state = rig.dispatch(json!("keep_off")).await.unwrap();
    assert_eq!(kinds(&state), Vec::<String>::new());
    assert!(rig.power.held().is_empty());
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn hold_for_still_leaves_a_display_lease_alone() {
    let rig = Rig::new();
    rig.dispatch(json!({ "display": { "on": true } }))
        .await
        .unwrap();
    let state = rig
        .dispatch(json!({ "hold_for": { "seconds": 300 } }))
        .await
        .unwrap();
    assert_eq!(kinds(&state), ["duration", "display"]);
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn keep_for_rejects_a_length_past_seven_days() {
    let rig = Rig::new();
    rig.dispatch(json!("keep_indefinitely")).await.unwrap();
    assert!(rig.dispatch(keep_for(0)).await.is_err());
    assert_eq!(
        kinds(&rig.kernel.state("awake").unwrap()),
        ["display"],
        "a refused length changes nothing"
    );
    rig.shutdown().await;
}

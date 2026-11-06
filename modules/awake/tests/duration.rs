mod common;

use std::time::Duration;

use common::{ASSERTION_NAME, ORIGIN_MS, Rig};
use navori_kernel::{ActionError, DispatchError, Severity};
use navori_macos::AssertionKind;
use serde_json::{Value, json};

const HOUR_MS: i64 = 3_600_000;
const SEVEN_DAYS: u64 = 7 * 24 * 60 * 60;

fn hold(seconds: u64) -> Value {
    json!({ "hold_for": { "seconds": seconds } })
}

fn lease_count(state: &Value) -> usize {
    state["leases"].as_array().map(Vec::len).unwrap_or(0)
}

#[tokio::test(start_paused = true)]
async fn a_one_hour_lease_holds_one_system_assertion_with_the_given_name() {
    let rig = Rig::new();
    let state = rig.dispatch(hold(3600)).await.unwrap();

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
                "kind": "duration",
                "harness": null,
                "label": "1 hour",
                "pid": null,
                "stopped": false,
                "ends_at_ms": ORIGIN_MS as i64 + HOUR_MS,
                "title": null
            }],
            "system_held": true,
            "display_held": false,
            "since_ms": ORIGIN_MS as i64,
            "problem": null,
            "tracking": true,
            "lid": false,
            "lid_held": false,
            "on_adapter": null,
            "lid_marker": false,
            "last_sleep_ms": null,
            "lid_problem": null
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
    assert_eq!(started[0].summary, "Keeping this Mac awake: 1 hour.");
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn a_ten_second_hold_starts_as_ten_seconds_not_one_hour() {
    let rig = Rig::new();
    rig.dispatch(hold(10)).await.unwrap();

    let events = rig.kernel.recent_events(10).unwrap();
    let started: Vec<_> = events
        .iter()
        .filter(|event| event.kind == "awake.lease_started")
        .collect();
    assert_eq!(started.len(), 1);
    assert_eq!(started[0].module, "awake");
    assert_eq!(started[0].severity, Severity::Info);
    assert!(
        !started[0].summary.contains("1 hour"),
        "{}",
        started[0].summary
    );
    assert!(
        started[0].summary.contains("10 seconds"),
        "{}",
        started[0].summary
    );
    assert_eq!(started[0].summary, "Keeping this Mac awake: 10 seconds.");
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn a_duration_lease_label_is_the_plain_english_length() {
    let cases = [
        (30 * 60, "30 minutes"),
        (90, "90 seconds"),
        (10, "10 seconds"),
        (2 * 3_600, "2 hours"),
        (86_400, "1 day"),
        (SEVEN_DAYS, "7 days"),
        (1, "1 second"),
        (3_600, "1 hour"),
    ];
    for (seconds, label) in cases {
        let rig = Rig::new();
        let state = rig.dispatch(hold(seconds)).await.unwrap();
        assert_eq!(
            state["leases"][0]["label"],
            json!(label),
            "{seconds} seconds published the wrong label"
        );
        assert_ne!(state["leases"][0]["label"], "Manual");
        let summary = rig
            .kernel
            .recent_events(10)
            .unwrap()
            .into_iter()
            .find(|event| event.kind == "awake.lease_started")
            .map(|event| event.summary)
            .unwrap_or_default();
        assert_eq!(summary, format!("Keeping this Mac awake: {label}."));
        rig.shutdown().await;
    }
}

#[tokio::test(start_paused = true)]
async fn two_duration_requests_keep_one_lease_and_restart_the_hour() {
    let rig = Rig::new();
    let first = rig.dispatch(hold(3600)).await.unwrap();
    assert_eq!(
        first["leases"][0]["ends_at_ms"].as_i64().unwrap(),
        ORIGIN_MS as i64 + HOUR_MS
    );

    rig.advance(Duration::from_secs(10 * 60)).await;
    let second = rig.dispatch(hold(3600)).await.unwrap();

    assert_eq!(lease_count(&second), 1);
    assert_eq!(second["leases"][0]["key"], "manual");
    assert_eq!(second["leases"][0]["kind"], "duration");
    assert_eq!(
        second["leases"][0]["ends_at_ms"].as_i64().unwrap(),
        ORIGIN_MS as i64 + 600_000 + HOUR_MS
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
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn replacing_a_duration_lease_emits_a_stop_and_another_start() {
    let rig = Rig::new();
    rig.dispatch(hold(3600)).await.unwrap();
    rig.advance(Duration::from_secs(10 * 60)).await;
    rig.dispatch(hold(3600)).await.unwrap();

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
    assert_eq!(started[1].summary, "Keeping this Mac awake: 1 hour.");
    assert_eq!(ended.len(), 1);
    assert_eq!(ended[0].summary, "Stopped keeping this Mac awake: 1 hour.");
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn the_lease_ends_at_its_deadline_and_releases_the_assertion() {
    let rig = Rig::new();
    rig.dispatch(hold(3600)).await.unwrap();

    rig.advance(Duration::from_secs(3599)).await;
    rig.idle().await;
    assert_eq!(lease_count(&rig.kernel.state("awake").unwrap()), 1);
    assert_eq!(rig.power.held().len(), 1);

    rig.advance(Duration::from_secs(1)).await;
    rig.idle().await;
    let state = rig.kernel.state("awake").unwrap();
    assert_eq!(lease_count(&state), 0);
    assert_eq!(state["system_held"], false);
    assert!(rig.power.held().is_empty());
    assert_eq!(rig.power.releases(), 1);

    let events = rig.kernel.recent_events(10).unwrap();
    let ended = events
        .iter()
        .find(|event| event.kind == "awake.lease_ended")
        .unwrap();
    assert_eq!(ended.module, "awake");
    assert_eq!(ended.severity, Severity::Info);
    assert_eq!(
        ended.summary,
        "Stopped keeping this Mac awake: 1 hour reached its end time."
    );
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn with_the_menu_closed_a_lease_ends_within_30_seconds_of_its_deadline() {
    let rig = Rig::new();
    rig.dispatch(hold(10)).await.unwrap();
    rig.advance(Duration::from_secs(10)).await;
    rig.idle().await;
    assert_eq!(lease_count(&rig.kernel.state("awake").unwrap()), 0);

    let again = rig.dispatch(hold(10)).await.unwrap();
    let deadline = again["leases"][0]["ends_at_ms"].as_i64().unwrap();
    assert_eq!(deadline, ORIGIN_MS as i64 + 20_000);

    rig.advance(Duration::from_secs(10)).await;
    rig.idle().await;
    assert_eq!(
        lease_count(&rig.kernel.state("awake").unwrap()),
        1,
        "a closed menu ended the lease before its deadline"
    );
    assert_eq!(rig.power.held().len(), 1);

    let mut ended_at = None;
    for _ in 0..30 {
        rig.advance(Duration::from_secs(1)).await;
        rig.idle().await;
        if lease_count(&rig.kernel.state("awake").unwrap()) == 0 {
            ended_at = Some(rig.clock.millis());
            break;
        }
    }
    let ended_at = ended_at.expect("the lease did not end within 30 seconds of its deadline");
    assert!(
        ended_at >= deadline,
        "ended at {ended_at}, deadline {deadline}"
    );
    assert!(
        ended_at - deadline <= 30_000,
        "ended at {ended_at}, deadline {deadline}"
    );
    let state = rig.kernel.state("awake").unwrap();
    assert_eq!(lease_count(&state), 0);
    assert_eq!(state["system_held"], false);
    assert!(rig.power.held().is_empty());
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn sleeping_past_the_deadline_ends_the_lease_on_wake() {
    let rig = Rig::new();
    rig.dispatch(hold(3600)).await.unwrap();

    rig.clock.advance(Duration::from_secs(3600));
    rig.idle().await;
    assert_eq!(lease_count(&rig.kernel.state("awake").unwrap()), 1);
    assert_eq!(rig.power.held().len(), 1);

    rig.kernel.scheduler().wake_pulse();
    rig.idle().await;
    let state = rig.kernel.state("awake").unwrap();
    assert_eq!(lease_count(&state), 0);
    assert_eq!(state["system_held"], false);
    assert!(rig.power.held().is_empty());
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn sleep_time_is_subtracted_from_the_duration() {
    let rig = Rig::new();
    let started = rig.dispatch(hold(3600)).await.unwrap();
    let deadline = started["leases"][0]["ends_at_ms"].as_i64().unwrap();
    assert_eq!(deadline, ORIGIN_MS as i64 + HOUR_MS);

    rig.clock.advance(Duration::from_secs(15 * 60));
    let during = rig.dispatch(json!("status")).await.unwrap();
    assert_eq!(
        during["leases"][0]["ends_at_ms"].as_i64().unwrap(),
        deadline
    );
    assert_eq!(during["system_held"], true);
    assert_eq!(rig.power.held().len(), 1);

    rig.advance(Duration::from_secs(44 * 60)).await;
    rig.idle().await;
    assert_eq!(
        rig.kernel.state("awake").unwrap()["leases"][0]["ends_at_ms"]
            .as_i64()
            .unwrap(),
        deadline
    );
    assert_eq!(rig.power.held().len(), 1);

    rig.advance(Duration::from_secs(60)).await;
    rig.idle().await;
    let state = rig.kernel.state("awake").unwrap();
    assert_eq!(lease_count(&state), 0);
    assert_eq!(state["system_held"], false);
    assert!(rig.power.held().is_empty());
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn a_zero_or_over_7_day_duration_is_invalid() {
    let rig = Rig::new();
    let zero = rig.dispatch(hold(0)).await.unwrap_err();
    assert!(matches!(
        zero,
        DispatchError::Action(ActionError::Invalid(_))
    ));
    let over = rig.dispatch(hold(SEVEN_DAYS + 1)).await.unwrap_err();
    assert!(matches!(
        over,
        DispatchError::Action(ActionError::Invalid(_))
    ));
    assert!(rig.power.held().is_empty());
    assert_eq!(lease_count(&rig.kernel.state("awake").unwrap()), 0);

    let max = rig.dispatch(hold(SEVEN_DAYS)).await.unwrap();
    assert_eq!(lease_count(&max), 1);
    assert_eq!(
        max["leases"][0]["ends_at_ms"].as_i64().unwrap(),
        ORIGIN_MS as i64 + i64::try_from(SEVEN_DAYS).unwrap() * 1000
    );
    assert_eq!(rig.power.held().len(), 1);
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn status_answers_the_state() {
    let rig = Rig::new();
    let idle = rig.dispatch(json!("status")).await.unwrap();
    assert_eq!(idle, rig.kernel.state("awake").unwrap());
    assert_eq!(
        idle,
        json!({
            "leases": [],
            "system_held": false,
            "display_held": false,
            "since_ms": null,
            "problem": null,
            "tracking": true,
            "lid": false,
            "lid_held": false,
            "on_adapter": null,
            "lid_marker": false,
            "last_sleep_ms": null,
            "lid_problem": null
        })
    );

    let held = rig.dispatch(hold(3600)).await.unwrap();
    assert_eq!(held, rig.kernel.state("awake").unwrap());
    assert_eq!(held["system_held"], true);
    assert_eq!(lease_count(&held), 1);
    assert_eq!(held["leases"][0]["kind"], "duration");

    let again = rig.dispatch(json!("status")).await.unwrap();
    assert_eq!(again, held);
    assert_eq!(rig.power.releases(), 0);
    rig.shutdown().await;
}

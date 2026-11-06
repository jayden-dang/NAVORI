#![allow(clippy::unwrap_used)]

mod common;

use common::Rig;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use common::ASSERTION_NAME;
use navori_kernel::{ActionError, DispatchError, ModuleState, RestartPolicy};
use navori_macos::fake::{FakeProcess, LidCall};
use navori_macos::{AssertionKind, PowerKind};
use serde_json::{Value, json};

fn lid(on: bool) -> Value {
    json!({ "lid": { "on": on } })
}

fn state(rig: &Rig) -> Value {
    rig.kernel.state("awake").unwrap()
}

fn stored(rig: &Rig, key: &str) -> Option<Value> {
    rig.store.kv_get("awake", key).unwrap()
}

fn events(rig: &Rig, kind: &str) -> Vec<(String, Value)> {
    rig.kernel
        .recent_events(20)
        .unwrap()
        .into_iter()
        .filter(|event| event.kind == kind)
        .map(|event| (event.summary, event.payload))
        .collect()
}

fn fail_writes_of(rig: &Rig, key: &str) {
    rig.store.with_conn(|conn| {
        conn.execute_batch(&format!(
            "CREATE TRIGGER refuse_{key} BEFORE INSERT ON kv WHEN NEW.key = '{key}' \
             BEGIN SELECT RAISE(ABORT, 'refused'); END;"
        ))
        .unwrap();
    });
}

#[tokio::test(start_paused = true)]
async fn lid_closed_is_off_on_a_fresh_store() {
    let rig = Rig::new();

    let answer = rig.dispatch(json!("status")).await.unwrap();

    assert_eq!(answer["lid"], false);
    assert_eq!(answer["lid_problem"], Value::Null);
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn a_stored_non_boolean_reads_as_off() {
    let mut rig = Rig::new();
    rig.store.kv_set("awake", "lid", &json!("yes")).unwrap();

    rig.reopen().await;

    assert_eq!(state(&rig)["lid"], false);
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn turning_lid_closed_on_stores_it_and_journals_the_change() {
    let rig = Rig::new();

    let answer = rig.dispatch(lid(true)).await.unwrap();

    assert_eq!(answer["lid"], true);
    assert_eq!(stored(&rig, "lid"), Some(json!(true)));
    assert_eq!(
        events(&rig, "awake.lid_changed"),
        [(
            "Lid closed is on: the lid may close without sleep while a Lease holds on the adapter."
                .to_owned(),
            json!({ "on": true })
        )]
    );

    rig.dispatch(lid(false)).await.unwrap();

    assert_eq!(stored(&rig, "lid"), Some(json!(false)));
    assert_eq!(
        events(&rig, "awake.lid_changed")[0],
        (
            "Lid closed is off: closing the lid sleeps this Mac.".to_owned(),
            json!({ "on": false })
        )
    );
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn asking_for_the_current_value_changes_nothing() {
    let rig = Rig::new();
    fail_writes_of(&rig, "lid");

    let answer = rig.dispatch(lid(false)).await.unwrap();

    assert_eq!(answer["lid"], false);
    assert_eq!(stored(&rig, "lid"), None);
    assert_eq!(events(&rig, "awake.lid_changed"), []);
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn a_failed_store_leaves_lid_closed_unchanged_and_fails_the_action() {
    let rig = Rig::new();
    fail_writes_of(&rig, "lid");

    let err = rig.dispatch(lid(true)).await.unwrap_err();

    assert!(matches!(err, DispatchError::Action(ActionError::Failed(_))));
    assert_eq!(state(&rig)["lid"], false);
    assert_eq!(events(&rig, "awake.lid_changed"), []);
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn the_stored_value_is_applied_after_a_relaunch() {
    let mut rig = Rig::new();
    rig.dispatch(lid(true)).await.unwrap();

    rig.reopen().await;

    assert_eq!(state(&rig)["lid"], true);
    assert_eq!(state(&rig)["lid_problem"], Value::Null);
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn an_unreadable_lid_value_reads_as_off_and_raises_a_lid_problem() {
    let mut rig = Rig::new();
    rig.store.with_conn(|conn| {
        conn.execute(
            "INSERT INTO kv (ns, key, value) VALUES ('awake', 'lid', '{broken')",
            [],
        )
        .unwrap();
    });

    rig.reopen().await;

    let state = state(&rig);
    assert_eq!(state["lid"], false);
    assert_eq!(
        state["lid_problem"],
        "Awake could not read the Lid closed setting, so it reads as off."
    );
    assert_eq!(state["problem"], Value::Null);
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn setting_tracking_still_stores_first_and_journals_tracking_changed() {
    let rig = Rig::new();

    rig.dispatch(json!({ "tracking": { "on": false } }))
        .await
        .unwrap();

    assert_eq!(stored(&rig, "tracking"), Some(json!(false)));
    assert_eq!(
        events(&rig, "awake.tracking_changed"),
        [(
            "Tracking agents is off: agent sessions no longer keep this Mac awake.".to_owned(),
            json!({ "on": false })
        )]
    );

    fail_writes_of(&rig, "tracking");
    let err = rig
        .dispatch(json!({ "tracking": { "on": true } }))
        .await
        .unwrap_err();
    assert!(matches!(err, DispatchError::Action(ActionError::Failed(_))));
    assert_eq!(state(&rig)["tracking"], false);
    rig.shutdown().await;
}

fn adapter_rig() -> Rig {
    let rig = Rig::new();
    rig.power_source.set(PowerKind::Adapter);
    rig
}

#[tokio::test(start_paused = true)]
async fn nothing_reads_the_power_source_while_lid_closed_is_off() {
    let rig = adapter_rig();
    rig.dispatch(json!("status")).await.unwrap();
    rig.idle().await;

    assert_eq!(rig.power_source.read_count(), 0);
    assert_eq!(rig.power_source.watch_count(), 0);
    assert_eq!(state(&rig)["on_adapter"], Value::Null);
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn turning_lid_closed_on_watches_first_then_reads_once() {
    let rig = adapter_rig();

    let answer = rig.dispatch(lid(true)).await.unwrap();

    assert_eq!(rig.power_source.watch_count(), 1);
    assert_eq!(rig.power_source.read_count(), 1);
    assert_eq!(answer["on_adapter"], true);
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn a_power_change_reads_the_source_once_and_publishes_only_a_real_change() {
    let rig = adapter_rig();
    rig.dispatch(lid(true)).await.unwrap();

    rig.power_source.fire();
    rig.idle().await;

    assert_eq!(rig.power_source.read_count(), 2);
    assert_eq!(state(&rig)["on_adapter"], true);
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn unplugging_publishes_not_on_the_adapter() {
    let rig = adapter_rig();
    rig.dispatch(lid(true)).await.unwrap();

    rig.power_source.set(PowerKind::Battery);
    rig.power_source.fire();
    rig.idle().await;

    assert_eq!(state(&rig)["on_adapter"], false);
    assert_eq!(state(&rig)["lid_problem"], Value::Null);
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn a_dead_watch_reads_as_battery_raises_a_lid_problem_and_retries() {
    let rig = adapter_rig();
    rig.dispatch(lid(true)).await.unwrap();

    rig.power_source.kill();
    rig.idle().await;

    assert_eq!(state(&rig)["on_adapter"], false);
    assert_ne!(state(&rig)["lid_problem"], Value::Null);
    assert_eq!(rig.power_source.watch_count(), 1);

    rig.advance(Duration::from_secs(60)).await;
    rig.idle().await;

    assert_eq!(rig.power_source.watch_count(), 2);
    assert_eq!(state(&rig)["on_adapter"], true);
    assert_eq!(state(&rig)["lid_problem"], Value::Null);
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn a_failed_watch_at_arming_reads_as_battery_and_retries() {
    let rig = adapter_rig();
    rig.power_source.fail_watch();

    let answer = rig.dispatch(lid(true)).await.unwrap();

    assert_eq!(answer["on_adapter"], false);
    assert_ne!(answer["lid_problem"], Value::Null);
    assert_eq!(rig.power_source.read_count(), 0);

    rig.advance(Duration::from_secs(60)).await;
    rig.idle().await;

    assert_eq!(rig.power_source.watch_count(), 2);
    assert_eq!(state(&rig)["on_adapter"], false);
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn turning_lid_closed_off_drops_the_watch() {
    let rig = adapter_rig();
    rig.dispatch(lid(true)).await.unwrap();

    let answer = rig.dispatch(lid(false)).await.unwrap();
    rig.power_source.fire();
    rig.idle().await;

    assert_eq!(answer["on_adapter"], Value::Null);
    assert_eq!(rig.power_source.read_count(), 1);
    assert_eq!(rig.power_source.drain_count(), 0);
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn lid_closed_stored_on_is_watched_after_a_relaunch() {
    let mut rig = adapter_rig();
    rig.dispatch(lid(true)).await.unwrap();

    rig.reopen().await;

    assert_eq!(rig.power_source.watch_count(), 2);
    assert_eq!(state(&rig)["on_adapter"], true);
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn an_idle_watch_does_not_spin() {
    let rig = adapter_rig();
    rig.dispatch(lid(true)).await.unwrap();

    for _ in 0..10 {
        rig.idle().await;
    }

    assert!(rig.power_source.drain_count() <= 1);
    rig.shutdown().await;
}

const POWER_PROBLEM: &str = "Awake could not watch the power source, so it reads as on battery. Navori retries in 60 seconds.";
const SYSTEM_PROBLEM: &str = "macOS refused the request. Navori keeps trying.";

fn hold(seconds: u64) -> Value {
    json!({ "hold_for": { "seconds": seconds } })
}

fn session_rig() -> Rig {
    let rig = adapter_rig();
    rig.processes.add(FakeProcess::new(
        100,
        1,
        501,
        100_000,
        "/usr/local/bin/claude",
    ));
    rig
}

fn hold_session() -> Value {
    json!({ "hold_session": { "session_id": "sess-1", "pid": 100, "ppid": 1 } })
}

type Calls = Arc<Mutex<Vec<(LidCall, bool)>>>;

fn watch_marker(rig: &Rig) -> Calls {
    let calls = Calls::default();
    let store = rig.store.clone();
    let seen = Arc::clone(&calls);
    rig.lid.on_call(Box::new(move |call| {
        let marker = store.kv_get("awake", "lid_held").unwrap().is_some();
        seen.lock().unwrap().push((call, marker));
    }));
    calls
}

fn log(calls: &Calls) -> Vec<(LidCall, bool)> {
    calls.lock().unwrap().clone()
}

async fn lid_on_with_a_lease(rig: &Rig) {
    rig.dispatch(lid(true)).await.unwrap();
    rig.dispatch(hold(600)).await.unwrap();
}

#[tokio::test(start_paused = true)]
async fn a_held_lease_on_the_adapter_with_lid_closed_on_sets_the_lid() {
    let rig = adapter_rig();

    lid_on_with_a_lease(&rig).await;

    assert!(rig.lid.is_set());
    assert_eq!(state(&rig)["lid_held"], true);
    assert_eq!(state(&rig)["lid_marker"], true);
    assert_eq!(stored(&rig, "lid_held"), Some(json!(true)));
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn the_marker_is_stored_before_the_lid_is_set() {
    let rig = adapter_rig();
    let calls = watch_marker(&rig);

    lid_on_with_a_lease(&rig).await;

    assert_eq!(log(&calls), [(LidCall::Set, true)]);
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn the_marker_is_deleted_only_after_the_lid_is_cleared() {
    let rig = adapter_rig();
    let calls = watch_marker(&rig);
    lid_on_with_a_lease(&rig).await;

    rig.dispatch(json!("release_all")).await.unwrap();

    assert_eq!(log(&calls), [(LidCall::Set, true), (LidCall::Clear, true)]);
    assert_eq!(stored(&rig, "lid_held"), None);
    assert_eq!(state(&rig)["lid_marker"], false);
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn the_last_lease_ending_gives_the_lid_back() {
    let rig = adapter_rig();
    rig.dispatch(lid(true)).await.unwrap();
    rig.dispatch(hold(60)).await.unwrap();
    assert!(rig.lid.is_set());

    rig.advance(Duration::from_secs(61)).await;
    rig.idle().await;

    assert!(!rig.lid.is_set());
    assert_eq!(state(&rig)["lid_held"], false);
    assert_eq!(stored(&rig, "lid_held"), None);
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn turning_lid_closed_off_gives_the_lid_back() {
    let rig = adapter_rig();
    lid_on_with_a_lease(&rig).await;

    let answer = rig.dispatch(lid(false)).await.unwrap();

    assert!(!rig.lid.is_set());
    assert_eq!(answer["lid_held"], false);
    assert_eq!(stored(&rig, "lid_held"), None);
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn with_tracking_agents_off_a_session_lease_alone_does_not_hold_the_lid() {
    let rig = session_rig();
    rig.dispatch(json!({ "tracking": { "on": false } }))
        .await
        .unwrap();
    rig.dispatch(lid(true)).await.unwrap();

    rig.dispatch(hold_session()).await.unwrap();

    assert!(!rig.lid.is_set());
    assert_eq!(rig.lid.calls(), []);
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn a_session_lease_holds_the_lid_with_tracking_agents_on() {
    let rig = session_rig();
    rig.dispatch(lid(true)).await.unwrap();

    rig.dispatch(hold_session()).await.unwrap();

    assert!(rig.lid.is_set());
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn a_manual_lease_holds_the_lid_with_tracking_agents_off() {
    let rig = adapter_rig();
    rig.dispatch(json!({ "tracking": { "on": false } }))
        .await
        .unwrap();

    lid_on_with_a_lease(&rig).await;

    assert!(rig.lid.is_set());
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn unplugging_gives_the_lid_back_and_plugging_in_holds_it_again() {
    let rig = adapter_rig();
    lid_on_with_a_lease(&rig).await;

    rig.power_source.set(PowerKind::Battery);
    rig.power_source.fire();
    rig.idle().await;
    assert!(!rig.lid.is_set());
    assert_eq!(stored(&rig, "lid_held"), None);

    rig.power_source.set(PowerKind::Adapter);
    rig.power_source.fire();
    rig.idle().await;
    assert!(rig.lid.is_set());
    assert_eq!(state(&rig)["lid_held"], true);
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn a_failed_marker_store_sets_nothing_and_raises_a_lid_problem() {
    let rig = adapter_rig();
    fail_writes_of(&rig, "lid_held");

    lid_on_with_a_lease(&rig).await;

    assert_eq!(rig.lid.calls(), []);
    assert_eq!(state(&rig)["lid_held"], false);
    assert_ne!(state(&rig)["lid_problem"], Value::Null);
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn a_refused_set_turns_lid_closed_off_stores_it_deletes_the_marker_and_does_not_retry() {
    let rig = adapter_rig();
    rig.lid.refuse_set(5);

    lid_on_with_a_lease(&rig).await;

    let now = state(&rig);
    assert_eq!(now["lid"], false);
    assert_eq!(now["lid_held"], false);
    assert_ne!(now["lid_problem"], Value::Null);
    assert_eq!(stored(&rig, "lid"), Some(json!(false)));
    assert_eq!(stored(&rig, "lid_held"), None);
    assert_eq!(
        events(&rig, "awake.lid_changed")[0],
        (
            "Lid closed is off: closing the lid sleeps this Mac.".to_owned(),
            json!({ "on": false })
        )
    );

    rig.advance(Duration::from_secs(180)).await;
    rig.idle().await;

    assert_eq!(rig.lid.calls(), [LidCall::Set]);
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn a_refused_clear_keeps_the_marker_raises_a_lid_problem_and_retries_until_cleared() {
    let rig = adapter_rig();
    lid_on_with_a_lease(&rig).await;
    rig.lid.refuse_clear(5);

    rig.dispatch(json!("release_all")).await.unwrap();

    assert!(rig.lid.is_set());
    assert_eq!(stored(&rig, "lid_held"), Some(json!(true)));
    assert_eq!(state(&rig)["lid_marker"], true);
    assert_eq!(state(&rig)["lid_held"], false);
    assert_ne!(state(&rig)["lid_problem"], Value::Null);

    rig.advance(Duration::from_secs(60)).await;
    rig.idle().await;
    assert_eq!(
        rig.lid.calls(),
        [LidCall::Set, LidCall::Clear, LidCall::Clear]
    );

    rig.lid.allow();
    rig.advance(Duration::from_secs(60)).await;
    rig.idle().await;

    assert!(!rig.lid.is_set());
    assert_eq!(stored(&rig, "lid_held"), None);
    assert_eq!(state(&rig)["lid_marker"], false);
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn the_lid_problem_clears_when_the_lid_call_succeeds() {
    let rig = adapter_rig();
    lid_on_with_a_lease(&rig).await;
    rig.lid.refuse_clear(5);
    rig.dispatch(json!("release_all")).await.unwrap();
    assert_ne!(state(&rig)["lid_problem"], Value::Null);

    rig.lid.allow();
    rig.advance(Duration::from_secs(60)).await;
    rig.idle().await;

    assert_eq!(state(&rig)["lid_problem"], Value::Null);
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn the_lid_problem_clears_when_the_marker_saves_again() {
    let rig = adapter_rig();
    fail_writes_of(&rig, "lid_held");
    lid_on_with_a_lease(&rig).await;
    assert_ne!(state(&rig)["lid_problem"], Value::Null);

    rig.store.with_conn(|conn| {
        conn.execute_batch("DROP TRIGGER refuse_lid_held;").unwrap();
    });
    rig.advance(Duration::from_secs(60)).await;
    rig.idle().await;

    assert_eq!(state(&rig)["lid_problem"], Value::Null);
    assert!(rig.lid.is_set());
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn the_lid_problem_clears_when_the_user_sets_lid_closed() {
    let rig = adapter_rig();
    fail_writes_of(&rig, "lid_held");
    lid_on_with_a_lease(&rig).await;
    assert_ne!(state(&rig)["lid_problem"], Value::Null);

    let answer = rig.dispatch(lid(false)).await.unwrap();

    assert_eq!(answer["lid_problem"], Value::Null);
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn a_dead_watch_while_holding_gives_the_lid_back_and_raises_a_lid_problem() {
    let rig = adapter_rig();
    let calls = watch_marker(&rig);
    lid_on_with_a_lease(&rig).await;
    assert!(rig.lid.is_set());

    rig.power_source.kill();
    rig.idle().await;

    assert!(!rig.lid.is_set());
    assert_eq!(log(&calls), [(LidCall::Set, true), (LidCall::Clear, true)]);
    assert_eq!(stored(&rig, "lid_held"), None);
    assert_eq!(state(&rig)["lid_problem"], POWER_PROBLEM);
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn the_lid_problem_never_becomes_the_awake_problem() {
    let rig = adapter_rig();
    lid_on_with_a_lease(&rig).await;
    rig.lid.refuse_clear(5);

    rig.dispatch(json!("release_all")).await.unwrap();

    assert_ne!(state(&rig)["lid_problem"], Value::Null);
    assert_eq!(state(&rig)["problem"], Value::Null);
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn the_system_assertion_is_held_whatever_the_lid() {
    let rig = adapter_rig();
    rig.lid.refuse_set(5);
    lid_on_with_a_lease(&rig).await;

    assert_eq!(
        rig.power.held(),
        vec![(AssertionKind::System, ASSERTION_NAME.to_owned())]
    );
    assert_eq!(state(&rig)["system_held"], true);
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn a_refused_system_assertion_still_refuses_every_kind() {
    let rig = adapter_rig();
    rig.power.refuse(4);

    lid_on_with_a_lease(&rig).await;

    assert_eq!(state(&rig)["system_held"], false);
    assert_eq!(state(&rig)["problem"], SYSTEM_PROBLEM);
    assert!(rig.power.held().is_empty());
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn lease_events_keep_their_key_and_kind_payload() {
    let rig = adapter_rig();
    rig.dispatch(lid(true)).await.unwrap();

    rig.dispatch(json!("hold_until_off")).await.unwrap();

    assert_eq!(
        events(&rig, "awake.lease_started")
            .into_iter()
            .map(|(_, payload)| payload)
            .collect::<Vec<_>>(),
        [json!({ "key": "manual", "kind": "until_off" })]
    );
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn a_clean_shutdown_gives_the_lid_back_then_deletes_the_marker() {
    let rig = adapter_rig();
    lid_on_with_a_lease(&rig).await;
    let calls = watch_marker(&rig);

    rig.shutdown().await;

    assert!(!rig.lid.is_set());
    assert_eq!(log(&calls), [(LidCall::Clear, true)]);
    assert_eq!(stored(&rig, "lid_held"), None);
}

#[tokio::test(start_paused = true)]
async fn a_module_that_fails_permanently_gives_the_lid_back_and_keeps_the_marker() {
    let rig = Rig::with_policy(RestartPolicy {
        max_restarts: 0,
        base_delay: Duration::from_millis(10),
        max_delay: Duration::from_millis(10),
    });
    rig.power_source.set(navori_macos::PowerKind::Adapter);
    lid_on_with_a_lease(&rig).await;
    assert!(rig.lid.is_set());

    rig.power.panic_on_hold(1);
    let error = rig
        .dispatch(json!({ "display": { "on": true } }))
        .await
        .unwrap_err();
    assert_eq!(error, DispatchError::Unavailable("awake".into()));

    for _ in 0..30 {
        let failed = rig.kernel.modules().into_iter().any(|module| {
            module.id == "awake" && matches!(module.state, ModuleState::Failed { .. })
        });
        if failed {
            break;
        }
        rig.idle().await;
    }

    assert!(!rig.lid.is_set());
    assert_eq!(stored(&rig, "lid_held"), Some(json!(true)));
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn dropping_the_kernel_while_holding_gives_the_lid_back() {
    let rig = adapter_rig();
    lid_on_with_a_lease(&rig).await;
    assert!(rig.lid.is_set());
    let lid = rig.lid.clone();

    drop(rig);
    tokio::task::yield_now().await;

    assert!(!lid.is_set());
}

#[tokio::test(start_paused = true)]
async fn a_wake_rereads_the_power_source_while_lid_closed_is_on() {
    let rig = adapter_rig();
    rig.dispatch(lid(true)).await.unwrap();
    let before = rig.power_source.read_count();

    rig.dispatch(json!("woke")).await.unwrap();

    assert_eq!(rig.power_source.read_count(), before + 1);
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn an_unplug_during_sleep_is_seen_at_wake_and_gives_the_lid_back() {
    let rig = adapter_rig();
    rig.processes.add(FakeProcess::new(
        100,
        1,
        501,
        100_000,
        "/usr/local/bin/claude",
    ));
    rig.dispatch(lid(true)).await.unwrap();
    rig.dispatch(json!({ "hold_session": { "session_id": "sess-1", "pid": 100, "ppid": 1 } }))
        .await
        .unwrap();
    assert_eq!(rig.lid.calls(), [LidCall::Set]);

    rig.power_source.set(PowerKind::Battery);
    rig.dispatch(json!("woke")).await.unwrap();

    assert_eq!(rig.lid.calls(), [LidCall::Set, LidCall::Clear]);
    assert_eq!(state(&rig)["on_adapter"], false);
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn a_wake_while_lid_closed_is_off_reads_nothing() {
    let rig = adapter_rig();

    rig.dispatch(json!("woke")).await.unwrap();

    assert_eq!(rig.power_source.read_count(), 0);
    assert_eq!(state(&rig)["on_adapter"], Value::Null);
    rig.shutdown().await;
}

const CLEAR_PROBLEM: &str =
    "Awake could not give the lid setting back. Navori retries in 60 seconds.";

#[tokio::test(start_paused = true)]
async fn a_failed_last_sleep_save_does_not_hide_a_pending_clear_failure() {
    let rig = adapter_rig();
    lid_on_with_a_lease(&rig).await;
    rig.lid.refuse_clear(5);
    rig.dispatch(json!("release_all")).await.unwrap();
    fail_writes_of(&rig, "last_sleep");

    rig.dispatch(json!({ "slept": { "at_ms": 1_793_931_600_000_i64 } }))
        .await
        .unwrap();

    assert!(rig.lid.is_set());
    assert_eq!(state(&rig)["lid_problem"], CLEAR_PROBLEM);
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn a_power_watch_that_arms_again_does_not_clear_a_pending_clear_failure() {
    let rig = adapter_rig();
    lid_on_with_a_lease(&rig).await;
    rig.lid.refuse_clear(5);
    rig.dispatch(json!("release_all")).await.unwrap();
    rig.power_source.kill();
    rig.idle().await;
    assert_eq!(state(&rig)["lid_problem"], CLEAR_PROBLEM);

    rig.advance(Duration::from_secs(60)).await;
    rig.idle().await;

    assert_eq!(rig.power_source.watch_count(), 2);
    assert!(rig.lid.is_set());
    assert_eq!(state(&rig)["lid_problem"], CLEAR_PROBLEM);
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn a_refused_set_whose_off_value_cannot_be_saved_raises_a_problem_and_deletes_the_marker() {
    let rig = adapter_rig();
    rig.lid.refuse_set(5);
    rig.dispatch(lid(true)).await.unwrap();
    fail_writes_of(&rig, "lid");

    rig.dispatch(hold(600)).await.unwrap();

    let now = state(&rig);
    assert_eq!(now["lid"], false);
    assert_eq!(
        now["lid_problem"],
        "Awake could not save the Lid closed setting as off, so it may come back on after a restart."
    );
    assert_eq!(stored(&rig, "lid_held"), None);
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn a_user_lid_set_keeps_a_pending_give_back_problem() {
    let rig = adapter_rig();
    lid_on_with_a_lease(&rig).await;
    rig.lid.refuse_clear(5);
    rig.dispatch(json!("release_all")).await.unwrap();
    assert_eq!(state(&rig)["lid_problem"], CLEAR_PROBLEM);

    let answer = rig.dispatch(lid(false)).await.unwrap();

    assert_eq!(answer["lid_problem"], CLEAR_PROBLEM);
    assert!(rig.lid.is_set());

    rig.lid.allow();
    rig.advance(Duration::from_secs(60)).await;
    rig.idle().await;

    assert!(!rig.lid.is_set());
    assert_eq!(state(&rig)["lid_problem"], Value::Null);
    rig.shutdown().await;
}

#![allow(clippy::unwrap_used)]

mod common;

use std::time::Duration;

use common::{ORIGIN_MS, Rig};
use navori_kernel::{ActionError, DispatchError};
use navori_macos::fake::FakeProcess;
use serde_json::{Value, json};

const CLAUDE: i32 = 100;
const HOOK: i32 = 102;
const SESSION: &str = "sess-1";
const GRACE: Duration = Duration::from_secs(121);

fn after_origin_us(seconds: u64) -> u64 {
    (ORIGIN_MS + seconds * 1000) * 1000
}

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

fn hold() -> Value {
    json!({ "hold_session": { "session_id": SESSION, "pid": HOOK, "ppid": CLAUDE } })
}

fn stop() -> Value {
    json!({ "stop_session": { "session_id": SESSION } })
}

fn start_agent(agent: &str) -> Value {
    json!({ "subagent_start": { "session_id": SESSION, "agent_id": agent, "pid": HOOK, "ppid": CLAUDE } })
}

fn stop_agent(agent: &str) -> Value {
    json!({ "subagent_stop": { "session_id": SESSION, "agent_id": agent } })
}

fn session(rig: &Rig) -> Option<Value> {
    rig.kernel.state("awake").unwrap()["leases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|lease| lease["kind"] == "session")
        .cloned()
}

async fn pass(rig: &Rig, by: Duration) {
    rig.advance(by).await;
    rig.idle().await;
}

#[tokio::test(start_paused = true)]
async fn a_running_subagent_keeps_a_stopped_session_with_no_child_process() {
    let rig = Rig::new();
    hook_chain(&rig);
    rig.dispatch(hold()).await.unwrap();
    rig.dispatch(start_agent("a1")).await.unwrap();
    rig.dispatch(stop()).await.unwrap();

    assert_eq!(session(&rig).unwrap()["stopped"], false);
    pass(&rig, GRACE).await;

    assert_eq!(session(&rig).unwrap()["stopped"], false);
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn grace_runs_again_from_the_last_subagent_stop() {
    let rig = Rig::new();
    hook_chain(&rig);
    rig.dispatch(hold()).await.unwrap();
    rig.dispatch(start_agent("a1")).await.unwrap();
    rig.dispatch(start_agent("a2")).await.unwrap();
    rig.dispatch(stop()).await.unwrap();
    pass(&rig, Duration::from_secs(600)).await;

    rig.dispatch(stop_agent("a1")).await.unwrap();
    pass(&rig, GRACE).await;
    assert_eq!(session(&rig).unwrap()["stopped"], false, "a2 still runs");

    rig.dispatch(stop_agent("a2")).await.unwrap();
    assert_eq!(session(&rig).unwrap()["stopped"], true);
    pass(&rig, Duration::from_secs(60)).await;
    assert!(session(&rig).is_some(), "grace is not over yet");

    pass(&rig, Duration::from_secs(61)).await;
    assert!(session(&rig).is_none());
    assert!(rig.power.held().is_empty());
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn a_subagent_with_no_lease_makes_one_that_ends_after_it_stops() {
    let rig = Rig::new();
    hook_chain(&rig);

    let answer = rig.dispatch(start_agent("a1")).await.unwrap();
    assert_eq!(answer["leases"][0]["kind"], "session");
    assert_eq!(answer["leases"][0]["stopped"], false);
    assert_eq!(answer["system_held"], true);

    rig.dispatch(stop_agent("a1")).await.unwrap();
    pass(&rig, GRACE).await;
    assert!(session(&rig).is_none());
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn a_subagent_start_moves_the_session_ttl() {
    let rig = Rig::new();
    hook_chain(&rig);
    rig.dispatch(hold()).await.unwrap();
    pass(&rig, Duration::from_secs(3600)).await;
    let answer = rig.dispatch(start_agent("a1")).await.unwrap();
    assert_eq!(
        answer["leases"][0]["ends_at_ms"],
        i64::try_from(ORIGIN_MS + (3600 + 8 * 3600) * 1000).unwrap()
    );
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn a_subagent_on_a_held_session_reads_no_process_and_a_running_one_needs_no_timer() {
    let rig = Rig::new();
    hook_chain(&rig);
    rig.dispatch(hold()).await.unwrap();
    let reads = rig.processes.list_calls();
    rig.dispatch(start_agent("a1")).await.unwrap();
    rig.dispatch(stop()).await.unwrap();
    pass(&rig, Duration::from_secs(1800)).await;
    assert_eq!(rig.processes.list_calls(), reads);
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn the_cap_still_ends_a_session_whose_subagent_stop_never_came() {
    let rig = Rig::new();
    hook_chain(&rig);
    rig.dispatch(hold()).await.unwrap();
    rig.dispatch(start_agent("a1")).await.unwrap();
    rig.dispatch(stop()).await.unwrap();
    pass(&rig, Duration::from_secs(2 * 3600 + 1)).await;
    assert!(session(&rig).is_none());
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn a_new_prompt_does_not_forget_running_subagents() {
    let rig = Rig::new();
    hook_chain(&rig);
    rig.dispatch(hold()).await.unwrap();
    rig.dispatch(start_agent("a1")).await.unwrap();
    rig.dispatch(stop()).await.unwrap();
    rig.dispatch(hold()).await.unwrap();
    rig.dispatch(stop()).await.unwrap();
    pass(&rig, GRACE).await;
    assert_eq!(session(&rig).unwrap()["stopped"], false);
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn unknown_subagents_and_sessions_change_nothing() {
    let rig = Rig::new();
    hook_chain(&rig);
    rig.dispatch(stop_agent("a1")).await.unwrap();
    assert!(session(&rig).is_none());
    rig.dispatch(hold()).await.unwrap();
    rig.dispatch(stop()).await.unwrap();
    rig.dispatch(stop_agent("never-started")).await.unwrap();
    pass(&rig, GRACE).await;
    assert!(session(&rig).is_none());
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn a_bad_agent_id_is_refused() {
    let rig = Rig::new();
    hook_chain(&rig);
    for action in [
        start_agent("not ok"),
        stop_agent(""),
        stop_agent(&"x".repeat(129)),
    ] {
        let err = rig.dispatch(action).await.unwrap_err();
        assert!(matches!(
            err,
            DispatchError::Action(ActionError::Invalid(_))
        ));
    }
    assert!(session(&rig).is_none());
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn forged_subagents_stay_bounded() {
    let rig = Rig::new();
    hook_chain(&rig);
    rig.dispatch(hold()).await.unwrap();
    for index in 0..300 {
        rig.dispatch(start_agent(&format!("a{index}")))
            .await
            .unwrap();
    }
    rig.dispatch(stop()).await.unwrap();
    for index in 0..64 {
        rig.dispatch(stop_agent(&format!("a{index}")))
            .await
            .unwrap();
    }
    assert_eq!(
        session(&rig).unwrap()["stopped"],
        true,
        "only the first 64 were remembered"
    );
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn release_all_forgets_subagents() {
    let rig = Rig::new();
    hook_chain(&rig);
    rig.dispatch(hold()).await.unwrap();
    rig.dispatch(start_agent("a1")).await.unwrap();
    rig.dispatch(json!("release_all")).await.unwrap();
    rig.dispatch(hold()).await.unwrap();
    rig.dispatch(stop()).await.unwrap();
    assert_eq!(session(&rig).unwrap()["stopped"], true);
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn a_followed_session_stays_hidden_when_a_subagent_starts_while_tracking_is_off() {
    let rig = Rig::new();
    hook_chain(&rig);
    rig.dispatch(json!({ "tracking": { "on": false } }))
        .await
        .unwrap();
    let answer = rig.dispatch(start_agent("a1")).await.unwrap();
    assert_eq!(answer["leases"], json!([]));
    assert!(rig.power.held().is_empty());
    let answer = rig
        .dispatch(json!({ "tracking": { "on": true } }))
        .await
        .unwrap();
    assert_eq!(answer["leases"][0]["stopped"], false);
    rig.shutdown().await;
}

// The label after grace.

#[tokio::test(start_paused = true)]
async fn a_stopped_session_reads_stopped_through_grace_then_working_if_a_new_child_runs() {
    let rig = Rig::new();
    hook_chain(&rig);
    rig.dispatch(hold()).await.unwrap();
    rig.dispatch(stop()).await.unwrap();
    rig.processes.add(FakeProcess::new(
        300,
        CLAUDE,
        501,
        after_origin_us(10),
        "/bin/zsh",
    ));
    assert_eq!(session(&rig).unwrap()["stopped"], true);
    pass(&rig, Duration::from_secs(60)).await;
    assert_eq!(session(&rig).unwrap()["stopped"], true);

    pass(&rig, Duration::from_secs(61)).await;
    assert_eq!(session(&rig).unwrap()["stopped"], false);

    rig.processes.exit(300);
    pass(&rig, Duration::from_secs(61)).await;
    assert!(session(&rig).is_none());
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn a_new_stop_after_found_work_reads_stopped_again() {
    let rig = Rig::new();
    hook_chain(&rig);
    rig.dispatch(hold()).await.unwrap();
    rig.dispatch(stop()).await.unwrap();
    rig.processes.add(FakeProcess::new(
        300,
        CLAUDE,
        501,
        after_origin_us(10),
        "/bin/zsh",
    ));
    pass(&rig, GRACE).await;
    assert_eq!(session(&rig).unwrap()["stopped"], false);

    rig.dispatch(hold()).await.unwrap();
    rig.dispatch(stop()).await.unwrap();
    assert_eq!(session(&rig).unwrap()["stopped"], true);
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn an_unreadable_process_list_keeps_the_session_but_does_not_call_it_working() {
    let rig = Rig::new();
    hook_chain(&rig);
    rig.dispatch(hold()).await.unwrap();
    rig.dispatch(stop()).await.unwrap();
    rig.processes.fail_list(true);
    pass(&rig, GRACE).await;
    let lease = session(&rig).unwrap();
    assert_eq!(lease["stopped"], true);
    rig.shutdown().await;
}

/// Claude Code runs `caffeinate` only while it is busy, so it counts as work like any other child.
#[tokio::test(start_paused = true)]
async fn the_harness_own_caffeinate_counts_as_work() {
    let rig = Rig::new();
    hook_chain(&rig);
    rig.dispatch(hold()).await.unwrap();
    rig.dispatch(stop()).await.unwrap();
    rig.processes.add(FakeProcess::new(
        300,
        CLAUDE,
        501,
        after_origin_us(10),
        "/usr/bin/caffeinate",
    ));
    pass(&rig, GRACE).await;
    assert_eq!(session(&rig).unwrap()["stopped"], false);
    rig.shutdown().await;
}

// What the Harness says is still in flight.

fn stop_with(background: bool) -> Value {
    json!({ "stop_session": { "session_id": SESSION, "background": background } })
}

fn stop_agent_with(agent: &str, background: bool) -> Value {
    json!({ "subagent_stop": { "session_id": SESSION, "agent_id": agent, "background": background } })
}

#[tokio::test(start_paused = true)]
async fn a_stop_with_background_work_keeps_the_lease_with_no_process_read() {
    let rig = Rig::new();
    hook_chain(&rig);
    rig.dispatch(hold()).await.unwrap();
    let reads = rig.processes.list_calls();
    rig.dispatch(stop_with(true)).await.unwrap();
    assert_eq!(session(&rig).unwrap()["stopped"], false);
    pass(&rig, Duration::from_secs(1800)).await;
    assert_eq!(session(&rig).unwrap()["stopped"], false);
    assert_eq!(rig.processes.list_calls(), reads);
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn a_later_stop_with_nothing_in_flight_starts_grace() {
    let rig = Rig::new();
    hook_chain(&rig);
    rig.dispatch(hold()).await.unwrap();
    rig.dispatch(stop_with(true)).await.unwrap();
    pass(&rig, Duration::from_secs(600)).await;
    rig.dispatch(stop_with(false)).await.unwrap();
    assert_eq!(session(&rig).unwrap()["stopped"], true);
    pass(&rig, Duration::from_secs(60)).await;
    assert!(session(&rig).is_some());
    pass(&rig, Duration::from_secs(61)).await;
    assert!(session(&rig).is_none());
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn a_subagent_stop_with_background_work_starts_no_grace() {
    let rig = Rig::new();
    hook_chain(&rig);
    rig.dispatch(hold()).await.unwrap();
    rig.dispatch(start_agent("a1")).await.unwrap();
    rig.dispatch(stop_with(true)).await.unwrap();
    rig.dispatch(stop_agent_with("a1", true)).await.unwrap();
    pass(&rig, GRACE).await;
    assert_eq!(session(&rig).unwrap()["stopped"], false);

    rig.dispatch(stop_agent_with("a1", false)).await.unwrap();
    assert_eq!(session(&rig).unwrap()["stopped"], true);
    pass(&rig, GRACE).await;
    assert!(session(&rig).is_none());
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn the_cap_still_ends_a_session_whose_background_work_never_reports_back() {
    let rig = Rig::new();
    hook_chain(&rig);
    rig.dispatch(hold()).await.unwrap();
    rig.dispatch(stop_with(true)).await.unwrap();
    pass(&rig, Duration::from_secs(2 * 3600 + 1)).await;
    assert!(session(&rig).is_none());
    rig.shutdown().await;
}

/// The event order of a real Claude Code 2.1.318 run with one background subagent.
#[tokio::test(start_paused = true)]
async fn a_recorded_background_subagent_run_reads_working_until_its_last_stop() {
    let rig = Rig::new();
    hook_chain(&rig);
    let steps = [
        (hold(), false),
        (start_agent("a1"), false),
        (stop_with(true), false),
        (stop_agent_with("a1", true), false),
        (hold(), false),
        (stop_with(true), false),
        (start_agent("a1"), false),
        (stop_agent_with("a1", true), false),
        (hold(), false),
        (start_agent("a1"), false),
        (stop_with(true), false),
        (stop_agent_with("a1", true), false),
        (hold(), false),
        (stop_with(false), true),
    ];
    for (index, (action, stopped)) in steps.into_iter().enumerate() {
        rig.dispatch(action).await.unwrap();
        assert_eq!(session(&rig).unwrap()["stopped"], stopped, "step {index}");
        pass(&rig, Duration::from_secs(5)).await;
    }
    pass(&rig, GRACE).await;
    assert!(session(&rig).is_none());
    rig.shutdown().await;
}

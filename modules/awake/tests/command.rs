mod common;

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use common::{ASSERTION_NAME, ORIGIN_MS, Rig};
use navori_kernel::{ActionError, DispatchError, Severity};
use navori_macos::fake::{FakeProcess, FakeProcesses};
use navori_macos::{AssertionKind, PlatformError, ProcEntry, ProcessSource, ReadError, WatchError};
use serde_json::{Value, json};

const START_US: u64 = 5_000;
const PID: i32 = 42;
const EXE: &str = "/tmp/feature-awake/sleep";
const EXIT_PROBLEM: &str =
    "Exit watches stopped, so a command lease has no end signal. Navori retries in 60 seconds.";

fn hold_command(pid: i32) -> Value {
    json!({ "hold_command": { "pid": pid } })
}

fn lease_count(state: &Value) -> usize {
    state["leases"].as_array().map(Vec::len).unwrap_or(0)
}

fn command(pid: i32) -> FakeProcess {
    FakeProcess::new(pid, 1, 501, START_US, EXE)
        .argv(&["sleep", "--branch", "feature-awake"])
        .cwd("/tmp/navori-project")
}

fn command_row(pid: i32) -> Value {
    json!({
        "key": format!("command:{pid}:{START_US}"),
        "kind": "command",
        "harness": null,
        "label": "sleep",
        "pid": pid,
        "stopped": false,
        "ends_at_ms": null
    })
}

fn awake_state(rig: &Rig) -> Value {
    match rig.kernel.state("awake") {
        Some(state) => state,
        None => panic!("awake state is missing"),
    }
}

async fn until(rig: &Rig, mut pred: impl FnMut(&Value) -> bool) -> Value {
    for _ in 0..50 {
        let state = awake_state(rig);
        if pred(&state) {
            return state;
        }
        rig.idle().await;
    }
    let state = awake_state(rig);
    panic!("condition not met: {state}");
}

#[tokio::test(start_paused = true)]
async fn a_command_lease_holds_until_its_process_exits() {
    let rig = Rig::new();
    rig.processes.add(command(PID));

    let state = rig.dispatch(hold_command(PID)).await.unwrap();

    assert_eq!(
        rig.power.held(),
        vec![(AssertionKind::System, ASSERTION_NAME.to_owned())]
    );
    assert_eq!(rig.power.releases(), 0);
    assert_eq!(rig.exits.watched(), vec![PID]);
    assert_eq!(state["leases"], json!([command_row(PID)]));
    assert_eq!(state["system_held"], true);
    assert_eq!(state["display_held"], false);
    assert_eq!(state["since_ms"], ORIGIN_MS as i64);
    assert_eq!(state["problem"], Value::Null);
    assert!(
        rig.processes.reads().iter().all(|(_, kind)| *kind == "exe"),
        "a command lease must not read argv, cwd, or cpu: {:?}",
        rig.processes.reads()
    );

    let started: Vec<_> = rig
        .kernel
        .recent_events(10)
        .unwrap()
        .into_iter()
        .filter(|event| event.kind == "awake.lease_started")
        .collect();
    assert_eq!(started.len(), 1);
    assert_eq!(started[0].module, "awake");
    assert_eq!(started[0].severity, Severity::Info);
    assert_eq!(started[0].summary, "Keeping this Mac awake: sleep.");
    assert!(!started[0].summary.contains("feature-awake"));
    assert!(!started[0].summary.contains("1 hour"));

    rig.exits.fire(PID);
    let ended_state = until(&rig, |state| lease_count(state) == 0).await;
    assert_eq!(ended_state["system_held"], false);
    assert_eq!(ended_state["since_ms"], Value::Null);
    assert_eq!(ended_state["problem"], Value::Null);
    assert!(rig.power.held().is_empty());
    assert_eq!(rig.power.releases(), 1);

    let ended: Vec<_> = rig
        .kernel
        .recent_events(10)
        .unwrap()
        .into_iter()
        .filter(|event| event.kind == "awake.lease_ended")
        .collect();
    assert_eq!(ended.len(), 1);
    assert_eq!(ended[0].module, "awake");
    assert_eq!(ended[0].severity, Severity::Info);
    assert_eq!(
        ended[0].summary,
        "Stopped keeping this Mac awake: sleep exited."
    );
    assert!(!ended[0].summary.contains("feature-awake"));
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn a_command_lease_has_no_deadline() {
    let rig = Rig::new();
    rig.processes.add(command(PID));
    let state = rig.dispatch(hold_command(PID)).await.unwrap();
    assert_eq!(state["leases"][0]["ends_at_ms"], Value::Null);
    assert_eq!(state["leases"][0]["kind"], "command");

    rig.advance(Duration::from_secs(8 * 3600 + 1)).await;
    rig.idle().await;
    let later = rig.dispatch(json!("status")).await.unwrap();
    assert_eq!(later["leases"], json!([command_row(PID)]));
    assert_eq!(later["system_held"], true);
    assert_eq!(rig.power.releases(), 0);
    assert_eq!(rig.exits.watched(), vec![PID]);
    let ended = rig
        .kernel
        .recent_events(10)
        .unwrap()
        .into_iter()
        .filter(|event| event.kind == "awake.lease_ended")
        .count();
    assert_eq!(ended, 0);
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn a_gone_process_answers_ok_with_no_lease() {
    let rig = Rig::new();
    let state = rig.dispatch(hold_command(99)).await.unwrap();
    assert_eq!(lease_count(&state), 0);
    assert_eq!(state["system_held"], false);
    assert_eq!(state["problem"], Value::Null);
    assert!(rig.power.held().is_empty());
    assert!(rig.exits.watched().is_empty());
    assert!(rig.processes.reads().is_empty());
    let started = rig
        .kernel
        .recent_events(10)
        .unwrap()
        .into_iter()
        .any(|event| event.kind == "awake.lease_started");
    assert!(!started);
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn a_zombie_process_answers_ok_with_no_lease() {
    let rig = Rig::new();
    rig.processes.add(command(PID).zombie());
    let state = rig.dispatch(hold_command(PID)).await.unwrap();
    assert_eq!(lease_count(&state), 0);
    assert_eq!(state["system_held"], false);
    assert_eq!(state["problem"], Value::Null);
    assert!(rig.power.held().is_empty());
    assert!(rig.exits.watched().is_empty());
    assert!(
        rig.processes.reads().is_empty(),
        "a zombie is not a command we name: {:?}",
        rig.processes.reads()
    );
    rig.shutdown().await;
}

struct FirstRuid {
    inner: Arc<FakeProcesses>,
    flipped: AtomicBool,
    ruid: u32,
}

impl ProcessSource for FirstRuid {
    fn current_uid(&self) -> u32 {
        self.inner.current_uid()
    }

    fn list(&self) -> Result<Vec<ProcEntry>, PlatformError> {
        self.inner.list()
    }

    fn entry(&self, pid: i32) -> Result<ProcEntry, ReadError> {
        let mut entry = self.inner.entry(pid)?;
        if !self.flipped.swap(true, Ordering::SeqCst) {
            entry.ruid = self.ruid;
        }
        Ok(entry)
    }

    fn exe(&self, pid: i32) -> Result<PathBuf, ReadError> {
        self.inner.exe(pid)
    }

    fn argv(&self, pid: i32) -> Result<Vec<String>, ReadError> {
        self.inner.argv(pid)
    }

    fn cwd(&self, pid: i32) -> Result<PathBuf, ReadError> {
        self.inner.cwd(pid)
    }

    fn cpu_time(&self, pid: i32) -> Result<Duration, ReadError> {
        self.inner.cpu_time(pid)
    }
}

#[tokio::test(start_paused = true)]
async fn a_pid_is_classified_once() {
    let processes = Arc::new(FakeProcesses::new(501));
    processes.add(command(PID));
    let source = Arc::new(FirstRuid {
        inner: Arc::clone(&processes),
        flipped: AtomicBool::new(false),
        ruid: 502,
    });
    let rig = Rig::with_source(processes, source);
    let err = rig.dispatch(hold_command(PID)).await.unwrap_err();
    assert!(
        matches!(
            err,
            DispatchError::Action(ActionError::Invalid(ref message))
                if message == "pid 42 belongs to another user"
        ),
        "{err:?}"
    );
    let state = rig.kernel.state("awake").unwrap();
    assert_eq!(lease_count(&state), 0);
    assert_eq!(state["system_held"], false);
    assert!(rig.power.held().is_empty());
    assert!(rig.exits.watched().is_empty());
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn an_unreadable_live_pid_is_not_an_empty_hold() {
    let rig = Rig::new();
    rig.processes.add(command(PID));
    rig.processes.fail_entry(PID, ReadError::Denied);
    let err = rig.dispatch(hold_command(PID)).await.unwrap_err();
    assert!(
        matches!(
            err,
            DispatchError::Action(ActionError::Failed(ref message))
                if message == "the process list could not be read (reading the process failed); retry the hold"
        ),
        "{err:?}"
    );
    let state = rig.kernel.state("awake").unwrap();
    assert_eq!(lease_count(&state), 0);
    assert_eq!(state["system_held"], false);
    assert_eq!(state["problem"], Value::Null);
    assert!(rig.power.held().is_empty());
    assert!(rig.exits.watched().is_empty());
    let started = rig
        .kernel
        .recent_events(10)
        .unwrap()
        .into_iter()
        .any(|event| event.kind == "awake.lease_started");
    assert!(
        !started,
        "awake run must not treat this answer as a held lease"
    );
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn a_process_of_another_user_is_refused() {
    let rig = Rig::new();
    rig.processes
        .add(FakeProcess::new(PID, 1, 502, START_US, EXE));
    let err = rig.dispatch(hold_command(PID)).await.unwrap_err();
    assert!(
        matches!(
            err,
            DispatchError::Action(ActionError::Invalid(ref message))
                if message == "pid 42 belongs to another user"
        ),
        "{err:?}"
    );
    let state = rig.kernel.state("awake").unwrap();
    assert_eq!(lease_count(&state), 0);
    assert_eq!(state["system_held"], false);
    assert!(rig.power.held().is_empty());
    assert!(rig.exits.watched().is_empty());
    assert!(rig.processes.reads().is_empty());
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn a_non_positive_pid_is_invalid() {
    let rig = Rig::new();
    for pid in [0, -1] {
        let err = rig.dispatch(hold_command(pid)).await.unwrap_err();
        assert!(
            matches!(
                err,
                DispatchError::Action(ActionError::Invalid(ref message))
                    if *message == format!("pid must be positive, not {pid}")
            ),
            "{err:?}"
        );
    }
    let state = rig.kernel.state("awake").unwrap();
    assert_eq!(lease_count(&state), 0);
    assert!(rig.power.held().is_empty());
    assert!(rig.exits.watched().is_empty());
    assert_eq!(rig.processes.list_calls(), 0);
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn a_failed_exit_source_build_answers_failed_with_no_lease() {
    let rig = Rig::with_exits_factory(Box::new(|_| {
        Err(PlatformError::Kqueue {
            op: "kqueue",
            errno: 1,
        })
    }));
    rig.processes.add(command(PID));
    let err = rig.dispatch(hold_command(PID)).await.unwrap_err();
    assert!(
        matches!(
            err,
            DispatchError::Action(ActionError::Failed(ref message))
                if message == "exit watches could not be started (kqueue kqueue failed with errno 1; the exit watch was not changed); no command lease was held"
        ),
        "{err:?}"
    );
    let state = rig.kernel.state("awake").unwrap();
    assert_eq!(lease_count(&state), 0);
    assert_eq!(state["system_held"], false);
    assert_eq!(state["problem"], Value::Null);
    assert!(rig.power.held().is_empty());
    assert!(rig.exits.watched().is_empty());
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn kqueue_rejecting_the_watch_answers_failed_with_no_lease() {
    let rig = Rig::new();
    rig.processes.add(command(PID));
    rig.exits
        .fail_next_watch(PID, WatchError::Kqueue { errno: 22 });
    let err = rig.dispatch(hold_command(PID)).await.unwrap_err();
    assert!(
        matches!(
            err,
            DispatchError::Action(ActionError::Failed(ref message))
                if message == "kqueue failed with errno 22; no command lease was held"
        ),
        "{err:?}"
    );
    let state = rig.kernel.state("awake").unwrap();
    assert_eq!(lease_count(&state), 0);
    assert_eq!(state["system_held"], false);
    assert_eq!(state["problem"], Value::Null);
    assert!(rig.power.held().is_empty());
    assert!(rig.exits.watched().is_empty());
    assert!(rig.exits.tokens(PID).is_empty());
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn two_leases_on_one_process_unwatch_only_at_the_last() {
    let rig = Rig::new();
    rig.processes.add(command(PID));
    rig.dispatch(hold_command(PID)).await.unwrap();
    rig.advance(Duration::from_secs(1)).await;
    let state = rig.dispatch(hold_command(PID)).await.unwrap();

    assert_eq!(lease_count(&state), 1);
    assert_eq!(state["leases"], json!([command_row(PID)]));
    assert_eq!(state["since_ms"], ORIGIN_MS as i64);
    assert_eq!(rig.exits.watched(), vec![PID]);
    assert_eq!(rig.exits.tokens(PID).len(), 1);
    assert_eq!(rig.power.held().len(), 1);
    assert_eq!(rig.power.releases(), 0);

    let started = rig
        .kernel
        .recent_events(10)
        .unwrap()
        .into_iter()
        .filter(|event| event.kind == "awake.lease_started")
        .count();
    assert_eq!(started, 1);
    let ended_early = rig
        .kernel
        .recent_events(10)
        .unwrap()
        .into_iter()
        .filter(|event| event.kind == "awake.lease_ended")
        .count();
    assert_eq!(ended_early, 0);

    rig.exits.fire(PID);
    let done = until(&rig, |state| lease_count(state) == 0).await;
    assert_eq!(done["system_held"], false);
    assert_eq!(done["since_ms"], Value::Null);
    assert!(rig.power.held().is_empty());
    assert_eq!(rig.power.releases(), 1);
    let ended: Vec<_> = rig
        .kernel
        .recent_events(10)
        .unwrap()
        .into_iter()
        .filter(|event| event.kind == "awake.lease_ended")
        .collect();
    assert_eq!(ended.len(), 1);
    assert_eq!(
        ended[0].summary,
        "Stopped keeping this Mac awake: sleep exited."
    );
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn no_exit_source_is_built_until_a_process_lease_exists() {
    let rig = Rig::new();
    rig.exits.close();
    let status = rig.dispatch(json!("status")).await.unwrap();
    assert_eq!(status["problem"], Value::Null);
    assert!(rig.exits.watched().is_empty());

    let manual = rig.dispatch(json!("hold_until_off")).await.unwrap();
    assert_eq!(manual["problem"], Value::Null);
    assert_eq!(lease_count(&manual), 1);
    assert!(rig.exits.watched().is_empty());
    assert!(rig.exits.tokens(PID).is_empty());
    assert_eq!(rig.processes.list_calls(), 0);

    rig.processes.add(command(PID));
    let held = rig.dispatch(hold_command(PID)).await.unwrap();
    assert_eq!(lease_count(&held), 2);
    assert_eq!(held["problem"], Value::Null);
    assert_eq!(rig.exits.watched(), vec![PID]);
    assert_eq!(rig.exits.tokens(PID).len(), 1);
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn a_dead_exit_source_records_a_problem_and_does_not_spin_then_rebuilds_on_the_timer() {
    let rig = Rig::new();
    rig.processes.add(command(PID));
    rig.dispatch(hold_command(PID)).await.unwrap();
    assert_eq!(rig.exits.tokens(PID).len(), 1);

    rig.exits.close();
    let dead = until(&rig, |state| state["problem"] == EXIT_PROBLEM).await;
    assert_eq!(lease_count(&dead), 1);
    assert_eq!(dead["system_held"], true);
    assert_eq!(rig.power.releases(), 0);
    rig.idle().await;

    rig.advance(Duration::from_secs(59)).await;
    rig.idle().await;
    let still = awake_state(&rig);
    assert_eq!(still["problem"], EXIT_PROBLEM);
    assert_eq!(lease_count(&still), 1);
    assert_eq!(rig.exits.tokens(PID).len(), 1);

    rig.advance(Duration::from_secs(1)).await;
    let rebuilt = until(&rig, |state| state["problem"].is_null()).await;
    assert_eq!(lease_count(&rebuilt), 1);
    assert_eq!(rebuilt["system_held"], true);
    assert_eq!(rig.exits.watched(), vec![PID]);
    assert_eq!(rig.exits.tokens(PID).len(), 2);

    rig.exits.fire(PID);
    let done = until(&rig, |state| lease_count(state) == 0).await;
    assert_eq!(done["problem"], Value::Null);
    assert_eq!(rig.power.releases(), 1);
    rig.shutdown().await;
}

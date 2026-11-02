#![allow(clippy::unwrap_used)]

mod common;

use std::time::Duration;

use common::{ASSERTION_NAME, ORIGIN_MS, Rig};
use navori_kernel::{ActionError, DispatchError, Severity};
use navori_macos::AssertionKind;
use navori_macos::fake::FakeProcess;
use serde_json::{Value, json};

const CLAUDE: i32 = 100;
const CLAUDE_START: u64 = 100_000;
const COMMAND_PID: i32 = 42;
const COMMAND_START: u64 = 5_000;
const SESSION: &str = "sess-1";
const FIRST_HOLD_US: i64 = 1_700_000_000_000_000;
const STOP_MARK_US: i64 = 1_700_000_005_000_000;
const DEADLINE_US: i64 = 1_700_028_800_000_000;
const ENDS_AT_MS: i64 = 1_700_028_800_000;
const FIVE_SECONDS: Duration = Duration::from_secs(5);
const GRACE: Duration = Duration::from_secs(120);
const CAP: Duration = Duration::from_secs(2 * 60 * 60);
const TTL: Duration = Duration::from_secs(8 * 60 * 60);
const RETRY: Duration = Duration::from_secs(60);
const ARGV_MARK: &str = "--secret-argv";
const CWD_MARK: &str = "secret-cwd-marker";
const ENV_MARK: &str = "SECRET_ENV_MARKER";
const EXE_MARK: &str = "secret-exe-marker";

const SCHEMA: &str = "
CREATE TABLE awake_leases (
  key               TEXT PRIMARY KEY,
  kind              TEXT NOT NULL CHECK (kind IN ('session','command')),
  session_id        TEXT,
  harness           TEXT,
  pid               INTEGER NOT NULL,
  process_start_us  INTEGER NOT NULL,
  first_hold_us     INTEGER,
  stop_mark_us      INTEGER,
  deadline_us       INTEGER
);
";

struct Row {
    key: String,
    kind: String,
    session_id: Option<String>,
    harness: Option<String>,
    pid: i64,
    process_start_us: i64,
    first_hold_us: Option<i64>,
    stop_mark_us: Option<i64>,
    deadline_us: Option<i64>,
}

impl Row {
    fn cells(&self) -> Vec<String> {
        vec![
            self.key.clone(),
            self.kind.clone(),
            self.session_id.clone().unwrap_or_default(),
            self.harness.clone().unwrap_or_default(),
            self.pid.to_string(),
            self.process_start_us.to_string(),
            self.first_hold_us
                .map(|v| v.to_string())
                .unwrap_or_default(),
            self.stop_mark_us.map(|v| v.to_string()).unwrap_or_default(),
            self.deadline_us.map(|v| v.to_string()).unwrap_or_default(),
        ]
    }
}

fn claude(rig: &Rig) {
    rig.processes.add(FakeProcess::new(
        CLAUDE,
        1,
        501,
        CLAUDE_START,
        "/usr/local/bin/claude",
    ));
}

fn command_process(rig: &Rig) {
    rig.processes.add(FakeProcess::new(
        COMMAND_PID,
        1,
        501,
        COMMAND_START,
        "/tmp/feature-awake/sleep",
    ));
}

fn hold_session(session_id: &str, pid: i32) -> Value {
    json!({ "hold_session": { "session_id": session_id, "pid": pid, "ppid": 1 } })
}

fn hold_command(pid: i32) -> Value {
    json!({ "hold_command": { "pid": pid } })
}

fn stop_session(session_id: &str) -> Value {
    json!({ "stop_session": { "session_id": session_id } })
}

fn awake_state(rig: &Rig) -> Value {
    match rig.kernel.state("awake") {
        Some(state) => state,
        None => panic!("awake state is missing"),
    }
}

fn lease_count(state: &Value) -> usize {
    state["leases"].as_array().map(Vec::len).unwrap_or(0)
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

fn ended(rig: &Rig) -> Vec<(Severity, String)> {
    rig.kernel
        .recent_events(20)
        .unwrap()
        .into_iter()
        .filter(|event| event.kind == "awake.lease_ended")
        .map(|event| (event.severity, event.summary))
        .collect()
}

fn table_exists(rig: &Rig) -> bool {
    rig.kernel.store().with_conn(|conn| {
        conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'awake_leases'",
            [],
            |row| row.get::<_, i64>(0),
        )
        .unwrap_or(0)
            > 0
    })
}

fn columns(rig: &Rig) -> Vec<String> {
    rig.kernel.store().with_conn(|conn| {
        let Ok(mut stmt) = conn.prepare("PRAGMA table_info(awake_leases)") else {
            return Vec::new();
        };
        stmt.query_map([], |row| row.get::<_, String>(1))
            .map(|rows| rows.filter_map(Result::ok).collect())
            .unwrap_or_default()
    })
}

fn rows(rig: &Rig) -> Vec<Row> {
    rig.kernel.store().with_conn(|conn| {
        let Ok(mut stmt) = conn.prepare(
            "SELECT key, kind, session_id, harness, pid, process_start_us, first_hold_us, stop_mark_us, deadline_us
             FROM awake_leases ORDER BY key",
        ) else {
            return Vec::new();
        };
        stmt.query_map([], |row| {
            Ok(Row {
                key: row.get(0)?,
                kind: row.get(1)?,
                session_id: row.get(2)?,
                harness: row.get(3)?,
                pid: row.get(4)?,
                process_start_us: row.get(5)?,
                first_hold_us: row.get(6)?,
                stop_mark_us: row.get(7)?,
                deadline_us: row.get(8)?,
            })
        })
        .map(|found| found.filter_map(Result::ok).collect())
        .unwrap_or_default()
    })
}

fn drop_leases(rig: &Rig) {
    rig.kernel.store().with_conn(|conn| {
        let _ = conn.execute("DROP TABLE awake_leases", []);
    });
}

fn create_leases(rig: &Rig) {
    rig.kernel.store().with_conn(|conn| {
        conn.execute_batch(SCHEMA).unwrap();
    });
}

fn assert_held_system(rig: &Rig) {
    assert_eq!(
        rig.power.held(),
        vec![(AssertionKind::System, ASSERTION_NAME.to_owned())]
    );
}

async fn open_session(rig: &Rig) -> Value {
    claude(rig);
    rig.dispatch(hold_session(SESSION, CLAUDE)).await.unwrap()
}

#[tokio::test(start_paused = true)]
async fn a_session_lease_row_stores_pid_start_first_hold_stop_mark_and_deadline() {
    let rig = Rig::new();
    open_session(&rig).await;
    rig.advance(FIVE_SECONDS).await;
    rig.dispatch(stop_session(SESSION)).await.unwrap();

    let found = rows(&rig);
    assert_eq!(found.len(), 1, "a session lease writes one row");
    let row = &found[0];
    assert_eq!(row.key, format!("session:{SESSION}"));
    assert_eq!(row.kind, "session");
    assert_eq!(row.session_id.as_deref(), Some(SESSION));
    assert_eq!(row.harness.as_deref(), Some("Claude Code"));
    assert_eq!(row.pid, i64::from(CLAUDE));
    assert_eq!(row.process_start_us, i64::try_from(CLAUDE_START).unwrap());
    assert_eq!(row.first_hold_us, Some(FIRST_HOLD_US));
    assert_eq!(row.stop_mark_us, Some(STOP_MARK_US));
    assert_eq!(row.deadline_us, Some(DEADLINE_US));
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn a_command_lease_row_has_no_end_time() {
    let rig = Rig::new();
    command_process(&rig);
    rig.dispatch(hold_command(COMMAND_PID)).await.unwrap();
    rig.advance(FIVE_SECONDS).await;

    let found = rows(&rig);
    assert_eq!(found.len(), 1, "a command lease writes one row");
    let row = &found[0];
    assert_eq!(row.key, format!("command:{COMMAND_PID}:{COMMAND_START}"));
    assert_eq!(row.kind, "command");
    assert_eq!(row.session_id, None);
    assert_eq!(row.harness, None);
    assert_eq!(row.pid, i64::from(COMMAND_PID));
    assert_eq!(row.process_start_us, i64::try_from(COMMAND_START).unwrap());
    assert_eq!(row.first_hold_us, None);
    assert_eq!(row.stop_mark_us, None);
    assert_eq!(row.deadline_us, None);
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn a_manual_lease_writes_no_row_and_is_gone_after_reopen() {
    let mut rig = Rig::new();
    let held = rig.dispatch(json!("hold_until_off")).await.unwrap();
    assert_eq!(lease_count(&held), 1);
    assert_held_system(&rig);
    assert!(table_exists(&rig), "the lease table exists");
    assert!(rows(&rig).is_empty(), "a manual lease is not stored");

    rig.reopen().await;

    let state = awake_state(&rig);
    assert_eq!(lease_count(&state), 0);
    assert_eq!(state["system_held"], false);
    assert!(rows(&rig).is_empty());
    assert!(rig.power.held().is_empty());
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn closing_the_app_ends_manual_leases_and_releases_the_assertion() {
    let rig = Rig::new();
    rig.dispatch(json!({ "hold_for": { "seconds": 3600 } }))
        .await
        .unwrap();
    assert_held_system(&rig);

    rig.shutdown().await;

    assert!(
        rig.power.held().is_empty(),
        "closing the app releases the assertion"
    );
}

#[tokio::test(start_paused = true)]
async fn a_live_process_is_restored_and_the_assertion_returns() {
    let mut rig = Rig::new();
    open_session(&rig).await;
    assert_held_system(&rig);
    let lists = rig.processes.list_calls();

    rig.reopen().await;

    let state = awake_state(&rig);
    assert_eq!(lease_count(&state), 1);
    assert_eq!(state["system_held"], true);
    assert_eq!(state["leases"][0]["key"], format!("session:{SESSION}"));
    assert_eq!(state["leases"][0]["kind"], "session");
    assert_eq!(state["leases"][0]["harness"], "Claude Code");
    assert_eq!(state["leases"][0]["pid"], CLAUDE);
    assert_eq!(state["leases"][0]["stopped"], false);
    assert_eq!(state["leases"][0]["ends_at_ms"], ENDS_AT_MS);
    assert_eq!(state["since_ms"], ORIGIN_MS as i64);
    assert_held_system(&rig);
    assert_eq!(rows(&rig).len(), 1);
    assert!(
        rig.processes.list_calls() > lists,
        "a surviving row recomputes its label from one process snapshot"
    );
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn a_gone_process_deletes_its_row_and_never_holds() {
    let mut rig = Rig::new();
    command_process(&rig);
    rig.dispatch(hold_command(COMMAND_PID)).await.unwrap();
    assert_eq!(
        rows(&rig).len(),
        1,
        "the command lease is stored before close"
    );
    let lists = rig.processes.list_calls();

    rig.shutdown().await;
    rig.processes.exit(COMMAND_PID);
    rig.reopen().await;

    let state = awake_state(&rig);
    assert_eq!(lease_count(&state), 0);
    assert_eq!(state["system_held"], false);
    assert!(rows(&rig).is_empty(), "a gone process deletes its row");
    assert!(rig.power.held().is_empty());
    assert!(
        ended(&rig).is_empty(),
        "a deleted row never becomes a lease"
    );
    assert_eq!(rig.processes.list_calls(), lists);
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn a_reused_pid_deletes_its_row_and_never_holds() {
    let mut rig = Rig::new();
    command_process(&rig);
    rig.dispatch(hold_command(COMMAND_PID)).await.unwrap();
    assert_eq!(
        rows(&rig).len(),
        1,
        "the command lease is stored before close"
    );

    rig.shutdown().await;
    rig.processes.replace(COMMAND_PID, COMMAND_START + 50, 501);
    rig.reopen().await;

    let state = awake_state(&rig);
    assert_eq!(lease_count(&state), 0);
    assert!(rows(&rig).is_empty(), "a reused pid deletes its row");
    assert!(rig.power.held().is_empty());
    assert!(ended(&rig).is_empty());
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn a_zombie_deletes_its_row() {
    let mut rig = Rig::new();
    command_process(&rig);
    rig.dispatch(hold_command(COMMAND_PID)).await.unwrap();
    assert_eq!(
        rows(&rig).len(),
        1,
        "the command lease is stored before close"
    );

    rig.shutdown().await;
    rig.processes.exit(COMMAND_PID);
    rig.processes.add(
        FakeProcess::new(
            COMMAND_PID,
            1,
            501,
            COMMAND_START,
            "/tmp/feature-awake/sleep",
        )
        .zombie(),
    );
    rig.reopen().await;

    let state = awake_state(&rig);
    assert_eq!(lease_count(&state), 0);
    assert!(rows(&rig).is_empty(), "a zombie deletes its row");
    assert!(rig.power.held().is_empty());
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn an_expired_session_ends_on_reopen() {
    let mut rig = Rig::new();
    open_session(&rig).await;

    rig.shutdown().await;
    rig.clock.advance(TTL);
    rig.reopen().await;

    let state = awake_state(&rig);
    assert_eq!(lease_count(&state), 0);
    assert_eq!(state["system_held"], false);
    assert!(rows(&rig).is_empty());
    assert!(rig.power.held().is_empty());
    assert_eq!(
        ended(&rig),
        vec![(
            Severity::Info,
            "Stopped keeping this Mac awake: Claude Code reached its end time.".to_owned()
        )]
    );
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn a_passed_cap_ends_on_reopen_with_the_cap_event() {
    let mut rig = Rig::new();
    open_session(&rig).await;
    rig.dispatch(stop_session(SESSION)).await.unwrap();
    rig.processes.add(FakeProcess::new(
        200,
        CLAUDE,
        501,
        (FIRST_HOLD_US as u64) + 1,
        "/bin/sleep",
    ));

    rig.shutdown().await;
    rig.clock.advance(CAP);
    rig.reopen().await;

    let state = awake_state(&rig);
    assert_eq!(lease_count(&state), 0);
    assert_eq!(state["system_held"], false);
    assert!(rows(&rig).is_empty());
    assert!(rig.power.held().is_empty());
    assert_eq!(
        ended(&rig),
        vec![(
            Severity::Warn,
            "Stopped keeping this Mac awake: Claude Code was still busy 2 hours after it stopped."
                .to_owned()
        )]
    );
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn grace_counts_from_the_stored_stop_mark_after_reopen() {
    let mut rig = Rig::new();
    open_session(&rig).await;
    rig.dispatch(stop_session(SESSION)).await.unwrap();
    rig.advance(GRACE / 2).await;

    rig.reopen().await;

    let restored = awake_state(&rig);
    assert_eq!(lease_count(&restored), 1);
    assert_eq!(restored["leases"][0]["stopped"], true);
    assert_eq!(restored["system_held"], true);
    assert_held_system(&rig);
    assert!(ended(&rig).is_empty());

    rig.advance(GRACE / 2).await;
    let state = until(&rig, |state| lease_count(state) == 0).await;
    assert_eq!(state["system_held"], false);
    assert!(rows(&rig).is_empty());
    assert!(rig.power.held().is_empty());
    assert_eq!(
        ended(&rig),
        vec![(
            Severity::Info,
            "Stopped keeping this Mac awake: Claude Code stopped.".to_owned()
        )]
    );
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn a_restored_lease_ends_when_its_process_exits() {
    let mut rig = Rig::new();
    command_process(&rig);
    rig.dispatch(hold_command(COMMAND_PID)).await.unwrap();

    rig.reopen().await;
    assert_eq!(lease_count(&awake_state(&rig)), 1);
    assert_held_system(&rig);

    rig.exits.fire(COMMAND_PID);
    let state = until(&rig, |state| lease_count(state) == 0).await;
    assert_eq!(state["system_held"], false);
    assert!(rows(&rig).is_empty());
    assert!(rig.power.held().is_empty());
    assert_eq!(
        ended(&rig),
        vec![(
            Severity::Info,
            "Stopped keeping this Mac awake: sleep exited.".to_owned()
        )]
    );
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn release_all_deletes_every_row_and_reopen_restores_nothing() {
    let mut rig = Rig::new();
    open_session(&rig).await;
    command_process(&rig);
    rig.dispatch(hold_command(COMMAND_PID)).await.unwrap();
    assert_eq!(rows(&rig).len(), 2);

    let released = rig.dispatch(json!("release_all")).await.unwrap();
    assert_eq!(lease_count(&released), 0);
    assert_eq!(released["system_held"], false);
    assert!(rows(&rig).is_empty());
    assert!(rig.power.held().is_empty());

    rig.reopen().await;
    let state = awake_state(&rig);
    assert_eq!(lease_count(&state), 0);
    assert_eq!(state["system_held"], false);
    assert!(rows(&rig).is_empty());
    assert!(rig.power.held().is_empty());
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn a_failed_release_all_delete_changes_nothing_and_answers_failed() {
    let rig = Rig::new();
    open_session(&rig).await;
    assert_eq!(rows(&rig).len(), 1);
    drop_leases(&rig);

    let error = rig.dispatch(json!("release_all")).await.unwrap_err();
    assert!(
        matches!(error, DispatchError::Action(ActionError::Failed(_))),
        "{error}"
    );
    let state = awake_state(&rig);
    assert_eq!(lease_count(&state), 1);
    assert_eq!(state["leases"][0]["key"], format!("session:{SESSION}"));
    assert_eq!(state["system_held"], true);
    assert_held_system(&rig);
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn a_failed_row_write_keeps_the_lease_and_retries_on_the_timer() {
    let rig = Rig::new();
    claude(&rig);
    for _ in 0..50 {
        if table_exists(&rig) {
            break;
        }
        rig.idle().await;
    }
    assert!(table_exists(&rig), "the lease table exists before the drop");
    drop_leases(&rig);
    assert!(!table_exists(&rig), "the lease table was dropped");

    let held = rig.dispatch(hold_session(SESSION, CLAUDE)).await.unwrap();
    assert_eq!(lease_count(&held), 1);
    assert_eq!(held["system_held"], true);
    assert_held_system(&rig);

    create_leases(&rig);
    rig.idle().await;
    assert!(
        rows(&rig).is_empty(),
        "a failed write waits for the retry, it does not write on the next poll"
    );

    rig.advance(RETRY - Duration::from_secs(1)).await;
    rig.idle().await;
    assert!(rows(&rig).is_empty(), "the retry waits the full minute");
    assert_eq!(lease_count(&awake_state(&rig)), 1);

    rig.advance(Duration::from_secs(1)).await;
    let _ = until(&rig, |_| rows(&rig).len() == 1).await;
    let found = rows(&rig);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].key, format!("session:{SESSION}"));
    assert_eq!(found[0].pid, i64::from(CLAUDE));
    assert_eq!(found[0].first_hold_us, Some(FIRST_HOLD_US));
    assert_eq!(lease_count(&awake_state(&rig)), 1);
    assert_held_system(&rig);
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn an_empty_table_takes_no_snapshot_on_open() {
    let rig = Rig::new();
    rig.idle().await;
    rig.dispatch(json!("status")).await.unwrap();

    assert!(table_exists(&rig), "opening migrates the lease table");
    assert!(rows(&rig).is_empty());
    assert_eq!(rig.processes.list_calls(), 0);
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn a_row_holds_no_argv_environment_or_cwd() {
    let rig = Rig::new();
    rig.processes.add(
        FakeProcess::new(
            CLAUDE,
            1,
            501,
            CLAUDE_START,
            &format!("/opt/{EXE_MARK}/claude"),
        )
        .argv(&["claude", ARGV_MARK, &format!("{ENV_MARK}=1")])
        .cwd(format!("/tmp/{CWD_MARK}")),
    );
    rig.dispatch(hold_session(SESSION, CLAUDE)).await.unwrap();

    let names = columns(&rig);
    assert_eq!(
        names,
        vec![
            "key",
            "kind",
            "session_id",
            "harness",
            "pid",
            "process_start_us",
            "first_hold_us",
            "stop_mark_us",
            "deadline_us",
        ]
    );
    let found = rows(&rig);
    assert_eq!(found.len(), 1);
    let blob = found[0].cells().join("\n");
    assert!(!blob.contains(ARGV_MARK), "{blob}");
    assert!(!blob.contains(CWD_MARK), "{blob}");
    assert!(!blob.contains(ENV_MARK), "{blob}");
    assert!(!blob.contains(EXE_MARK), "{blob}");
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn the_assertion_name_holds_no_session_text() {
    let rig = Rig::new();
    let session_id = "sess-private-text";
    claude(&rig);
    rig.dispatch(hold_session(session_id, CLAUDE))
        .await
        .unwrap();

    assert_held_system(&rig);
    let name = &rig.power.held()[0].1;
    assert_eq!(name, ASSERTION_NAME);
    assert!(!name.contains(session_id), "{name}");
    assert!(!name.contains("claude"));
    rig.shutdown().await;
}

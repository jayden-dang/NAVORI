#![allow(clippy::unwrap_used)]

mod common;

use std::fs;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use common::{ORIGIN_MS, Rig};
use navori_macos::fake::FakeProcess;
use serde_json::{Value, json};

fn status() -> Value {
    json!("status")
}

fn hold(seconds: u64) -> Value {
    json!({ "hold_for": { "seconds": seconds } })
}

fn idle_state() -> Value {
    json!({
        "leases": [],
        "system_held": false,
        "display_held": false,
        "since_ms": null,
        "problem": null
    })
}

#[tokio::test(start_paused = true)]
async fn an_hour_with_no_lease_reads_nothing_and_calls_nothing() {
    let rig = Rig::new();
    let before = rig.dispatch(status()).await.unwrap();
    assert_eq!(before, idle_state());
    assert_eq!(before, rig.kernel.state("awake").unwrap());
    assert_eq!(rig.processes.list_calls(), 0);
    assert!(rig.processes.reads().is_empty());
    assert!(rig.power.held().is_empty());
    assert_eq!(rig.power.releases(), 0);
    assert!(rig.exits.watched().is_empty());

    rig.advance(Duration::from_secs(60 * 60)).await;
    rig.idle().await;

    let after = rig.dispatch(status()).await.unwrap();
    assert_eq!(after, idle_state());
    assert_eq!(rig.processes.list_calls(), 0);
    assert!(rig.processes.reads().is_empty());
    assert!(rig.power.held().is_empty());
    assert_eq!(rig.power.releases(), 0);
    assert!(rig.exits.watched().is_empty());
    rig.shutdown().await;
}

const CLAUDE: i32 = 100;
const SHELL: i32 = 101;
const HOOK: i32 = 102;
const SESSION: &str = "sess-1";

struct Checkout {
    path: PathBuf,
}

impl Checkout {
    fn new() -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("navori-idle-cwd-{nanos}"));
        let git = path.join(".git");
        fs::create_dir_all(&git).unwrap();
        fs::write(git.join("HEAD"), "ref: refs/heads/main\n").unwrap();
        Self { path }
    }
}

impl Drop for Checkout {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn hold_session(session_id: &str, pid: i32, ppid: i32) -> Value {
    json!({ "hold_session": { "session_id": session_id, "pid": pid, "ppid": ppid } })
}

fn lease_count(state: &Value) -> usize {
    state["leases"].as_array().map(Vec::len).unwrap_or(0)
}

#[tokio::test(start_paused = true)]
async fn opening_the_panel_takes_no_snapshot() {
    let checkout = Checkout::new();
    let rig = Rig::new();
    rig.processes.add(
        FakeProcess::new(CLAUDE, 1, 501, 100_000, "/usr/local/bin/claude")
            .cwd(checkout.path.clone()),
    );
    rig.processes
        .add(FakeProcess::new(SHELL, CLAUDE, 501, 101_000, "/bin/sh"));
    rig.processes.add(FakeProcess::new(
        HOOK,
        SHELL,
        501,
        102_000,
        "/usr/local/bin/navori",
    ));

    let held = rig
        .dispatch(hold_session(SESSION, HOOK, SHELL))
        .await
        .unwrap();
    assert_eq!(lease_count(&held), 1);
    assert_eq!(held["system_held"], true);
    assert_eq!(held["leases"][0]["kind"], "session");
    let lists = rig.processes.list_calls();
    assert!(lists > 0);
    let releases = rig.power.releases();

    rig.kernel.scheduler().set_panel_visible(true);
    rig.idle().await;
    rig.kernel.scheduler().set_panel_visible(false);
    rig.idle().await;

    let still = rig.dispatch(status()).await.unwrap();
    rig.idle().await;
    assert_eq!(rig.processes.list_calls(), lists);
    assert_eq!(lease_count(&still), 1);
    assert_eq!(still["system_held"], true);
    assert_eq!(rig.power.releases(), releases);
    assert_eq!(rig.power.held().len(), 1);
    rig.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn with_the_menu_closed_two_timer_wakes_are_30_seconds_apart() {
    let rig = Rig::new();
    assert!(!rig.kernel.scheduler().panel_visible());

    rig.dispatch(hold(1)).await.unwrap();
    rig.advance(Duration::from_secs(1)).await;
    rig.idle().await;
    assert_eq!(lease_count(&rig.kernel.state("awake").unwrap()), 0);
    assert!(rig.power.held().is_empty());

    let again = rig.dispatch(hold(1)).await.unwrap();
    assert_eq!(
        again["leases"][0]["ends_at_ms"].as_i64().unwrap(),
        ORIGIN_MS as i64 + 2_000
    );
    assert_eq!(rig.power.held().len(), 1);

    rig.advance(Duration::from_secs(29)).await;
    rig.idle().await;
    assert_eq!(
        lease_count(&rig.kernel.state("awake").unwrap()),
        1,
        "the second timer wake arrived before 30 seconds"
    );
    assert_eq!(rig.power.held().len(), 1);

    rig.advance(Duration::from_secs(1)).await;
    rig.idle().await;
    let ended = rig.kernel.state("awake").unwrap();
    assert_eq!(lease_count(&ended), 0);
    assert_eq!(ended["system_held"], false);
    assert!(rig.power.held().is_empty());
    assert_eq!(rig.power.releases(), 2);
    rig.shutdown().await;
}

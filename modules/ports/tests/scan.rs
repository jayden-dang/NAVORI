#![allow(clippy::unwrap_used)]

mod common;

use std::time::Duration;

use common::{ORIGIN_MS, Rig};
use navori_kernel::Risk;
use navori_macos::ReadError;
use navori_macos::fake::FakeProcess;
use navori_mod_ports::{
    DockerStatus, Placement, PortEntry, PortKind, ProjectGroup, WorktreeGroup, group,
};
use serde_json::{Value, json};

fn listener(pid: i32, exe: &str) -> FakeProcess {
    FakeProcess::new(pid, 1, 501, 10_000 + u64::try_from(pid).unwrap(), exe)
}

async fn scanned(rig: &Rig) -> Value {
    rig.advance(Duration::from_secs(5)).await;
    rig.settle().await;
    rig.dispatch(json!("list")).await.unwrap()
}

fn entry(pid: i32, port: u16, project: Option<&str>, worktree: Option<&str>) -> PortEntry {
    PortEntry {
        port,
        addresses: vec!["127.0.0.1".parse().unwrap()],
        pid,
        start_us: 1,
        name: format!("proc{pid}"),
        exe: None,
        place: Placement {
            project: project.map(str::to_owned),
            worktree: worktree.map(str::to_owned),
            branch: None,
            missing_path: None,
        },
        harness: None,
        kind: PortKind::Other,
        container: None,
    }
}

fn projects(groups: &[ProjectGroup]) -> Vec<Option<&str>> {
    groups.iter().map(|g| g.project.as_deref()).collect()
}

fn worktrees(group: &ProjectGroup) -> Vec<Option<&str>> {
    group
        .worktrees
        .iter()
        .map(|w: &WorktreeGroup| w.worktree.as_deref())
        .collect()
}

#[tokio::test(start_paused = true)]
async fn listing_returns_one_entry_per_process_and_port_with_both_addresses() {
    let rig = Rig::new();
    rig.processes.add(
        listener(100, "/usr/local/bin/node")
            .listen("127.0.0.1", 3000)
            .listen("::1", 3000),
    );
    let state = scanned(&rig).await;
    let entries = &state["groups"][0]["worktrees"][0]["processes"][0]["entries"];
    assert_eq!(entries.as_array().unwrap().len(), 1, "{state}");
    assert_eq!(entries[0]["port"], 3000);
    assert_eq!(entries[0]["pid"], 100);
    assert_eq!(entries[0]["addresses"], json!(["127.0.0.1", "::1"]));
    assert_eq!(entries[0]["name"], "node");
    assert_eq!(entries[0]["exe"], "/usr/local/bin/node");
    assert_eq!(entries[0]["start_us"], 10_100);
    assert_eq!(entries[0]["kind"], "other");
    assert_eq!(state["docker"], json!("not_seen"));
}

#[tokio::test(start_paused = true)]
async fn listing_before_any_scan_says_nothing_was_scanned_and_starts_no_scan() {
    let rig = Rig::new();
    rig.processes
        .add(listener(100, "/bin/node").listen("127.0.0.1", 3000));
    let state = rig.dispatch(json!("list")).await.unwrap();
    assert_eq!(state["scanned_at_ms"], Value::Null);
    assert_eq!(state["groups"], json!([]));
    assert_eq!(rig.processes.list_calls(), 0);
}

#[tokio::test(start_paused = true)]
async fn the_first_scan_comes_five_seconds_after_start() {
    let rig = Rig::new();
    rig.processes
        .add(listener(100, "/bin/node").listen("127.0.0.1", 3000));
    rig.advance(Duration::from_millis(4_999)).await;
    assert_eq!(rig.processes.list_calls(), 0);
    rig.advance(Duration::from_millis(1)).await;
    rig.settle().await;
    assert_eq!(rig.state()["scanned_at_ms"], ORIGIN_MS + 5_000);
}

#[tokio::test(start_paused = true)]
async fn listing_counts_a_process_whose_sockets_could_not_be_read() {
    let rig = Rig::new();
    rig.processes
        .add(listener(100, "/bin/node").listen("127.0.0.1", 3000));
    rig.processes
        .add(listener(200, "/bin/locked").fds_err(ReadError::Denied));
    let state = scanned(&rig).await;
    assert_eq!(state["skipped"], 1);
    let ports: Vec<_> = state["groups"][0]["worktrees"][0]["processes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["pid"].clone())
        .collect();
    assert_eq!(ports, vec![json!(100)]);
}

#[tokio::test(start_paused = true)]
async fn a_process_whose_executable_cannot_be_read_is_named_by_the_os_short_name() {
    let rig = Rig::new();
    rig.processes.add(
        listener(100, "/unused")
            .exe_err(ReadError::Denied)
            .name("node")
            .listen("127.0.0.1", 8080),
    );
    let state = scanned(&rig).await;
    let process = &state["groups"][0]["worktrees"][0]["processes"][0];
    assert_eq!(process["name"], "node");
    assert_eq!(process["exe"], Value::Null);
}

#[tokio::test(start_paused = true)]
async fn a_scan_that_cannot_list_processes_is_reported_as_a_problem() {
    let rig = Rig::new();
    rig.gate.deny(true);
    let state = scanned(&rig).await;
    assert_eq!(state["scanned_at_ms"], Value::Null);
    assert_eq!(
        state["problem"],
        "reading the process list failed (sysctl failed with errno 1; retry the process read); retry the snapshot"
    );
}

#[tokio::test(start_paused = true)]
async fn the_list_is_grouped_by_project_then_worktree_then_process_in_a_stable_order() {
    let entries = vec![
        entry(30, 9000, None, None),
        entry(20, 4000, Some("Zed"), Some("b")),
        entry(20, 4001, Some("alpha"), None),
        entry(10, 3000, Some("Zed"), None),
        entry(10, 80, Some("Zed"), None),
        entry(40, 5000, Some("Beta"), Some("a")),
        entry(41, 5001, Some("Beta"), Some("A")),
    ];
    let mut reversed = entries.clone();
    reversed.reverse();
    let grouped = group(entries);
    assert_eq!(grouped, group(reversed));
    assert_eq!(
        projects(&grouped),
        vec![Some("alpha"), Some("Beta"), Some("Zed"), None]
    );
    assert_eq!(worktrees(&grouped[1]), vec![Some("A"), Some("a")]);
    assert_eq!(worktrees(&grouped[2]), vec![None, Some("b")]);
    let zed = &grouped[2].worktrees[0].processes;
    assert_eq!(zed.len(), 1);
    assert_eq!(zed[0].pid, 10);
    let ports: Vec<u16> = zed[0].entries.iter().map(|e| e.port).collect();
    assert_eq!(ports, vec![80, 3000]);
}

#[tokio::test(start_paused = true)]
async fn processes_without_a_project_are_listed_by_pid_and_their_ports_in_order() {
    let rig = Rig::new();
    rig.processes.add(
        listener(300, "/bin/b")
            .listen("127.0.0.1", 9000)
            .listen("127.0.0.1", 80),
    );
    rig.processes
        .add(listener(200, "/bin/a").listen("127.0.0.1", 5000));
    let state = scanned(&rig).await;
    let processes = state["groups"][0]["worktrees"][0]["processes"]
        .as_array()
        .unwrap();
    let pids: Vec<_> = processes.iter().map(|p| p["pid"].clone()).collect();
    assert_eq!(pids, vec![json!(200), json!(300)]);
    let ports: Vec<_> = processes[1]["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["port"].clone())
        .collect();
    assert_eq!(ports, vec![json!(80), json!(9000)]);
    assert_eq!(state["groups"][0]["project"], Value::Null);
}

#[tokio::test(start_paused = true)]
async fn list_is_a_read_action_with_a_schema_and_description() {
    let rig = Rig::new();
    let specs: Vec<_> = rig
        .kernel
        .actions()
        .into_iter()
        .filter(|spec| spec.module == "ports")
        .collect();
    assert_eq!(specs.len(), 1);
    assert_eq!(specs[0].name, "list");
    assert_eq!(specs[0].risk, Risk::Read);
    assert!(specs[0].description.contains(' ') && specs[0].description.ends_with('.'));
    assert!(specs[0].input_schema.to_string().contains("list"));
}

#[tokio::test(start_paused = true)]
async fn the_list_states_when_its_scan_was_taken() {
    let rig = Rig::new();
    rig.processes
        .add(listener(100, "/bin/node").listen("127.0.0.1", 3000));
    rig.advance(Duration::from_secs(5)).await;
    rig.settle().await;
    rig.advance(Duration::from_secs(2)).await;
    let state = rig.dispatch(json!("list")).await.unwrap();
    assert_eq!(state["scanned_at_ms"], 1_700_000_005_000_u64);
    assert_eq!(state["problem"], Value::Null);
}

#[test]
fn an_empty_scan_has_no_groups_and_docker_not_seen() {
    assert!(group(Vec::new()).is_empty());
    assert_eq!(DockerStatus::default(), DockerStatus::NotSeen);
}

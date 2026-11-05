//! The Running view, composed from fixture Awake and Ports states.
#![allow(clippy::unwrap_used)]

use std::path::PathBuf;

use navori_ffi::running::{Own, ProjectState, RunningProject, RunningView, StopTargetView, build};
use navori_ffi::{AwakeView, LeaseView};
use navori_mod_ports::PortsState;
use serde_json::{Value, json};

const HELPER: &str = "/Applications/Navori.app/Contents/Helpers/navori";
const AWAKE_UNREADABLE: &str = "Awake cannot be read, so Stop is off.";
const TRACKING_OFF: &str = "Tracking agents is off, so Stop is off.";
const NO_LABEL: &str = "A session has no project label, so Stop is off.";

fn own() -> Own {
    Own {
        pid: 1,
        helper: Some(PathBuf::from(HELPER)),
    }
}

fn session(label: Option<&str>, stopped: bool) -> LeaseView {
    LeaseView {
        key: format!("session-{}", label.unwrap_or("none")),
        kind: "session".into(),
        harness: Some("claude_code".into()),
        label: label.map(str::to_owned),
        pid: Some(900),
        stopped,
        ends_at_ms: None,
        title: None,
    }
}

fn awake(leases: Vec<LeaseView>) -> AwakeView {
    AwakeView {
        leases,
        system_held: false,
        display_held: false,
        since_ms: None,
        problem: None,
        tracking: true,
    }
}

fn port(port: u16, pid: i32) -> Value {
    json!({
        "port": port,
        "addresses": ["127.0.0.1"],
        "pid": pid,
        "start_us": 1_000_000_u64,
        "name": "node",
        "exe": null,
        "project": null,
        "worktree": null,
        "branch": null,
        "missing_path": null,
        "root": null,
        "harness": null,
        "kind": "dev_server",
        "container": null
    })
}

fn with(mut value: Value, key: &str, to: Value) -> Value {
    value[key] = to;
    value
}

fn in_project(entry: Value, project: &str) -> Value {
    with(entry, "project", json!(project))
}

fn database(entry: Value) -> Value {
    with(entry, "kind", json!("database"))
}

fn container(entry: Value, id: &str, image: &str) -> Value {
    let entry = with(entry, "kind", json!("docker"));
    with(
        entry,
        "container",
        json!({"id": id, "name": format!("c-{id}"), "image": image}),
    )
}

fn state(entries: Vec<Value>) -> PortsState {
    serde_json::from_value(json!({
        "scanned_at_ms": 1_700_000_000_000_u64,
        "skipped": 0,
        "docker": "read",
        "problem": null,
        "groups": [{
            "project": null,
            "worktrees": [{
                "worktree": null,
                "processes": [{
                    "pid": 1,
                    "start_us": 1,
                    "name": "x",
                    "exe": null,
                    "harness": null,
                    "entries": entries
                }]
            }]
        }]
    }))
    .unwrap()
}

fn view_of(awake: Option<&AwakeView>, ports: Option<&PortsState>) -> RunningView {
    build(awake, ports, own()).unwrap()
}

fn project<'a>(view: &'a RunningView, name: &str) -> &'a RunningProject {
    view.projects
        .iter()
        .find(|project| project.name == name && !project.is_other)
        .unwrap_or_else(|| panic!("no project {name}"))
}

fn other(view: &RunningView) -> &RunningProject {
    view.projects.iter().find(|p| p.is_other).unwrap()
}

fn names(view: &RunningView) -> Vec<&str> {
    view.projects.iter().map(|p| p.name.as_str()).collect()
}

fn json_of(request: &Option<String>) -> Value {
    serde_json::from_str(request.as_deref().unwrap()).unwrap()
}

fn kill(selector: Value, targets: Value) -> Value {
    json!({"kill": {"selector": selector, "targets": targets}})
}

fn process(pid: i32) -> Value {
    json!({"process": {"pid": pid, "start_us": 1_000_000_u64}})
}

fn assert_stop_is_off(view: &RunningView) {
    assert!(!view.projects.is_empty());
    for project in &view.projects {
        assert_eq!(project.state, ProjectState::Unknown, "{}", project.name);
        assert_eq!(project.stoppable_ports, 0, "{}", project.name);
        assert_eq!(project.child_stop_request, None, "{}", project.name);
        for listed in &project.ports {
            assert_eq!(listed.stop_request, "", "{}", project.name);
        }
    }
    assert!(view.stop_all.is_empty());
    assert_eq!(view.stop_all_ports, 0);
}

fn dev_servers() -> PortsState {
    state(vec![in_project(port(3000, 10), "klynt")])
}

#[test]
fn a_label_joins_the_project_before_its_first_slash() {
    let cases: Vec<Value> =
        serde_json::from_str(include_str!("fixtures/lease_labels.json")).unwrap();
    for case in cases {
        let label = case["label"].as_str();
        let lease = session(label, false);
        let view = view_of(Some(&awake(vec![lease])), Some(&PortsState::default()));
        match case["project"].as_str() {
            Some(expected) => {
                assert_eq!(names(&view), [expected], "label {label:?}");
                assert_eq!(view.projects[0].state, ProjectState::Working);
                assert_eq!(view.unknown_reason, None);
            }
            None => {
                assert_eq!(
                    view.unknown_reason.as_deref(),
                    Some(NO_LABEL),
                    "label {label:?}"
                );
                assert!(view.projects.is_empty());
            }
        }
    }
}

#[test]
fn a_session_lease_with_a_working_flag_makes_its_project_working() {
    let mut running = session(Some("klynt/feature/x"), false);
    running.title = Some("Fix the build".into());
    let finished = session(Some("zed/main"), true);
    let ports = state(vec![in_project(port(4000, 20), "zed")]);
    let view = view_of(Some(&awake(vec![running, finished])), Some(&ports));
    let klynt = project(&view, "klynt");
    assert_eq!(klynt.state, ProjectState::Working);
    assert_eq!(klynt.sessions.len(), 1);
    assert_eq!(klynt.sessions[0].harness.as_deref(), Some("claude_code"));
    assert_eq!(klynt.sessions[0].title.as_deref(), Some("Fix the build"));
    assert_eq!(klynt.sessions[0].branch.as_deref(), Some("feature/x"));
    assert!(!klynt.sessions[0].stopped);
    let zed = project(&view, "zed");
    assert_eq!(zed.state, ProjectState::Idle);
    assert!(zed.sessions[0].stopped);
}

#[test]
fn an_unknown_state_carries_its_reason_sentence_per_case_and_none_while_known() {
    let ports = dev_servers();
    let mut problem = awake(vec![]);
    problem.problem = Some("The Awake module stopped.".into());
    let mut untracked = awake(vec![]);
    untracked.tracking = false;
    let unlabeled = awake(vec![session(None, false)]);

    let reason = |view: RunningView| view.unknown_reason;
    assert_eq!(
        reason(view_of(Some(&problem), Some(&ports))).as_deref(),
        Some("The Awake module stopped.")
    );
    assert_eq!(
        reason(view_of(None, Some(&ports))).as_deref(),
        Some(AWAKE_UNREADABLE)
    );
    assert_eq!(
        reason(view_of(Some(&untracked), Some(&ports))).as_deref(),
        Some(TRACKING_OFF)
    );
    assert_eq!(
        reason(view_of(Some(&unlabeled), Some(&ports))).as_deref(),
        Some(NO_LABEL)
    );
    assert_eq!(reason(view_of(Some(&awake(vec![])), Some(&ports))), None);
}

#[test]
fn every_way_awake_can_be_unknown_turns_stop_off_and_says_why() {
    let with_problem = |problem: &str| {
        let mut broken = awake(vec![]);
        broken.problem = Some(problem.into());
        broken
    };
    let mut untracked = awake(vec![]);
    untracked.tracking = false;
    let ports_with_other = state(vec![in_project(port(3000, 10), "klynt"), port(3001, 11)]);
    let cases: Vec<(&str, Option<AwakeView>, PortsState, &str)> = vec![
        ("not registered", None, dev_servers(), AWAKE_UNREADABLE),
        (
            "state that did not deserialize",
            None,
            ports_with_other,
            AWAKE_UNREADABLE,
        ),
        (
            "problem set",
            Some(with_problem("Awake cannot reach the power service.")),
            dev_servers(),
            "Awake cannot reach the power service.",
        ),
        (
            "module down",
            Some(with_problem("The Awake module stopped.")),
            dev_servers(),
            "The Awake module stopped.",
        ),
        ("tracking off", Some(untracked), dev_servers(), TRACKING_OFF),
        (
            "a live session without a label",
            Some(awake(vec![
                session(Some("klynt/main"), false),
                session(None, false),
            ])),
            dev_servers(),
            NO_LABEL,
        ),
    ];
    for (case, awake_view, ports, reason) in cases {
        let view = view_of(awake_view.as_ref(), Some(&ports));
        assert_eq!(view.unknown_reason.as_deref(), Some(reason), "{case}");
        assert_stop_is_off(&view);
    }
    let ports = state(vec![in_project(port(3000, 10), "klynt"), port(3001, 11)]);
    assert_eq!(names(&view_of(None, Some(&ports))), ["klynt", "Other"]);
}

#[test]
fn both_awake_and_ports_down_gives_no_running_view() {
    let mut down = awake(vec![]);
    down.problem = Some("The Awake module stopped.".into());
    assert_eq!(build(Some(&down), None, own()), None);
    assert_eq!(build(None, None, own()), None);
}

#[test]
fn awake_down_with_ports_up_and_ports_down_with_awake_up_still_give_a_view() {
    let mut down = awake(vec![]);
    down.problem = Some("The Awake module stopped.".into());
    assert!(build(Some(&down), Some(&dev_servers()), own()).is_some());
    assert!(build(Some(&awake(vec![])), None, own()).is_some());
    let mut broken = awake(vec![]);
    broken.problem = Some("Awake cannot reach the power service.".into());
    assert!(build(Some(&broken), None, own()).is_some());
}

#[test]
fn a_project_with_no_working_session_is_idle() {
    let view = view_of(Some(&awake(vec![])), Some(&dev_servers()));
    assert_eq!(project(&view, "klynt").state, ProjectState::Idle);
}

#[test]
fn the_ports_module_down_gives_no_stop_requests() {
    let leases = vec![
        session(Some("klynt/main"), false),
        session(Some("zed/x"), true),
    ];
    let view = view_of(Some(&awake(leases)), None);
    assert_eq!(names(&view), ["zed", "klynt"]);
    for project in &view.projects {
        assert!(project.ports.is_empty());
        assert_eq!(project.stoppable_ports, 0);
        assert_eq!(project.child_stop_request, None);
    }
    assert!(view.stop_all.is_empty());
}

#[test]
fn idle_and_unknown_projects_come_first_then_case_insensitive_name_then_other_last() {
    let ports = state(vec![
        in_project(port(3001, 11), "Bravo"),
        in_project(port(3002, 12), "alpha"),
        in_project(port(3003, 13), "aaa"),
        port(3004, 14),
    ]);
    let known = view_of(
        Some(&awake(vec![session(Some("aaa/main"), false)])),
        Some(&ports),
    );
    assert_eq!(names(&known), ["alpha", "Bravo", "aaa", "Other"]);

    let unknown = view_of(None, Some(&ports));
    assert_eq!(names(&unknown), ["aaa", "alpha", "Bravo", "Other"]);
}

#[test]
fn a_database_and_a_system_port_are_left_out_of_the_group_requests_but_have_their_own_pid_request()
{
    let ports = state(vec![
        in_project(port(3000, 10), "klynt"),
        in_project(database(port(5432, 11)), "klynt"),
        in_project(with(port(22, 12), "kind", json!("system")), "klynt"),
    ]);
    let view = view_of(Some(&awake(vec![])), Some(&ports));
    let klynt = project(&view, "klynt");
    let group = kill(json!({"project": "klynt"}), json!([process(10)]));
    assert_eq!(json_of(&klynt.child_stop_request), group);
    assert_eq!(klynt.stoppable_ports, 1);
    assert_eq!(view.hidden_system_ports, 2);
    let by_port = |number: u16| klynt.ports.iter().find(|p| p.port == number).unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&by_port(5432).stop_request).unwrap(),
        kill(json!({"pid": 11}), json!([process(11)]))
    );
    assert!(by_port(5432).protected_in_group);
    assert!(by_port(22).protected_in_group);
    assert!(!by_port(3000).protected_in_group);
    assert_eq!(
        by_port(22).target,
        StopTargetView::Process {
            pid: 12,
            start_us: 1_000_000
        }
    );
}

#[test]
fn a_container_is_in_the_group_request_and_a_database_image_container_is_not() {
    let ports = state(vec![
        in_project(container(port(8080, 50), "ab12", "node"), "klynt"),
        in_project(container(port(5433, 50), "cd34", "postgres"), "klynt"),
    ]);
    let view = view_of(Some(&awake(vec![])), Some(&ports));
    let klynt = project(&view, "klynt");
    assert_eq!(
        json_of(&klynt.child_stop_request),
        kill(
            json!({"project": "klynt"}),
            json!([{"container": {"id": "ab12"}}])
        )
    );
    let database = klynt.ports.iter().find(|p| p.port == 5433).unwrap();
    assert!(database.container && database.protected_in_group);
    assert_eq!(
        database.target,
        StopTargetView::Container { id: "cd34".into() }
    );
    assert_eq!(database.stop_request, "");
    let plain = klynt.ports.iter().find(|p| p.port == 8080).unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&plain.stop_request).unwrap(),
        kill(
            json!({"project": "klynt"}),
            json!([{"container": {"id": "ab12"}}])
        )
    );
    assert_eq!(view.hidden_system_ports, 1);
}

#[test]
fn an_unclaimed_docker_entry_has_no_stop_request() {
    let ports = state(vec![
        in_project(port(3000, 10), "klynt"),
        in_project(with(port(5000, 60), "kind", json!("docker")), "klynt"),
    ]);
    let view = view_of(Some(&awake(vec![])), Some(&ports));
    let klynt = project(&view, "klynt");
    let backend = klynt.ports.iter().find(|p| p.port == 5000).unwrap();
    assert_eq!(backend.stop_request, "");
    assert_eq!(klynt.stoppable_ports, 1);
    assert_eq!(
        json_of(&klynt.child_stop_request),
        kill(json!({"project": "klynt"}), json!([process(10)]))
    );
}

#[test]
fn navori_itself_and_its_helper_have_no_stop_request() {
    let helper = with(port(7000, 70), "exe", json!(HELPER));
    let ports = state(vec![
        in_project(port(3000, 10), "klynt"),
        in_project(port(6000, 1), "klynt"),
        in_project(helper, "klynt"),
    ]);
    let view = view_of(Some(&awake(vec![])), Some(&ports));
    let klynt = project(&view, "klynt");
    assert_eq!(klynt.stoppable_ports, 1);
    for number in [6000, 7000] {
        let listed = klynt.ports.iter().find(|p| p.port == number).unwrap();
        assert_eq!(listed.stop_request, "");
    }
}

#[test]
fn stop_all_holds_each_idle_named_project_with_a_stoppable_port_and_never_other() {
    let ports = state(vec![
        in_project(port(3000, 10), "alpha"),
        in_project(port(3001, 11), "beta"),
        in_project(database(port(5432, 12)), "gamma"),
        port(3002, 13),
    ]);
    let view = view_of(
        Some(&awake(vec![session(Some("beta/main"), false)])),
        Some(&ports),
    );
    assert_eq!(view.stop_all.len(), 1);
    assert_eq!(
        serde_json::from_str::<Value>(&view.stop_all[0]).unwrap(),
        kill(json!({"project": "alpha"}), json!([process(10)]))
    );
    assert_eq!(
        json_of(&other(&view).child_stop_request),
        kill(json!({"project": null}), json!([process(13)]))
    );
}

#[test]
fn the_counts_equal_the_ports_in_the_requests_sent() {
    let ports = state(vec![
        in_project(port(3000, 10), "alpha"),
        in_project(port(3001, 10), "alpha"),
        in_project(container(port(8080, 50), "ab12", "node"), "alpha"),
        in_project(container(port(8081, 50), "ab12", "node"), "alpha"),
        in_project(port(3100, 11), "beta"),
        in_project(port(3101, 12), "beta"),
        in_project(port(3200, 13), "busy"),
    ]);
    let view = view_of(
        Some(&awake(vec![session(Some("busy/main"), false)])),
        Some(&ports),
    );
    assert_eq!(view.stop_all.len(), 2);
    assert_eq!(view.stop_all_ports, 6);
    assert_eq!(view.in_use_ports, 1);
}

#[test]
fn other_ports_left_counts_the_ports_in_other() {
    let ports = state(vec![
        in_project(port(3000, 10), "alpha"),
        port(3001, 11),
        port(3002, 12),
        database(port(5432, 13)),
    ]);
    let view = view_of(Some(&awake(vec![])), Some(&ports));
    assert_eq!(view.other_ports_left, 2);
    assert_eq!(other(&view).stoppable_ports, 2);
}

#[test]
fn the_view_carries_the_project_path_and_the_oldest_listeners_start() {
    let newer = with(
        with(port(3000, 10), "start_us", json!(3_000_000_000_u64)),
        "root",
        json!("/Users/me/klynt-new"),
    );
    let older = with(
        with(port(3001, 11), "start_us", json!(1_500_000_000_u64)),
        "root",
        json!("/Users/me/klynt"),
    );
    let ports = state(vec![in_project(newer, "klynt"), in_project(older, "klynt")]);
    let view = view_of(Some(&awake(vec![])), Some(&ports));
    let klynt = project(&view, "klynt");
    assert_eq!(klynt.path.as_deref(), Some("/Users/me/klynt"));
    assert_eq!(klynt.up_since_ms, Some(1_500_000));

    let sessions_only = view_of(
        Some(&awake(vec![session(Some("zed/x"), false)])),
        Some(&ports),
    );
    let zed = project(&sessions_only, "zed");
    assert_eq!(zed.path, None);
    assert_eq!(zed.up_since_ms, None);
}

#[test]
fn a_port_row_carries_its_worktree_and_harness_and_a_docker_port_carries_no_harness() {
    let dev = with(
        with(port(3000, 10), "worktree", json!("klynt-feature")),
        "harness",
        json!("claude_code"),
    );
    let docker = with(
        container(port(8080, 50), "ab12", "node"),
        "harness",
        json!("codex"),
    );
    let ports = state(vec![in_project(dev, "klynt"), in_project(docker, "klynt")]);
    let view = view_of(Some(&awake(vec![])), Some(&ports));
    let klynt = project(&view, "klynt");
    let dev = klynt.ports.iter().find(|p| p.port == 3000).unwrap();
    assert_eq!(dev.worktree.as_deref(), Some("klynt-feature"));
    assert_eq!(dev.harness.as_deref(), Some("claude_code"));
    let docker = klynt.ports.iter().find(|p| p.port == 8080).unwrap();
    assert_eq!(docker.harness, None);
    assert_eq!(docker.name, "c-ab12");
}

#[test]
fn two_ports_of_one_process_are_one_target_and_count_as_two_ports() {
    let ports = state(vec![
        in_project(port(3000, 10), "klynt"),
        in_project(port(3001, 10), "klynt"),
    ]);
    let view = view_of(Some(&awake(vec![])), Some(&ports));
    let klynt = project(&view, "klynt");
    assert_eq!(klynt.stoppable_ports, 2);
    assert_eq!(
        json_of(&klynt.child_stop_request),
        kill(json!({"project": "klynt"}), json!([process(10)]))
    );
}

#[test]
fn a_repo_literally_named_other_stays_a_named_project_not_the_no_project_group() {
    let ports = state(vec![in_project(port(3000, 10), "Other"), port(3001, 11)]);
    let view = view_of(Some(&awake(vec![])), Some(&ports));
    let named = project(&view, "Other");
    assert!(!named.is_other);
    assert_eq!(
        json_of(&named.child_stop_request),
        kill(json!({"project": "Other"}), json!([process(10)]))
    );
    assert_eq!(view.projects.iter().filter(|p| p.is_other).count(), 1);
    assert!(view.projects.last().unwrap().is_other);
    assert_eq!(view.stop_all.len(), 1);
}

#[test]
fn a_child_stop_request_exists_for_every_known_project_and_no_unknown_one() {
    let ports = state(vec![
        in_project(port(3000, 10), "busy"),
        in_project(port(3001, 11), "calm"),
    ]);
    let known = view_of(
        Some(&awake(vec![session(Some("busy/main"), false)])),
        Some(&ports),
    );
    let busy = project(&known, "busy");
    assert!(busy.child_stop_request.is_some());
    let calm = project(&known, "calm");
    assert!(calm.child_stop_request.is_some());

    let unknown = view_of(None, Some(&ports));
    for project in &unknown.projects {
        assert_eq!(project.child_stop_request, None);
    }
}

#[test]
fn openable_is_true_for_a_listed_port_that_is_not_a_database_or_system_port() {
    let ports = state(vec![
        in_project(port(3000, 10), "klynt"),
        in_project(with(port(3001, 11), "kind", json!("other")), "klynt"),
        in_project(container(port(8080, 50), "ab12", "node"), "klynt"),
        in_project(database(port(5432, 12)), "klynt"),
        in_project(with(port(22, 13), "kind", json!("system")), "klynt"),
    ]);
    let view = view_of(Some(&awake(vec![])), Some(&ports));
    let klynt = project(&view, "klynt");
    let openable = |number: u16| {
        klynt
            .ports
            .iter()
            .find(|p| p.port == number)
            .unwrap()
            .openable
    };
    assert!(openable(3000) && openable(3001) && openable(8080));
    assert!(!openable(5432) && !openable(22));
}

#[test]
fn nothing_to_build_from_gives_no_view() {
    assert!(build(None, None, own()).is_none());
}

#[test]
fn the_oldest_listeners_start_is_read_as_microseconds_since_the_epoch() {
    let started = with(port(3000, 10), "start_us", json!(1_730_000_000_123_456_u64));
    let ports = state(vec![in_project(started, "klynt")]);
    let view = view_of(Some(&awake(vec![])), Some(&ports));
    assert_eq!(project(&view, "klynt").up_since_ms, Some(1_730_000_000_123));
}

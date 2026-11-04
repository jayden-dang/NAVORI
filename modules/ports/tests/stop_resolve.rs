#![allow(clippy::unwrap_used)]

mod common;

use std::path::PathBuf;
use std::sync::Arc;

use navori_macos::ReadError;
use navori_macos::fake::{FakeProcess, FakeProcesses, FakeSignals};
use navori_mod_ports::{
    ContainerPlan, DockerStatus, FreshRead, Guard, Outcome, Placement, Plan, PortEntry, PortKind,
    ProcessPlan, Reach, Resolved, Scan, Selector, StopClass, Target, resolve, stop_class,
};

const START_US: u64 = 1_730_000_000_000_000;
const USER: u32 = 501;
const OWN: i32 = 100;
const PARENT: i32 = 90;
const HELPER: &str = "/Applications/Navori.app/Contents/Helpers/navori";
const BACKEND: &str = "/Applications/Docker.app/Contents/MacOS/com.docker.backend";
const CONTAINER: &str = "4f2a9c0d11ee";

struct World {
    processes: Arc<FakeProcesses>,
    guard: Guard,
}

fn world() -> World {
    let processes = Arc::new(FakeProcesses::new(USER));
    processes.add(FakeProcess::new(1, 0, 0, START_US, "/sbin/launchd"));
    processes.add(FakeProcess::new(PARENT, 1, USER, START_US, "/bin/zsh"));
    processes.add(FakeProcess::new(OWN, PARENT, USER, START_US, "/navori"));
    let signals = Arc::new(FakeSignals::new(OWN, OWN));
    let guard = Guard::new(
        signals,
        Arc::clone(&processes) as Arc<dyn navori_macos::ProcessSource>,
        Some(PathBuf::from(HELPER)),
    );
    World { processes, guard }
}

fn live(world: &World, pid: i32, exe: &str) {
    world
        .processes
        .add(FakeProcess::new(pid, 1, USER, START_US, exe));
}

fn entry(pid: i32, port: u16, exe: &str, kind: PortKind, project: Option<&str>) -> PortEntry {
    PortEntry {
        port,
        addresses: vec!["127.0.0.1".parse().unwrap()],
        pid,
        start_us: START_US,
        name: "server".to_owned(),
        exe: Some(PathBuf::from(exe)),
        place: Placement {
            project: project.map(str::to_owned),
            worktree: project.map(|_| "main".to_owned()),
            ..Placement::default()
        },
        harness: None,
        kind,
        container: None,
    }
}

fn dev(pid: i32, port: u16) -> PortEntry {
    entry(
        pid,
        port,
        "/usr/local/bin/node",
        PortKind::DevServer,
        Some("klynt"),
    )
}

fn container_entry(id: &str, port: u16, image: &str, project: Option<&str>) -> PortEntry {
    let mut held = entry(700, port, BACKEND, PortKind::Docker, project);
    held.container = Some(navori_mod_ports::ContainerRef {
        id: id.to_owned(),
        name: "web".to_owned(),
        image: Some(image.to_owned()),
    });
    held
}

fn fresh(entries: Vec<PortEntry>) -> FreshRead {
    FreshRead {
        scan: Scan::default(),
        entries,
        docker: DockerStatus::Read,
    }
}

fn process(pid: i32) -> Target {
    Target::Process {
        pid,
        start_us: START_US,
    }
}

fn container(id: &str) -> Target {
    Target::Container { id: id.to_owned() }
}

fn project(name: &str) -> Selector {
    Selector::Project(Some(name.to_owned()))
}

fn one(resolved: Vec<Resolved>) -> Resolved {
    assert_eq!(resolved.len(), 1);
    resolved.into_iter().next().unwrap()
}

fn refused(outcome: Outcome, reason: &'static str) -> Resolved {
    Resolved::Refused(outcome, Some(reason))
}

fn go_process(pid: i32) -> Resolved {
    Resolved::Go(Plan::Process(ProcessPlan {
        pid,
        start_us: START_US,
        members: vec![(pid, START_US)],
        reach: Reach::Alone,
    }))
}

#[test]
fn a_gone_pid_is_already_gone() {
    let world = world();
    let read = fresh(vec![dev(200, 3000)]);
    let got = one(resolve(
        &read,
        &world.guard,
        &Selector::Pid(200),
        &[process(200)],
    ));
    assert_eq!(got, Resolved::Refused(Outcome::AlreadyGone, None));
}

#[test]
fn a_zombie_is_already_gone() {
    let world = world();
    world
        .processes
        .add(FakeProcess::new(200, 1, USER, START_US, "/usr/local/bin/node").zombie());
    let read = fresh(vec![dev(200, 3000)]);
    let got = one(resolve(
        &read,
        &world.guard,
        &Selector::Pid(200),
        &[process(200)],
    ));
    assert_eq!(got, Resolved::Refused(Outcome::AlreadyGone, None));
}

#[test]
fn a_reused_pid_with_another_start_time_is_stale() {
    let world = world();
    world.processes.add(FakeProcess::new(
        200,
        1,
        USER,
        START_US + 5,
        "/usr/local/bin/node",
    ));
    let read = fresh(vec![dev(200, 3000)]);
    let got = one(resolve(
        &read,
        &world.guard,
        &Selector::Pid(200),
        &[process(200)],
    ));
    assert_eq!(got, refused(Outcome::Stale, "The process changed"));
}

#[test]
fn a_pid_of_another_user_is_stale() {
    let world = world();
    world.processes.add(FakeProcess::new(
        200,
        1,
        USER + 1,
        START_US,
        "/usr/local/bin/node",
    ));
    let read = fresh(vec![dev(200, 3000)]);
    let got = one(resolve(
        &read,
        &world.guard,
        &Selector::Pid(200),
        &[process(200)],
    ));
    assert_eq!(got, refused(Outcome::Stale, "The process changed"));
}

#[test]
fn a_pid_that_stopped_listening_is_stale_with_the_port_closed_sentence() {
    let world = world();
    live(&world, 200, "/usr/local/bin/node");
    let read = fresh(vec![dev(201, 3001)]);
    let got = one(resolve(
        &read,
        &world.guard,
        &Selector::Pid(200),
        &[process(200)],
    ));
    assert_eq!(got, refused(Outcome::Stale, "The port closed"));
}

#[test]
fn a_live_listener_under_its_project_is_planned_alone() {
    let world = world();
    live(&world, 200, "/usr/local/bin/node");
    let read = fresh(vec![dev(200, 3000)]);
    let got = one(resolve(
        &read,
        &world.guard,
        &project("klynt"),
        &[process(200)],
    ));
    assert_eq!(got, go_process(200));
}

#[test]
fn navoris_own_pid_is_protected_under_every_selector() {
    let world = world();
    let read = fresh(vec![dev(OWN, 3000)]);
    for selector in [
        Selector::Pid(OWN),
        project("klynt"),
        Selector::Worktree {
            project: "klynt".to_owned(),
            worktree: "main".to_owned(),
        },
    ] {
        let got = one(resolve(&read, &world.guard, &selector, &[process(OWN)]));
        assert_eq!(
            got,
            refused(Outcome::Protected, "This process is protected"),
            "{selector:?}"
        );
    }
}

#[test]
fn an_ancestor_of_navori_is_protected_under_every_selector() {
    let world = world();
    let read = fresh(vec![dev(PARENT, 3000)]);
    for selector in [Selector::Pid(PARENT), project("klynt")] {
        let got = one(resolve(&read, &world.guard, &selector, &[process(PARENT)]));
        assert_eq!(
            got,
            refused(Outcome::Protected, "This process is protected"),
            "{selector:?}"
        );
    }
}

#[test]
fn the_helper_by_exe_path_is_protected() {
    let world = world();
    live(&world, 200, HELPER);
    let read = fresh(vec![entry(200, 3000, HELPER, PortKind::Other, None)]);
    let got = one(resolve(
        &read,
        &world.guard,
        &Selector::Pid(200),
        &[process(200)],
    ));
    assert_eq!(
        got,
        refused(Outcome::Protected, "This process is protected")
    );
}

#[test]
fn an_unclaimed_docker_backend_entry_is_protected() {
    let world = world();
    live(&world, 700, BACKEND);
    let read = fresh(vec![entry(700, 5432, BACKEND, PortKind::Docker, None)]);
    assert_eq!(
        stop_class(&read.entries[0], OWN, Some(&PathBuf::from(HELPER))),
        StopClass::Never
    );
    let got = one(resolve(
        &read,
        &world.guard,
        &Selector::Pid(700),
        &[process(700)],
    ));
    assert_eq!(
        got,
        refused(Outcome::Protected, "This process is protected")
    );
}

#[test]
fn a_container_entry_whose_pid_is_the_backend_is_free_under_stop_class() {
    let held = container_entry(CONTAINER, 8080, "nginx", Some("klynt"));
    assert_eq!(stop_class(&held, OWN, None), StopClass::Free);
}

#[test]
fn a_process_target_naming_the_backend_pid_is_protected() {
    let world = world();
    live(&world, 700, BACKEND);
    let read = fresh(vec![container_entry(
        CONTAINER,
        8080,
        "nginx",
        Some("klynt"),
    )]);
    let got = one(resolve(
        &read,
        &world.guard,
        &Selector::Pid(700),
        &[process(700)],
    ));
    assert_eq!(
        got,
        refused(Outcome::Protected, "This process is protected")
    );
}

#[test]
fn a_database_under_a_project_selector_is_protected_but_under_a_pid_selector_is_free() {
    let world = world();
    live(&world, 300, "/opt/homebrew/bin/postgres");
    let mut database = entry(
        300,
        5432,
        "/opt/homebrew/bin/postgres",
        PortKind::Database,
        Some("klynt"),
    );
    database.name = "postgres".to_owned();
    let read = fresh(vec![database]);
    assert_eq!(
        stop_class(&read.entries[0], OWN, None),
        StopClass::GroupProtected
    );
    let under_project = one(resolve(
        &read,
        &world.guard,
        &project("klynt"),
        &[process(300)],
    ));
    assert_eq!(
        under_project,
        refused(Outcome::Protected, "This port is not stopped with a group")
    );
    let under_pid = one(resolve(
        &read,
        &world.guard,
        &Selector::Pid(300),
        &[process(300)],
    ));
    assert_eq!(under_pid, go_process(300));
}

#[test]
fn a_system_port_follows_the_same_rule() {
    let world = world();
    live(&world, 310, "/usr/libexec/rapportd");
    let read = fresh(vec![entry(
        310,
        49152,
        "/usr/libexec/rapportd",
        PortKind::System,
        None,
    )]);
    let under_project = one(resolve(
        &read,
        &world.guard,
        &Selector::Project(None),
        &[process(310)],
    ));
    assert_eq!(
        under_project,
        refused(Outcome::Protected, "This port is not stopped with a group")
    );
    let under_pid = one(resolve(
        &read,
        &world.guard,
        &Selector::Pid(310),
        &[process(310)],
    ));
    assert_eq!(under_pid, go_process(310));
}

#[test]
fn protection_comes_from_the_fresh_entry_not_the_request() {
    let world = world();
    live(&world, 300, "/usr/local/bin/node");
    let request = [process(300)];
    let as_dev = fresh(vec![dev(300, 3000)]);
    assert_eq!(
        one(resolve(&as_dev, &world.guard, &project("klynt"), &request)),
        go_process(300)
    );
    let mut now_a_database = dev(300, 3000);
    now_a_database.kind = PortKind::Database;
    let as_database = fresh(vec![now_a_database]);
    assert_eq!(
        one(resolve(
            &as_database,
            &world.guard,
            &project("klynt"),
            &request
        )),
        refused(Outcome::Protected, "This port is not stopped with a group")
    );
}

#[test]
fn a_target_outside_the_named_worktree_is_outside_the_selector() {
    let world = world();
    live(&world, 200, "/usr/local/bin/node");
    let read = fresh(vec![dev(200, 3000)]);
    let elsewhere = Selector::Worktree {
        project: "klynt".to_owned(),
        worktree: "feature".to_owned(),
    };
    let got = one(resolve(&read, &world.guard, &elsewhere, &[process(200)]));
    assert_eq!(
        got,
        refused(Outcome::OutsideSelector, "It is not in that project")
    );
    let other_project = one(resolve(
        &read,
        &world.guard,
        &project("other"),
        &[process(200)],
    ));
    assert_eq!(
        other_project,
        refused(Outcome::OutsideSelector, "It is not in that project")
    );
}

#[test]
fn a_project_selector_with_no_project_reaches_only_unplaced_targets() {
    let world = world();
    live(&world, 200, "/usr/local/bin/node");
    live(&world, 201, "/usr/local/bin/node");
    let placed = dev(200, 3000);
    let unplaced = entry(201, 3001, "/usr/local/bin/node", PortKind::Other, None);
    let read = fresh(vec![placed, unplaced]);
    let got = resolve(
        &read,
        &world.guard,
        &Selector::Project(None),
        &[process(200), process(201)],
    );
    assert_eq!(
        got,
        vec![
            refused(Outcome::OutsideSelector, "It is not in that project"),
            go_process(201)
        ]
    );
}

#[test]
fn a_selector_never_adds_a_target_the_request_did_not_name() {
    let world = world();
    live(&world, 200, "/usr/local/bin/node");
    live(&world, 201, "/usr/local/bin/node");
    let read = fresh(vec![dev(200, 3000), dev(201, 3001)]);
    let got = resolve(&read, &world.guard, &project("klynt"), &[process(200)]);
    assert_eq!(got, vec![go_process(200)]);
}

#[test]
fn a_container_not_running_is_stale() {
    let world = world();
    let read = fresh(vec![container_entry("aaaa", 8080, "nginx", Some("klynt"))]);
    let got = one(resolve(
        &read,
        &world.guard,
        &project("klynt"),
        &[container(CONTAINER)],
    ));
    assert_eq!(got, refused(Outcome::Stale, "The container is not running"));
}

#[test]
fn a_container_that_no_longer_publishes_a_port_is_stale() {
    let world = world();
    let read = fresh(vec![container_entry("aaaa", 8080, "nginx", Some("klynt"))]);
    let got = resolve(
        &read,
        &world.guard,
        &project("klynt"),
        &[container("aaaa"), container(CONTAINER)],
    );
    assert_eq!(
        got,
        vec![
            Resolved::Go(Plan::Container(ContainerPlan {
                id: "aaaa".to_owned(),
                ports: vec![8080]
            })),
            refused(Outcome::Stale, "The container is not running")
        ]
    );
}

#[test]
fn a_container_with_several_ports_is_planned_once_with_all_of_them() {
    let world = world();
    let read = fresh(vec![
        container_entry(CONTAINER, 9090, "nginx", Some("klynt")),
        container_entry(CONTAINER, 8080, "nginx", Some("klynt")),
        container_entry(CONTAINER, 8080, "nginx", Some("klynt")),
    ]);
    let got = one(resolve(
        &read,
        &world.guard,
        &project("klynt"),
        &[container(CONTAINER)],
    ));
    assert_eq!(
        got,
        Resolved::Go(Plan::Container(ContainerPlan {
            id: CONTAINER.to_owned(),
            ports: vec![8080, 9090]
        }))
    );
}

#[test]
fn a_container_in_another_project_is_outside_the_selector() {
    let world = world();
    let read = fresh(vec![container_entry(
        CONTAINER,
        8080,
        "nginx",
        Some("other"),
    )]);
    let got = one(resolve(
        &read,
        &world.guard,
        &project("klynt"),
        &[container(CONTAINER)],
    ));
    assert_eq!(
        got,
        refused(Outcome::OutsideSelector, "It is not in that project")
    );
}

#[test]
fn a_database_image_container_is_protected_under_a_project_selector() {
    let world = world();
    let held = container_entry(CONTAINER, 5432, "docker.io/library/postgres", Some("klynt"));
    assert_eq!(stop_class(&held, OWN, None), StopClass::GroupProtected);
    let read = fresh(vec![held]);
    let got = one(resolve(
        &read,
        &world.guard,
        &project("klynt"),
        &[container(CONTAINER)],
    ));
    assert_eq!(
        got,
        refused(Outcome::Protected, "This port is not stopped with a group")
    );
}

#[test]
fn unreachable_docker_fails_every_container_before_membership_is_checked() {
    let world = world();
    live(&world, 200, "/usr/local/bin/node");
    for docker in [
        DockerStatus::Unreadable("Docker did not answer".to_owned()),
        DockerStatus::NotSeen,
    ] {
        let read = FreshRead {
            scan: Scan::default(),
            entries: vec![dev(200, 3000)],
            docker,
        };
        let got = resolve(
            &read,
            &world.guard,
            &project("klynt"),
            &[container(CONTAINER), process(200)],
        );
        assert_eq!(
            got,
            vec![
                refused(Outcome::Failed, "Docker is not reachable"),
                go_process(200)
            ]
        );
    }
}

#[tokio::test]
async fn a_fresh_read_lists_the_listeners_again_and_leaves_docker_unseen_without_a_backend() {
    let processes = Arc::new(FakeProcesses::new(USER));
    processes.add(FakeProcess::new(1, 0, 0, START_US, "/sbin/launchd"));
    processes.add(
        FakeProcess::new(200, 1, USER, START_US, "/usr/local/bin/node")
            .name("node")
            .listen("127.0.0.1", 3000),
    );
    let containers = Arc::new(common::FakeContainers::new());
    let read = FreshRead::take(
        processes,
        PathBuf::from("/Users/nobody"),
        containers.clone(),
    )
    .await
    .unwrap();
    let ports: Vec<u16> = read.entries.iter().map(|entry| entry.port).collect();
    assert_eq!(ports, vec![3000]);
    assert_eq!(read.docker, DockerStatus::NotSeen);
    assert_eq!(containers.calls(), 0);
}

fn guarded(processes: &Arc<FakeProcesses>) -> Guard {
    Guard::new(
        Arc::new(FakeSignals::new(OWN, OWN)),
        Arc::clone(processes) as Arc<dyn navori_macos::ProcessSource>,
        None,
    )
}

fn every_selector() -> Vec<Selector> {
    vec![
        Selector::Pid(200),
        Selector::Worktree {
            project: "klynt".to_owned(),
            worktree: "main".to_owned(),
        },
        project("klynt"),
    ]
}

fn assert_unchecked(processes: &Arc<FakeProcesses>) {
    processes.add(FakeProcess::new(
        200,
        1,
        USER,
        START_US,
        "/usr/local/bin/node",
    ));
    let guard = guarded(processes);
    let read = fresh(vec![dev(200, 3000)]);
    for selector in every_selector() {
        let got = one(resolve(&read, &guard, &selector, &[process(200)]));
        assert_eq!(
            got,
            refused(
                Outcome::Failed,
                "Navori could not check what it must not stop"
            ),
            "{selector:?}"
        );
    }
}

#[test]
fn an_unreadable_own_process_fails_every_process_target() {
    let processes = Arc::new(FakeProcesses::new(USER));
    processes.add(FakeProcess::new(OWN, PARENT, USER, START_US, "/navori"));
    processes.fail_entry(OWN, ReadError::Denied);
    assert_unchecked(&processes);
}

#[test]
fn an_unreadable_middle_ancestor_fails_every_process_target() {
    let processes = Arc::new(FakeProcesses::new(USER));
    processes.add(FakeProcess::new(1, 0, 0, START_US, "/sbin/launchd"));
    processes.add(FakeProcess::new(80, 1, USER, START_US, "/bin/login"));
    processes.add(FakeProcess::new(PARENT, 80, USER, START_US, "/bin/zsh"));
    processes.add(FakeProcess::new(OWN, PARENT, USER, START_US, "/navori"));
    processes.fail_entry(PARENT, ReadError::Denied);
    assert_unchecked(&processes);
}

#[test]
fn an_ancestry_deeper_than_the_walk_allows_fails_every_process_target() {
    let processes = Arc::new(FakeProcesses::new(USER));
    processes.add(FakeProcess::new(1, 0, 0, START_US, "/sbin/launchd"));
    let mut parent = 1;
    for pid in 1000..1070 {
        processes.add(FakeProcess::new(pid, parent, USER, START_US, "/bin/sh"));
        parent = pid;
    }
    processes.add(FakeProcess::new(OWN, parent, USER, START_US, "/navori"));
    assert_unchecked(&processes);
}

#[test]
fn an_ancestry_that_loops_fails_every_process_target() {
    let processes = Arc::new(FakeProcesses::new(USER));
    processes.add(FakeProcess::new(PARENT, 80, USER, START_US, "/bin/zsh"));
    processes.add(FakeProcess::new(80, PARENT, USER, START_US, "/bin/login"));
    processes.add(FakeProcess::new(OWN, PARENT, USER, START_US, "/navori"));
    assert_unchecked(&processes);
}

#[test]
fn a_container_entry_held_by_navori_itself_is_protected_under_every_selector() {
    let world = world();
    let mut held = container_entry(CONTAINER, 8080, "nginx", Some("klynt"));
    held.pid = OWN;
    let read = fresh(vec![held]);
    for selector in [Selector::Pid(OWN), project("klynt")] {
        let got = one(resolve(
            &read,
            &world.guard,
            &selector,
            &[container(CONTAINER)],
        ));
        assert_eq!(
            got,
            refused(Outcome::Protected, "This process is protected"),
            "{selector:?}"
        );
    }
}

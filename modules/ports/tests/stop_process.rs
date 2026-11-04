#![allow(clippy::unwrap_used)]

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use navori_macos::fake::{FakeProcess, FakeProcesses, FakeSignals};
use navori_macos::{ProcessSource, Signal, SignalTarget, Signals};
use navori_mod_ports::{
    DockerStatus, FreshRead, Guard, Outcome, Placement, Plan, PortEntry, PortKind, ProcessPlan,
    Reach, Resolved, Scan, Selector, Target, member_outcome, reach_for, resolve, stop_process,
};
use tokio::time::Instant;

const START_US: u64 = 1_730_000_000_000_000;
const OTHER_START_US: u64 = 1_730_000_999_000_000;
const USER: u32 = 501;
const OWN: i32 = 100;
const PARENT: i32 = 90;
const HELPER: &str = "/Applications/Navori.app/Contents/Helpers/navori";

struct World {
    processes: Arc<FakeProcesses>,
    signals: Arc<FakeSignals>,
    guard: Guard,
}

fn world_in_group(own_group: i32) -> World {
    let processes = Arc::new(FakeProcesses::new(USER));
    processes.add(FakeProcess::new(1, 0, 0, START_US, "/sbin/launchd"));
    processes.add(FakeProcess::new(PARENT, 1, USER, START_US, "/bin/zsh"));
    processes.add(FakeProcess::new(OWN, PARENT, USER, START_US, "/navori"));
    let signals = Arc::new(FakeSignals::new(OWN, own_group));
    signals.set_group(1, 1);
    signals.set_group(PARENT, PARENT);
    signals.set_group(OWN, own_group);
    let guard = Guard::new(
        Arc::clone(&signals) as Arc<dyn Signals>,
        Arc::clone(&processes) as Arc<dyn ProcessSource>,
        Some(PathBuf::from(HELPER)),
    );
    World {
        processes,
        signals,
        guard,
    }
}

fn world() -> World {
    world_in_group(OWN)
}

fn listener(world: &World, pid: i32, group: i32) {
    world.processes.add(FakeProcess::new(
        pid,
        1,
        USER,
        START_US,
        "/usr/local/bin/node",
    ));
    world.signals.set_group(pid, group);
}

fn entry(pid: i32, port: u16, exe: &str) -> PortEntry {
    PortEntry {
        port,
        addresses: vec!["127.0.0.1".parse().unwrap()],
        pid,
        start_us: START_US,
        name: "server".to_owned(),
        exe: Some(PathBuf::from(exe)),
        place: Placement {
            project: Some("klynt".to_owned()),
            worktree: Some("main".to_owned()),
            ..Placement::default()
        },
        harness: None,
        kind: PortKind::DevServer,
        container: None,
    }
}

fn dev(pid: i32, port: u16) -> PortEntry {
    entry(pid, port, "/usr/local/bin/node")
}

fn fresh(entries: Vec<PortEntry>) -> FreshRead {
    FreshRead {
        scan: Scan::default(),
        entries,
        docker: DockerStatus::Read,
    }
}

fn plan(pid: i32, reach: Reach) -> ProcessPlan {
    ProcessPlan {
        pid,
        start_us: START_US,
        members: vec![(pid, START_US)],
        reach,
    }
}

fn stopped_plan(pid: i32) -> ProcessPlan {
    plan(pid, Reach::Alone)
}

fn reach(world: &World, entries: Vec<PortEntry>, pid: i32, confirmed: &[(i32, u64)]) -> Reach {
    reach_for(
        &plan(pid, Reach::Alone),
        &fresh(entries),
        world.signals.as_ref(),
        &world.guard,
        confirmed,
    )
}

fn stamps(signals: &FakeSignals) -> Arc<Mutex<Vec<Instant>>> {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&seen);
    signals.before_send(Box::new(move || sink.lock().unwrap().push(Instant::now())));
    seen
}

#[tokio::test(start_paused = true)]
async fn a_process_that_obeys_term_is_gone_after_the_wait_and_gets_no_kill() {
    let world = world();
    listener(&world, 200, 150);
    world.signals.obeys(200, &world.processes);

    let got = stop_process(
        stopped_plan(200),
        world.processes.as_ref(),
        world.signals.as_ref(),
    )
    .await;

    assert_eq!(got, (Outcome::StoppedAlone, None));
    assert_eq!(
        world.signals.sent(),
        vec![(SignalTarget::Process(200), Signal::Term)]
    );
}

#[tokio::test(start_paused = true)]
async fn a_process_that_ignores_term_gets_kill_after_three_seconds() {
    let world = world();
    listener(&world, 200, 150);
    world.signals.obeys(200, &world.processes);
    world.signals.ignore_term(200);
    let at = stamps(&world.signals);

    let got = stop_process(
        stopped_plan(200),
        world.processes.as_ref(),
        world.signals.as_ref(),
    )
    .await;

    assert_eq!(got, (Outcome::StoppedAlone, None));
    assert_eq!(
        world.signals.sent(),
        vec![
            (SignalTarget::Process(200), Signal::Term),
            (SignalTarget::Process(200), Signal::Kill),
        ]
    );
    let at = at.lock().unwrap();
    assert_eq!(at[1] - at[0], Duration::from_secs(3));
}

#[tokio::test(start_paused = true)]
async fn a_process_still_running_one_second_after_kill_fails_with_a_fixed_sentence() {
    let world = world();
    listener(&world, 200, 150);
    let started = Instant::now();

    let got = stop_process(
        stopped_plan(200),
        world.processes.as_ref(),
        world.signals.as_ref(),
    )
    .await;

    assert_eq!(got, (Outcome::Failed, Some("It is still running")));
    assert_eq!(started.elapsed(), Duration::from_secs(4));
    assert_eq!(
        world.signals.sent(),
        vec![
            (SignalTarget::Process(200), Signal::Term),
            (SignalTarget::Process(200), Signal::Kill),
        ]
    );
}

#[tokio::test(start_paused = true)]
async fn a_signal_refused_by_the_os_fails_and_names_no_path() {
    let world = world();
    listener(&world, 200, 150);
    world.signals.deny(200);

    let got = stop_process(
        stopped_plan(200),
        world.processes.as_ref(),
        world.signals.as_ref(),
    )
    .await;

    assert_eq!(got, (Outcome::Failed, Some("The signal was refused")));
    assert!(world.signals.sent().is_empty());
}

#[tokio::test(start_paused = true)]
async fn a_first_term_that_finds_no_such_process_is_already_gone() {
    let world = world();
    listener(&world, 200, 150);
    world.signals.obeys(200, &world.processes);
    world.processes.exit(200);

    let got = stop_process(
        stopped_plan(200),
        world.processes.as_ref(),
        world.signals.as_ref(),
    )
    .await;

    assert_eq!(got, (Outcome::AlreadyGone, None));
    assert!(world.signals.sent().is_empty());
}

#[tokio::test(start_paused = true)]
async fn a_zombie_after_term_counts_as_gone() {
    let world = world();
    world
        .processes
        .add(FakeProcess::new(200, 1, USER, START_US, "/usr/local/bin/node").zombie());
    world.signals.set_group(200, 150);

    let got = stop_process(
        stopped_plan(200),
        world.processes.as_ref(),
        world.signals.as_ref(),
    )
    .await;

    assert_eq!(got, (Outcome::StoppedAlone, None));
    assert_eq!(world.signals.sent().len(), 1);
}

#[tokio::test(start_paused = true)]
async fn a_pid_replaced_just_before_the_kill_check_gets_no_kill() {
    let world = world();
    listener(&world, 200, 150);
    world.signals.ignore_term(200);
    world.signals.obeys(200, &world.processes);
    let processes = Arc::clone(&world.processes);
    world.signals.before_send(Box::new(move || {
        processes.replace(200, OTHER_START_US, USER);
    }));

    let got = stop_process(
        stopped_plan(200),
        world.processes.as_ref(),
        world.signals.as_ref(),
    )
    .await;

    assert_eq!(got.0, Outcome::StoppedAlone);
    assert_eq!(
        world.signals.sent(),
        vec![(SignalTarget::Process(200), Signal::Term)]
    );
}

#[tokio::test(start_paused = true)]
async fn a_pid_replaced_between_term_and_the_wait_end_is_reported_gone_and_gets_no_kill() {
    let world = world();
    listener(&world, 200, 150);
    world.signals.ignore_term(200);
    world.signals.obeys(200, &world.processes);
    let processes = Arc::clone(&world.processes);
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(2)).await;
        processes.replace(200, OTHER_START_US, USER);
    });

    let got = stop_process(
        stopped_plan(200),
        world.processes.as_ref(),
        world.signals.as_ref(),
    )
    .await;

    assert_eq!(got, (Outcome::StoppedAlone, None));
    assert_eq!(
        world.signals.sent(),
        vec![(SignalTarget::Process(200), Signal::Term)]
    );
}

#[tokio::test(start_paused = true)]
async fn a_leader_whose_group_holds_only_confirmed_listeners_is_signalled_as_a_group_and_each_member_answers_from_its_own_check()
 {
    let world = world();
    listener(&world, 200, 200);
    listener(&world, 201, 200);
    listener(&world, 202, 200);
    for pid in [200, 201, 202] {
        world.signals.obeys(pid, &world.processes);
    }
    world.signals.ignore_term(202);
    let confirmed = [(200, START_US), (201, START_US), (202, START_US)];
    let entries = vec![dev(200, 3000), dev(201, 3001), dev(202, 3002)];

    let decided = reach(&world, entries, 200, &confirmed);
    assert_eq!(
        decided,
        Reach::Group(vec![(200, START_US), (201, START_US), (202, START_US)])
    );
    let got = stop_process(
        plan(200, decided),
        world.processes.as_ref(),
        world.signals.as_ref(),
    )
    .await;

    assert_eq!(got, (Outcome::Stopped, None));
    assert_eq!(
        world.signals.sent(),
        vec![(SignalTarget::Group(200), Signal::Term)]
    );
    assert_eq!(
        member_outcome(201, START_US, world.processes.as_ref()),
        (Outcome::Stopped, None)
    );
    assert_eq!(
        member_outcome(202, START_US, world.processes.as_ref()),
        (Outcome::Failed, Some("It is still running"))
    );
}

#[test]
fn a_leader_whose_group_holds_an_unconfirmed_listener_is_signalled_alone() {
    struct Case {
        name: &'static str,
        mate_exe: &'static str,
        mate_group: Option<i32>,
        confirmed_mate: bool,
    }
    let cases = [
        Case {
            name: "unconfirmed",
            mate_exe: "/usr/local/bin/node",
            mate_group: Some(200),
            confirmed_mate: false,
        },
        Case {
            name: "protected",
            mate_exe: HELPER,
            mate_group: Some(200),
            confirmed_mate: true,
        },
        Case {
            name: "unreadable group",
            mate_exe: "/usr/local/bin/node",
            mate_group: None,
            confirmed_mate: true,
        },
    ];
    for case in cases {
        let world = world();
        listener(&world, 200, 200);
        world
            .processes
            .add(FakeProcess::new(201, 1, USER, START_US, case.mate_exe));
        if let Some(group) = case.mate_group {
            world.signals.set_group(201, group);
        }
        let mut confirmed = vec![(200, START_US)];
        if case.confirmed_mate {
            confirmed.push((201, START_US));
        }
        let entries = vec![dev(200, 3000), entry(201, 3001, case.mate_exe)];

        assert_eq!(
            reach(&world, entries, 200, &confirmed),
            Reach::Alone,
            "{}",
            case.name
        );
    }
}

#[test]
fn a_leader_in_navoris_own_group_or_an_ancestors_group_is_signalled_alone() {
    let own = world_in_group(200);
    listener(&own, 200, 200);
    assert_eq!(
        reach(&own, vec![dev(200, 3000)], 200, &[(200, START_US)]),
        Reach::Alone,
        "own group"
    );

    let ancestor = world();
    listener(&ancestor, 200, 200);
    ancestor.signals.set_group(PARENT, 200);
    assert_eq!(
        reach(&ancestor, vec![dev(200, 3000)], 200, &[(200, START_US)]),
        Reach::Alone,
        "an ancestor's group"
    );

    let init = world();
    init.processes.add(FakeProcess::new(
        1000,
        1,
        USER,
        START_US,
        "/usr/local/bin/node",
    ));
    init.signals.set_group(1000, 1);
    assert_eq!(
        reach(&init, vec![dev(1000, 3000)], 1000, &[(1000, START_US)]),
        Reach::Alone,
        "group one"
    );
}

#[test]
fn a_leader_in_navoris_own_group_is_signalled_alone_under_every_selector() {
    let world = world_in_group(200);
    listener(&world, 200, 200);
    let read = fresh(vec![dev(200, 3000)]);
    let selectors = [
        Selector::Pid(200),
        Selector::Worktree {
            project: "klynt".to_owned(),
            worktree: "main".to_owned(),
        },
        Selector::Project(Some("klynt".to_owned())),
    ];
    for selector in selectors {
        let targets = [Target::Process {
            pid: 200,
            start_us: START_US,
        }];
        let Resolved::Go(Plan::Process(planned)) =
            resolve(&read, &world.guard, &selector, &targets).remove(0)
        else {
            panic!("{selector:?} did not resolve to a process stop");
        };
        let decided = reach_for(
            &planned,
            &read,
            world.signals.as_ref(),
            &world.guard,
            &[(200, START_US)],
        );
        assert_eq!(decided, Reach::Alone, "{selector:?}");
    }
}

#[test]
fn a_non_leader_is_signalled_alone() {
    let world = world();
    listener(&world, 200, 150);
    assert_eq!(
        reach(&world, vec![dev(200, 3000)], 200, &[(200, START_US)]),
        Reach::Alone
    );
}

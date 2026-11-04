#![allow(clippy::unwrap_used)]

mod common;

use std::path::PathBuf;
use std::sync::Arc;

use common::LogCapture;
use navori_macos::fake::{FakeProcess, FakeProcesses, FakeSignals};
use navori_macos::{ProcessSource, Signals};
use navori_mod_ports::{
    DockerStatus, FreshRead, Guard, Placement, PortEntry, PortKind, ProcessPlan, Reach, Scan,
    reach_for, stop_process,
};

const START_US: u64 = 1_730_000_000_000_000;
const USER: u32 = 501;
const OWN: i32 = 100;
const PARENT: i32 = 90;
const HELPER: &str = "/Applications/Navori.app/Contents/Helpers/navori";

struct World {
    processes: Arc<FakeProcesses>,
    signals: Arc<FakeSignals>,
    guard: Guard,
}

fn world() -> World {
    let processes = Arc::new(FakeProcesses::new(USER));
    processes.add(FakeProcess::new(1, 0, 0, START_US, "/sbin/launchd"));
    processes.add(FakeProcess::new(PARENT, 1, USER, START_US, "/bin/zsh"));
    processes.add(FakeProcess::new(OWN, PARENT, USER, START_US, "/navori"));
    let signals = Arc::new(FakeSignals::new(OWN, OWN));
    signals.set_group(1, 1);
    signals.set_group(PARENT, PARENT);
    signals.set_group(OWN, OWN);
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

fn dev(pid: i32, port: u16) -> PortEntry {
    PortEntry {
        port,
        addresses: vec!["127.0.0.1".parse().unwrap()],
        pid,
        start_us: START_US,
        name: "server".to_owned(),
        exe: Some(PathBuf::from("/usr/local/bin/node")),
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

#[tokio::test(start_paused = true)]
async fn each_signal_sent_is_logged_with_pid_start_time_signal_scope_and_result() {
    let logs = LogCapture::start();
    let world = world();
    listener(&world, 200, 150);
    listener(&world, 300, 300);

    stop_process(
        stopped_plan(200),
        world.processes.as_ref(),
        world.signals.as_ref(),
    )
    .await;
    stop_process(
        plan(300, Reach::Group(vec![(300, START_US)])),
        world.processes.as_ref(),
        world.signals.as_ref(),
    )
    .await;

    let text = logs.text();
    let sent: Vec<&str> = text
        .lines()
        .filter(|line| line.contains("a stop signal was sent"))
        .collect();
    assert_eq!(sent.len(), 4, "{text}");
    let expected = [
        ("pid=200", "signal=Term", "scope=\"process\""),
        ("pid=200", "signal=Kill", "scope=\"process\""),
        ("pid=300", "signal=Term", "scope=\"group\""),
        ("pid=300", "signal=Kill", "scope=\"group\""),
    ];
    for (line, (pid, signal, scope)) in sent.iter().zip(expected) {
        for part in [pid, signal, scope, "result=Ok(())"] {
            assert!(line.contains(part), "{part} missing in {line}");
        }
        assert!(line.contains(&format!("start_us={START_US}")), "{line}");
    }
}

#[tokio::test(start_paused = true)]
async fn a_refused_signal_and_a_vanished_target_are_logged_with_their_results() {
    let logs = LogCapture::start();
    let world = world();
    listener(&world, 200, 150);
    world.signals.deny(200);

    stop_process(
        stopped_plan(200),
        world.processes.as_ref(),
        world.signals.as_ref(),
    )
    .await;
    stop_process(
        plan(999, Reach::Group(vec![(999, START_US)])),
        world.processes.as_ref(),
        world.signals.as_ref(),
    )
    .await;

    let text = logs.text();
    assert!(text.contains("result=Err(Denied)"), "{text}");
    assert!(text.contains("result=Err(Gone)"), "{text}");
}

#[test]
fn every_reason_a_group_stop_falls_back_to_the_process_alone_is_logged() {
    let logs = LogCapture::start();
    let world = world();
    listener(&world, 200, 200);
    listener(&world, 201, 200);
    let entries = vec![dev(200, 3000), dev(201, 3001)];

    let got = reach(&world, entries.clone(), 200, &[(200, START_US)]);
    assert_eq!(got, Reach::Alone);
    assert!(
        logs.text().contains("reason=\"group_mate_unconfirmed\""),
        "{}",
        logs.text()
    );

    let processes = Arc::new(FakeProcesses::new(USER));
    let blind = Guard::new(
        Arc::clone(&world.signals) as Arc<dyn Signals>,
        processes as Arc<dyn ProcessSource>,
        None,
    );
    let got = reach_for(
        &plan(200, Reach::Alone),
        &fresh(entries),
        world.signals.as_ref(),
        &blind,
        &[(200, START_US), (201, START_US)],
    );
    assert_eq!(got, Reach::Alone);
    assert!(
        logs.text().contains("reason=\"guard_incomplete\""),
        "{}",
        logs.text()
    );
    assert!(logs.text().contains("pid=200"), "{}", logs.text());
}

#[tokio::test(start_paused = true)]
async fn the_signal_log_names_no_path_executable_or_environment() {
    let logs = LogCapture::start();
    let world = world();
    listener(&world, 200, 150);

    stop_process(
        stopped_plan(200),
        world.processes.as_ref(),
        world.signals.as_ref(),
    )
    .await;

    let text = logs.text();
    for hidden in ["node", "/usr", "/bin", "klynt"] {
        assert!(!text.contains(hidden), "{hidden} in {text}");
    }
}

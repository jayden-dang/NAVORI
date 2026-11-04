#![allow(clippy::unwrap_used)]

mod common;

use std::time::Duration;

use common::{ORIGIN_MS, Rig};
use navori_kernel::{Module, TrayGlyph};
use navori_macos::fake::FakeProcess;
use navori_mod_ports::{DockerError, Ports, PortsState};
use serde_json::{Value, json};

const BACKEND: &str = "/Applications/Docker.app/Contents/MacOS/com.docker.backend";

async fn first_scan(rig: &Rig) -> Value {
    rig.advance(Duration::from_secs(5)).await;
    rig.settle().await;
    rig.dispatch(json!("list")).await.unwrap()
}

fn published(rig: &Rig) -> PortsState {
    serde_json::from_value(rig.state()).unwrap()
}

#[tokio::test(start_paused = true)]
async fn a_failed_scan_keeps_the_last_good_list_and_says_why() {
    let rig = Rig::new();
    rig.processes.add(
        FakeProcess::new(100, 1, 501, 10_100, "/usr/local/bin/node").listen("127.0.0.1", 3000),
    );
    let good = first_scan(&rig).await;
    assert_eq!(good["problem"], Value::Null);

    rig.gate.deny(true);
    rig.advance(Duration::from_secs(30)).await;
    rig.settle_problem().await;
    let failed = rig.dispatch(json!("list")).await.unwrap();

    assert_eq!(failed["scanned_at_ms"], ORIGIN_MS + 5_000);
    assert_eq!(failed["groups"], good["groups"]);
    assert_eq!(
        failed["problem"],
        "reading the process list failed (sysctl failed with errno 1; retry the process read); retry the snapshot"
    );
}

#[tokio::test(start_paused = true)]
async fn a_failed_scan_asks_for_the_problem_glyph() {
    let rig = Rig::new();
    rig.gate.deny(true);
    first_scan(&rig).await;

    let tray = Ports::tray(&published(&rig));

    assert_eq!(
        tray.glyph,
        Some((
            TrayGlyph::Problem,
            "Can't scan ports. reading the process list failed (sysctl failed with errno 1; retry the process read); retry the snapshot".to_owned()
        ))
    );
}

#[tokio::test(start_paused = true)]
async fn a_docker_failure_does_not_ask_for_the_problem_glyph() {
    let rig = Rig::new();
    rig.processes
        .add(FakeProcess::new(100, 1, 501, 10_100, BACKEND).listen("127.0.0.1", 5432));
    rig.containers.reply(Err(DockerError::Timeout));
    let state = first_scan(&rig).await;
    assert_eq!(
        state["docker"],
        json!({"unreadable": "Docker did not answer in time."})
    );

    let tray = Ports::tray(&published(&rig));

    assert_eq!(tray.glyph, None);
}

#[tokio::test(start_paused = true)]
async fn port_counts_never_turn_on_attention() {
    let rig = Rig::new();
    for pid in 100..105 {
        rig.processes.add(
            FakeProcess::new(
                pid,
                1,
                501,
                10_000 + u64::try_from(pid).unwrap(),
                "/bin/node",
            )
            .listen("127.0.0.1", u16::try_from(3000 + pid).unwrap()),
        );
    }
    first_scan(&rig).await;

    let tray = Ports::tray(&published(&rig));

    assert_eq!(tray.attention, None);
    assert_eq!(tray.glyph, None);
    let unscanned = Ports::tray(&PortsState::default());
    assert_eq!(unscanned.attention, None);
    assert_eq!(unscanned.glyph, None);
}

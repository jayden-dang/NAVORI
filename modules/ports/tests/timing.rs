#![allow(clippy::unwrap_used)]

mod common;

use std::time::Duration;

use common::{ORIGIN_MS, Rig};
use navori_macos::fake::FakeProcess;
use serde_json::{Value, json};

fn with_one_listener(rig: &Rig) {
    rig.processes.add(
        FakeProcess::new(100, 1, 501, 10_100, "/usr/local/bin/node").listen("127.0.0.1", 3000),
    );
}

#[tokio::test(start_paused = true)]
async fn with_the_panel_closed_scans_come_every_thirty_seconds() {
    let rig = Rig::new();
    with_one_listener(&rig);
    rig.advance(Duration::from_secs(5)).await;
    rig.settle_at(ORIGIN_MS + 5_000).await;
    rig.advance(Duration::from_secs(29)).await;
    assert_eq!(rig.state()["scanned_at_ms"], ORIGIN_MS + 5_000);
    rig.advance(Duration::from_secs(1)).await;
    rig.settle_at(ORIGIN_MS + 35_000).await;
    rig.advance(Duration::from_secs(30)).await;
    rig.settle_at(ORIGIN_MS + 65_000).await;
}

#[tokio::test(start_paused = true)]
async fn with_the_panel_open_scans_come_every_three_seconds() {
    let rig = Rig::new();
    with_one_listener(&rig);
    rig.idle().await;
    rig.kernel.scheduler().set_panel_visible(true);
    rig.advance(Duration::from_secs(3)).await;
    rig.settle_at(ORIGIN_MS + 3_000).await;
    rig.advance(Duration::from_secs(2)).await;
    assert_eq!(rig.state()["scanned_at_ms"], ORIGIN_MS + 3_000);
    rig.advance(Duration::from_secs(1)).await;
    rig.settle_at(ORIGIN_MS + 6_000).await;
    rig.advance(Duration::from_secs(3)).await;
    rig.settle_at(ORIGIN_MS + 9_000).await;
}

#[tokio::test(start_paused = true)]
async fn opening_the_panel_brings_the_next_scan_within_three_seconds_but_not_at_once() {
    let rig = Rig::new();
    with_one_listener(&rig);
    rig.advance(Duration::from_secs(5)).await;
    rig.settle_at(ORIGIN_MS + 5_000).await;
    let mark = rig.processes.list_calls();
    let one_scan = mark;
    rig.advance(Duration::from_secs(10)).await;
    assert_eq!(rig.list_calls_since(mark), 0);
    rig.kernel.scheduler().set_panel_visible(true);
    rig.idle().await;
    assert_eq!(rig.list_calls_since(mark), 0);
    rig.advance(Duration::from_secs(3)).await;
    rig.settle_at(ORIGIN_MS + 18_000).await;
    assert_eq!(rig.list_calls_since(mark), one_scan);
}

#[allow(clippy::await_holding_lock)]
#[tokio::test(start_paused = true)]
async fn a_scan_that_comes_due_during_a_slow_scan_is_skipped() {
    let rig = Rig::new();
    with_one_listener(&rig);
    let hold = rig.gate.block();
    rig.advance(Duration::from_secs(5)).await;
    rig.advance(Duration::from_secs(60)).await;
    drop(hold);
    rig.settle_at(ORIGIN_MS + 5_000).await;
    rig.idle().await;
    let mark = rig.processes.list_calls();
    let first = mark;
    rig.advance(Duration::from_secs(29)).await;
    assert_eq!(rig.state()["scanned_at_ms"], ORIGIN_MS + 5_000);
    assert_eq!(rig.list_calls_since(mark), 0);
    rig.advance(Duration::from_secs(1)).await;
    rig.settle_at(ORIGIN_MS + 95_000).await;
    assert_eq!(rig.processes.list_calls(), 2 * first);
}

#[allow(clippy::await_holding_lock)]
#[tokio::test(start_paused = true)]
async fn list_answers_from_the_last_scan_while_a_later_scan_is_running() {
    let rig = Rig::new();
    with_one_listener(&rig);
    rig.advance(Duration::from_secs(5)).await;
    rig.settle_at(ORIGIN_MS + 5_000).await;
    let hold = rig.gate.block();
    rig.advance(Duration::from_secs(30)).await;
    let listed = tokio::time::timeout(Duration::from_secs(1), rig.dispatch(json!("list")))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(listed["scanned_at_ms"], ORIGIN_MS + 5_000);
    let opened = tokio::time::timeout(
        Duration::from_secs(1),
        rig.dispatch(json!({"open": {"port": 3000}})),
    )
    .await
    .unwrap();
    assert!(opened.is_ok());
    drop(hold);
    rig.settle_at(ORIGIN_MS + 35_000).await;
}

#[allow(clippy::await_holding_lock)]
#[tokio::test(start_paused = true)]
async fn a_list_during_the_first_scan_waits_for_it_and_starts_no_second_scan() {
    let rig = Rig::new();
    with_one_listener(&rig);
    let hold = rig.gate.block();
    rig.advance(Duration::from_secs(5)).await;
    let (listed, ()) = tokio::join!(rig.dispatch(json!("list")), async {
        rig.idle().await;
        drop(hold);
    });
    assert_eq!(listed.unwrap()["scanned_at_ms"], ORIGIN_MS + 5_000);
    let control = Rig::new();
    control.advance(Duration::from_secs(5)).await;
    control.settle().await;
    assert_eq!(rig.gate.attempts(), control.gate.attempts());
}

#[tokio::test(start_paused = true)]
async fn after_a_crash_the_module_says_nothing_was_scanned_and_scans_again() {
    let rig = Rig::new();
    with_one_listener(&rig);
    rig.advance(Duration::from_secs(5)).await;
    rig.settle_at(ORIGIN_MS + 5_000).await;
    rig.gate.panic_once();
    rig.advance(Duration::from_secs(30)).await;
    rig.settle_restart().await;
    assert_eq!(rig.state()["scanned_at_ms"], Value::Null);
    assert_eq!(rig.state()["groups"], json!([]));
    rig.advance(Duration::from_secs(5)).await;
    rig.settle().await;
    assert!(rig.state()["scanned_at_ms"].as_u64().unwrap() > ORIGIN_MS + 35_000);
}

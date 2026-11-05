//! Kill-to-exit latency of one watched child. One test, so nothing else shares this process.

use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::Instant;

use navori_macos::{ExitSink, ExitSource, KqueueExits};

#[test]
fn a_killed_child_is_reported_with_its_token_within_ten_milliseconds() {
    let mut child = Command::new("sleep")
        .arg("30")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap_or_else(|err| panic!("spawn sleep: {err}"));
    let pid = child.id() as i32;
    let (tx, rx) = mpsc::channel();
    let sink: ExitSink = Box::new(move |pid, token| {
        let _ = tx.send((pid, token));
    });
    let mut exits = KqueueExits::start(sink).unwrap_or_else(|err| panic!("start watcher: {err}"));
    exits
        .watch(pid, 7)
        .unwrap_or_else(|err| panic!("watch {pid}: {err}"));

    let started = Instant::now();
    child
        .kill()
        .unwrap_or_else(|err| panic!("kill {pid}: {err}"));
    let event = rx
        .recv_timeout(std::time::Duration::from_secs(2))
        .unwrap_or_else(|err| panic!("exit event: {err}"));
    let elapsed = started.elapsed();
    let _ = child.wait();
    drop(exits);

    println!("exit event in {elapsed:?}");
    assert_eq!(event, (pid, 7));
    // A shared CI runner cannot hold a millisecond budget (it measured 30 ms); the budget is
    // checked on the maintainer's Mac by `cargo xtask ci` and `cargo xtask release-check`.
    if std::env::var_os("CI").is_some() {
        eprintln!("skipped the 10 ms assertion: CI is set");
        return;
    }
    assert!(
        elapsed < std::time::Duration::from_millis(10),
        "exit event took {elapsed:?}"
    );
}

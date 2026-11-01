//! Exit watches against real processes on this Mac.

use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::time::Duration;

use navori_macos::{ExitSink, ExitSource, KqueueExits, WatchError};

/// Kills and reaps the child on drop so a failed assertion cannot leave it running.
struct Reap(Option<Child>);

impl Reap {
    fn spawn(program: &str, args: &[&str]) -> Self {
        let child = Command::new(program)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap_or_else(|err| panic!("spawn {program}: {err}"));
        Self(Some(child))
    }

    fn id(&self) -> i32 {
        let Some(child) = self.0.as_ref() else {
            panic!("child already reaped");
        };
        child.id() as i32
    }

    fn kill(&mut self) {
        let Some(child) = self.0.as_mut() else {
            return;
        };
        let pid = child.id();
        child
            .kill()
            .unwrap_or_else(|err| panic!("kill {pid}: {err}"));
    }

    fn wait(&mut self) {
        if let Some(mut child) = self.0.take() {
            child
                .wait()
                .unwrap_or_else(|err| panic!("wait {}: {err}", child.id()));
        }
    }
}

impl Drop for Reap {
    fn drop(&mut self) {
        if let Some(mut child) = self.0.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

fn watcher() -> (KqueueExits, Receiver<(i32, u64)>) {
    let (tx, rx) = mpsc::channel();
    let sink: ExitSink = Box::new(move |pid, token| {
        let _ = tx.send((pid, token));
    });
    let exits = KqueueExits::start(sink).unwrap_or_else(|err| panic!("start watcher: {err}"));
    (exits, rx)
}

#[test]
fn watching_a_process_that_already_exited_reports_gone() {
    let mut child = Reap::spawn("true", &[]);
    let pid = child.id();
    child.wait();
    let (mut exits, _rx) = watcher();
    let err = exits
        .watch(pid, 1)
        .expect_err("a reaped process is not watched");
    assert_eq!(err, WatchError::Gone);
}

#[test]
fn watching_an_unreaped_zombie_reports_gone() {
    let mut child = Reap::spawn("true", &[]);
    let pid = child.id();
    std::thread::sleep(Duration::from_millis(100));
    let (mut exits, _rx) = watcher();
    let err = exits.watch(pid, 2).expect_err("a zombie is not watched");
    assert_eq!(err, WatchError::Gone);
    child.wait();
}

#[test]
fn an_unwatched_process_produces_no_event() {
    let mut child = Reap::spawn("sleep", &["30"]);
    let pid = child.id();
    let (mut exits, rx) = watcher();
    exits
        .watch(pid, 4)
        .unwrap_or_else(|err| panic!("watch {pid}: {err}"));
    exits.unwatch(pid);
    child.kill();
    match rx.recv_timeout(Duration::from_millis(200)) {
        Err(RecvTimeoutError::Timeout) => {}
        other => panic!("unwatched pid {pid} produced {other:?}"),
    }
    child.wait();

    let mut fired = Reap::spawn("sleep", &["30"]);
    let fired_pid = fired.id();
    exits
        .watch(fired_pid, 9)
        .unwrap_or_else(|err| panic!("watch {fired_pid}: {err}"));
    fired.kill();
    let event = rx
        .recv_timeout(Duration::from_secs(2))
        .unwrap_or_else(|err| panic!("exit event: {err}"));
    assert_eq!(event, (fired_pid, 9));
    exits.unwatch(fired_pid);
    fired.wait();
}

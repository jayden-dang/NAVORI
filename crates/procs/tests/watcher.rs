//! One exit for each watched process identity.

use std::sync::Arc;
use std::time::Duration;

use navori_macos::fake::{FakeExitsHandle, FakeProcess, FakeProcesses};
use navori_macos::{ProcessSource, StartTime, WatchError};
use navori_procs::{Exited, ProcId, ProcWatcher, ProcsError};

const PID: i32 = 10;
const START_US: u64 = 1_000;
const REUSED_US: u64 = 2_000;

struct Rig {
    processes: Arc<FakeProcesses>,
    exits: FakeExitsHandle,
    watcher: ProcWatcher,
}

fn identity(start_us: u64) -> ProcId {
    ProcId {
        pid: PID,
        start: StartTime(start_us),
    }
}

fn source(processes: &Arc<FakeProcesses>) -> Arc<dyn ProcessSource> {
    let owned: Arc<FakeProcesses> = Arc::clone(processes);
    owned
}

fn rig() -> Result<Rig, String> {
    let processes = Arc::new(FakeProcesses::new(501));
    processes.add(FakeProcess::new(PID, 1, 501, START_US, "/bin/zsh"));
    let exits = FakeExitsHandle::new();
    let watcher =
        ProcWatcher::new(source(&processes), exits.factory()).map_err(|err| err.to_string())?;
    Ok(Rig {
        processes,
        exits,
        watcher,
    })
}

async fn next_within(watcher: &mut ProcWatcher, ms: u64) -> Result<Exited, String> {
    match tokio::time::timeout(Duration::from_millis(ms), watcher.next()).await {
        Ok(Some(exited)) => Ok(exited),
        Ok(None) => Err("the watcher ended with no exit".to_owned()),
        Err(_) => Err(format!("no exit within {ms} ms")),
    }
}

async fn no_exit_within(watcher: &mut ProcWatcher, ms: u64) -> Result<(), String> {
    match tokio::time::timeout(Duration::from_millis(ms), watcher.next()).await {
        Err(_) => Ok(()),
        Ok(exited) => Err(format!("unexpected exit: {exited:?}")),
    }
}

#[tokio::test]
async fn an_exit_of_a_watched_process_is_delivered_once() -> Result<(), String> {
    let mut rig = rig()?;
    let id = identity(START_US);
    rig.watcher.watch(id).map_err(|err| err.to_string())?;

    rig.exits.fire(PID);
    rig.exits.fire(PID);

    assert_eq!(next_within(&mut rig.watcher, 100).await?, Exited { id });
    no_exit_within(&mut rig.watcher, 100).await
}

#[tokio::test]
async fn a_process_already_gone_is_reported_at_once() -> Result<(), String> {
    let mut rig = rig()?;
    let id = identity(START_US);
    rig.exits.fail_next_watch(PID, WatchError::Gone);

    rig.watcher.watch(id).map_err(|err| err.to_string())?;

    assert_eq!(next_within(&mut rig.watcher, 100).await?, Exited { id });
    assert!(
        rig.exits.watched().is_empty(),
        "a refused registration is not watched"
    );
    Ok(())
}

#[tokio::test]
async fn a_kqueue_failure_is_returned_and_nothing_is_watched() -> Result<(), String> {
    let mut rig = rig()?;
    rig.exits
        .fail_next_watch(PID, WatchError::Kqueue { errno: 9 });

    match rig.watcher.watch(identity(START_US)) {
        Err(ProcsError::Watch { errno: 9 }) => {}
        other => return Err(format!("expected errno 9, got {other:?}")),
    }
    assert!(
        rig.exits.watched().is_empty(),
        "watched: {:?}",
        rig.exits.watched()
    );
    no_exit_within(&mut rig.watcher, 100).await
}

#[tokio::test]
async fn a_pid_reused_before_registration_reports_the_old_identity_as_exited() -> Result<(), String>
{
    let mut rig = rig()?;
    let old = identity(START_US);
    rig.processes.replace(PID, REUSED_US, 501);

    rig.watcher.watch(old).map_err(|err| err.to_string())?;

    assert!(
        !rig.exits.watched().contains(&PID),
        "the reused pid is not watched"
    );
    assert_eq!(
        next_within(&mut rig.watcher, 100).await?,
        Exited { id: old }
    );
    no_exit_within(&mut rig.watcher, 100).await
}

#[tokio::test]
async fn an_exit_between_registration_and_recheck_is_delivered_once() -> Result<(), String> {
    let mut rig = rig()?;
    let id = identity(START_US);
    rig.processes.exit(PID);

    rig.watcher.watch(id).map_err(|err| err.to_string())?;
    assert_eq!(
        rig.exits.tokens(PID).len(),
        1,
        "registration must succeed before the re-check"
    );
    assert!(
        rig.exits.watched().is_empty(),
        "the re-check removes the watch"
    );

    rig.exits.fire(PID);

    assert_eq!(next_within(&mut rig.watcher, 100).await?, Exited { id });
    no_exit_within(&mut rig.watcher, 100).await
}

#[tokio::test]
async fn an_unwatched_process_produces_no_event() -> Result<(), String> {
    let mut rig = rig()?;
    let id = identity(START_US);
    rig.watcher.watch(id).map_err(|err| err.to_string())?;
    rig.watcher.unwatch(id);
    let old = rig
        .exits
        .tokens(PID)
        .first()
        .copied()
        .ok_or("unwatch dropped the registered token")?;

    rig.exits.fire_with(PID, old);

    no_exit_within(&mut rig.watcher, 100).await
}

#[tokio::test]
async fn rewatching_a_reused_pid_ignores_the_previous_identitys_late_exit() -> Result<(), String> {
    let mut rig = rig()?;
    let first = identity(START_US);
    rig.watcher.watch(first).map_err(|err| err.to_string())?;
    rig.watcher.unwatch(first);
    rig.processes.replace(PID, REUSED_US, 501);
    let second = identity(REUSED_US);
    rig.watcher.watch(second).map_err(|err| err.to_string())?;
    let previous = rig
        .exits
        .tokens(PID)
        .first()
        .copied()
        .ok_or("the first identity has no token")?;

    rig.exits.fire_with(PID, previous);
    no_exit_within(&mut rig.watcher, 100).await?;

    rig.exits.fire(PID);
    assert_eq!(
        next_within(&mut rig.watcher, 100).await?,
        Exited { id: second }
    );
    Ok(())
}

#[tokio::test]
async fn a_new_watcher_after_dropping_one_works_the_same() -> Result<(), String> {
    let processes = Arc::new(FakeProcesses::new(501));
    processes.add(FakeProcess::new(PID, 1, 501, START_US, "/bin/zsh"));
    let exits = FakeExitsHandle::new();
    let id = identity(START_US);

    let mut dropped =
        ProcWatcher::new(source(&processes), exits.factory()).map_err(|err| err.to_string())?;
    dropped.watch(id).map_err(|err| err.to_string())?;
    drop(dropped);

    let mut watcher =
        ProcWatcher::new(source(&processes), exits.factory()).map_err(|err| err.to_string())?;
    watcher.watch(id).map_err(|err| err.to_string())?;
    exits.fire(PID);
    exits.fire(PID);

    assert_eq!(next_within(&mut watcher, 100).await?, Exited { id });
    no_exit_within(&mut watcher, 100).await
}

#[tokio::test]
async fn next_is_cancel_safe() -> Result<(), String> {
    let mut rig = rig()?;
    let id = identity(START_US);
    rig.watcher.watch(id).map_err(|err| err.to_string())?;
    rig.exits.fire(PID);

    for _ in 0..2 {
        // `biased` polls the ready branch first. An unbiased `select!` may take the exit.
        tokio::select! {
            biased;
            () = std::future::ready(()) => {}
            exited = rig.watcher.next() => {
                return Err(format!("a cancelled poll delivered {exited:?}"));
            }
        }
    }

    assert_eq!(next_within(&mut rig.watcher, 100).await?, Exited { id });
    Ok(())
}

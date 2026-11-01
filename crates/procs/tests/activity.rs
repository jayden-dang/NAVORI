//! Live descendants of one process, each with the CPU time it has used.

use std::sync::Arc;
use std::time::Duration;

use navori_macos::StartTime;
use navori_macos::fake::{FakeProcess, FakeProcesses};
use navori_procs::{ChildActivity, NotRunning, ProcId, Snapshot, activity};

fn source() -> Arc<FakeProcesses> {
    Arc::new(FakeProcesses::new(501))
}

fn take(fake: &Arc<FakeProcesses>) -> Result<Snapshot, String> {
    let owned = Arc::clone(fake);
    Snapshot::take(owned).map_err(|err| err.to_string())
}

fn id(snapshot: &Snapshot, pid: i32) -> Result<ProcId, String> {
    snapshot
        .get(pid)
        .map(|process| process.id)
        .ok_or_else(|| format!("pid {pid} is not in the snapshot"))
}

/// `claude(100) → zsh(101) → cargo(102)` and `zsh → node(103)`.
fn tree(fake: &FakeProcesses) {
    fake.add(FakeProcess::new(100, 1, 501, 1_000, "/bin/claude"));
    fake.add(FakeProcess::new(101, 100, 501, 1_100, "/bin/zsh").cpu(Duration::from_millis(10)));
    fake.add(FakeProcess::new(102, 101, 501, 1_200, "/usr/bin/cargo").cpu(Duration::from_secs(3)));
    fake.add(
        FakeProcess::new(103, 101, 501, 1_300, "/usr/local/bin/node")
            .cpu(Duration::from_millis(250)),
    );
}

fn child(pid: i32, start_us: u64, cpu_time: Duration) -> ChildActivity {
    ChildActivity {
        process: ProcId {
            pid,
            start: StartTime(start_us),
        },
        cpu_time,
    }
}

#[test]
fn activity_lists_every_live_descendant_with_its_cpu_time() -> Result<(), String> {
    let fake = source();
    tree(&fake);
    let snapshot = take(&fake)?;

    let listed = activity(&snapshot, id(&snapshot, 100)?).map_err(|err| err.to_string())?;

    assert_eq!(
        listed,
        vec![
            child(101, 1_100, Duration::from_millis(10)),
            child(102, 1_200, Duration::from_secs(3)),
            child(103, 1_300, Duration::from_millis(250)),
        ]
    );
    Ok(())
}

#[test]
fn a_descendant_that_vanished_is_skipped() -> Result<(), String> {
    let fake = source();
    tree(&fake);
    let snapshot = take(&fake)?;
    fake.exit(103);

    let listed = activity(&snapshot, id(&snapshot, 100)?).map_err(|err| err.to_string())?;

    assert_eq!(
        listed,
        vec![
            child(101, 1_100, Duration::from_millis(10)),
            child(102, 1_200, Duration::from_secs(3)),
        ]
    );
    Ok(())
}

#[test]
fn activity_of_a_process_that_exited_is_not_running() -> Result<(), String> {
    let fake = source();
    tree(&fake);
    let snapshot = take(&fake)?;
    let root = id(&snapshot, 100)?;
    fake.exit(100);

    assert_eq!(activity(&snapshot, root), Err(NotRunning));
    Ok(())
}

#[test]
fn activity_of_a_pid_not_in_the_snapshot_is_not_running() -> Result<(), String> {
    let fake = source();
    tree(&fake);
    let snapshot = take(&fake)?;
    let missing = ProcId {
        pid: 404,
        start: StartTime(9),
    };

    assert_eq!(activity(&snapshot, missing), Err(NotRunning));
    Ok(())
}

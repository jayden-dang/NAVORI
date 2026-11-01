//! One user's processes, taken from an in-memory process list.

use std::path::PathBuf;
use std::sync::Arc;

use navori_macos::fake::{FakeProcess, FakeProcesses};
use navori_macos::{ReadError, StartTime};
use navori_procs::{ProcId, Process, Snapshot, Unavailable};

fn source() -> Arc<FakeProcesses> {
    Arc::new(FakeProcesses::new(501))
}

fn take(fake: &Arc<FakeProcesses>) -> Result<Snapshot, String> {
    let owned = Arc::clone(fake);
    Snapshot::take(owned).map_err(|err| err.to_string())
}

fn process(snapshot: &Snapshot, pid: i32) -> Result<&Process, String> {
    snapshot
        .get(pid)
        .ok_or_else(|| format!("pid {pid} is not in the snapshot"))
}

#[test]
fn a_recheck_that_is_denied_or_malformed_keeps_that_reason() -> Result<(), String> {
    for (err, want) in [
        (ReadError::Denied, Unavailable::PermissionDenied),
        (ReadError::Malformed, Unavailable::Malformed),
    ] {
        let fake = source();
        fake.add(FakeProcess::new(10, 1, 501, 1_000, "/usr/bin/git").argv(&["git"]));
        let snapshot = take(&fake)?;
        let id = process(&snapshot, 10)?.id;
        fake.fail_entry_after(id.pid, 1, err);

        assert_eq!(
            snapshot.argv(id),
            Err(want),
            "a {err:?} re-check became the wrong reason"
        );
    }
    Ok(())
}

fn pids(snapshot: &Snapshot) -> Vec<i32> {
    snapshot.processes().map(|process| process.id.pid).collect()
}

#[test]
fn the_snapshot_holds_every_process_of_the_user_with_parent_start_and_executable()
-> Result<(), String> {
    let fake = source();
    fake.add(FakeProcess::new(10, 1, 501, 1_000, "/bin/zsh"));
    fake.add(FakeProcess::new(11, 10, 501, 2_000, "/usr/bin/git"));

    let snapshot = take(&fake)?;

    assert_eq!(
        snapshot.processes().collect::<Vec<_>>(),
        vec![
            &Process {
                id: ProcId {
                    pid: 10,
                    start: StartTime(1_000),
                },
                parent: Some(1),
                exe: Ok(PathBuf::from("/bin/zsh")),
            },
            &Process {
                id: ProcId {
                    pid: 11,
                    start: StartTime(2_000),
                },
                parent: Some(10),
                exe: Ok(PathBuf::from("/usr/bin/git")),
            },
        ]
    );
    Ok(())
}

#[test]
fn processes_of_another_user_are_left_out() -> Result<(), String> {
    let fake = source();
    fake.add(FakeProcess::new(10, 1, 501, 1_000, "/bin/zsh"));
    fake.add(FakeProcess::new(1, 0, 0, 50, "/sbin/launchd"));

    let snapshot = take(&fake)?;

    assert_eq!(pids(&snapshot), vec![10]);
    Ok(())
}

#[test]
fn a_zombie_is_left_out() -> Result<(), String> {
    let fake = source();
    fake.add(FakeProcess::new(10, 1, 501, 1_000, "/bin/zsh"));
    fake.add(FakeProcess::new(12, 10, 501, 1_500, "/bin/sleep").zombie());

    let snapshot = take(&fake)?;

    assert_eq!(pids(&snapshot), vec![10]);
    Ok(())
}

#[test]
fn a_process_whose_executable_read_says_gone_is_left_out() -> Result<(), String> {
    let fake = source();
    fake.add(FakeProcess::new(10, 1, 501, 1_000, "/bin/zsh"));
    fake.add(FakeProcess::new(12, 10, 501, 1_500, "/bin/sleep").exe_err(ReadError::Gone));

    let snapshot = take(&fake)?;

    assert_eq!(pids(&snapshot), vec![10]);
    Ok(())
}

#[test]
fn a_process_replaced_during_the_snapshot_is_left_out() -> Result<(), String> {
    let fake = source();
    fake.add(FakeProcess::new(10, 1, 501, 1_000, "/bin/zsh"));
    fake.add(FakeProcess::new(12, 10, 501, 1_500, "/bin/sleep"));
    fake.replace_on_read(12, "exe", 999, 501);

    let snapshot = take(&fake)?;

    assert_eq!(pids(&snapshot), vec![10]);
    Ok(())
}

#[test]
fn an_unreadable_executable_is_kept_as_unavailable() -> Result<(), String> {
    let fake = source();
    fake.add(FakeProcess::new(10, 1, 501, 1_000, "/bin/zsh").exe_err(ReadError::Denied));

    let snapshot = take(&fake)?;

    assert_eq!(
        process(&snapshot, 10)?.exe,
        Err(Unavailable::PermissionDenied)
    );
    Ok(())
}

#[test]
fn a_reused_pid_gets_a_different_identity() -> Result<(), String> {
    let fake = source();
    fake.add(FakeProcess::new(10, 1, 501, 100, "/bin/old"));
    let first = take(&fake)?;
    let first_id = process(&first, 10)?.id;

    fake.replace(10, 999, 501);
    let second = take(&fake)?;
    let second_id = process(&second, 10)?.id;

    assert_eq!(first_id.pid, 10);
    assert_eq!(second_id.pid, 10);
    assert_ne!(first_id, second_id);
    Ok(())
}

#[test]
fn ancestors_and_descendants_are_ordered_nearest_first() -> Result<(), String> {
    let fake = source();
    fake.add(FakeProcess::new(10, 1, 501, 100, "/bin/a"));
    fake.add(FakeProcess::new(11, 10, 501, 200, "/bin/b"));
    fake.add(FakeProcess::new(12, 11, 501, 300, "/bin/c"));
    fake.add(FakeProcess::new(13, 11, 501, 400, "/bin/d"));

    let snapshot = take(&fake)?;
    let ancestors: Vec<i32> = snapshot
        .ancestors(process(&snapshot, 12)?.id)
        .iter()
        .map(|process| process.id.pid)
        .collect();
    let descendants: Vec<i32> = snapshot
        .descendants(process(&snapshot, 10)?.id)
        .iter()
        .map(|process| process.id.pid)
        .collect();

    assert_eq!(ancestors, vec![11, 10]);
    assert_eq!(descendants, vec![11, 12, 13]);
    Ok(())
}

#[test]
fn argv_and_cwd_are_read_only_for_the_asked_process() -> Result<(), String> {
    let fake = source();
    fake.add(
        FakeProcess::new(10, 1, 501, 100, "/bin/zsh")
            .argv(&["zsh", "-l"])
            .cwd("/Users/me"),
    );
    fake.add(
        FakeProcess::new(11, 10, 501, 200, "/bin/sleep")
            .argv(&["sleep", "30"])
            .cwd("/tmp"),
    );

    let snapshot = take(&fake)?;
    let asked = process(&snapshot, 10)?.id;
    assert_eq!(
        snapshot.argv(asked).map_err(|err| err.to_string())?,
        vec!["zsh".to_owned(), "-l".to_owned()]
    );
    assert_eq!(
        snapshot.cwd(asked).map_err(|err| err.to_string())?,
        PathBuf::from("/Users/me")
    );

    let argv_cwd: Vec<_> = fake
        .reads()
        .into_iter()
        .filter(|(_, kind)| *kind == "argv" || *kind == "cwd")
        .collect();
    assert_eq!(argv_cwd, vec![(10, "argv"), (10, "cwd")]);
    Ok(())
}

#[test]
fn argv_of_a_process_replaced_since_the_snapshot_is_unavailable() -> Result<(), String> {
    let fake = source();
    fake.add(FakeProcess::new(10, 1, 501, 100, "/bin/zsh").argv(&["zsh"]));
    let snapshot = take(&fake)?;
    let id = process(&snapshot, 10)?.id;

    fake.replace(10, 999, 501);

    assert_eq!(snapshot.argv(id), Err(Unavailable::Exited));
    Ok(())
}

#[test]
fn argv_and_cwd_are_never_read_for_a_pid_now_owned_by_another_user() -> Result<(), String> {
    let fake = source();
    fake.add(
        FakeProcess::new(10, 1, 501, 100, "/bin/zsh")
            .argv(&["zsh"])
            .cwd("/Users/me"),
    );
    let snapshot = take(&fake)?;
    let id = process(&snapshot, 10)?.id;

    fake.replace(10, 999, 0);

    assert_eq!(snapshot.argv(id), Err(Unavailable::Exited));
    assert_eq!(snapshot.cwd(id), Err(Unavailable::Exited));
    let argv_cwd: Vec<_> = fake
        .reads()
        .into_iter()
        .filter(|(pid, kind)| *pid == 10 && (*kind == "argv" || *kind == "cwd"))
        .collect();
    assert_eq!(argv_cwd, Vec::<(i32, &str)>::new());
    Ok(())
}

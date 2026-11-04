//! Listening sockets and names of the user's processes, read from an in-memory process list.

use std::sync::Arc;

use navori_macos::fake::{FakeProcess, FakeProcesses};
use navori_macos::{Listener, ReadError, StartTime};
use navori_procs::{ProcId, Snapshot, Unavailable};

fn source() -> Arc<FakeProcesses> {
    Arc::new(FakeProcesses::new(501))
}

fn take(fake: &Arc<FakeProcesses>) -> Result<Snapshot, String> {
    let owned = Arc::clone(fake);
    Snapshot::take(owned).map_err(|err| err.to_string())
}

fn id_of(snapshot: &Snapshot, pid: i32) -> Result<ProcId, String> {
    snapshot
        .get(pid)
        .map(|process| process.id)
        .ok_or_else(|| format!("pid {pid} is not in the snapshot"))
}

fn listener(addr: &str, port: u16) -> Result<Listener, String> {
    Ok(Listener {
        addr: addr.parse().map_err(|err| format!("{addr}: {err}"))?,
        port,
    })
}

#[test]
fn the_sockets_of_a_listed_process_are_read() -> Result<(), String> {
    let fake = source();
    fake.add(
        FakeProcess::new(10, 1, 501, 1_000, "/usr/bin/node")
            .listen("127.0.0.1", 3000)
            .listen("::1", 3001),
    );
    let snapshot = take(&fake)?;
    let id = id_of(&snapshot, 10)?;

    assert_eq!(
        snapshot.listeners(id),
        Ok(vec![listener("127.0.0.1", 3000)?, listener("::1", 3001)?])
    );
    assert!(
        snapshot
            .still_same()
            .map_err(|e| e.to_string())?
            .contains(&id)
    );
    Ok(())
}

#[test]
fn only_processes_listed_under_this_user_have_their_sockets_read() -> Result<(), String> {
    let fake = source();
    fake.add(FakeProcess::new(10, 1, 501, 1_000, "/usr/bin/node").listen("127.0.0.1", 3000));
    fake.add(FakeProcess::new(20, 1, 0, 1_000, "/usr/sbin/httpd").listen("0.0.0.0", 80));
    let snapshot = take(&fake)?;
    let other = ProcId {
        pid: 20,
        start: StartTime(1_000),
    };

    assert_eq!(snapshot.listeners(other), Err(Unavailable::Exited));
    assert_eq!(snapshot.name(other), Err(Unavailable::Exited));
    assert!(
        fake.reads().iter().all(|read| read.0 != 20),
        "a process of another user was read: {:?}",
        fake.reads()
    );
    Ok(())
}

#[test]
fn a_pid_taken_over_by_another_user_during_the_scan_is_dropped() -> Result<(), String> {
    let fake = source();
    fake.add(FakeProcess::new(10, 1, 501, 1_000, "/usr/bin/node").listen("127.0.0.1", 3000));
    fake.add(FakeProcess::new(11, 1, 501, 1_100, "/usr/bin/node").listen("127.0.0.1", 3001));
    let snapshot = take(&fake)?;
    let taken = id_of(&snapshot, 10)?;
    let kept = id_of(&snapshot, 11)?;
    fake.replace_on_read(10, "fds", 2, 0);

    let _ = snapshot.listeners(taken);
    let _ = snapshot.listeners(kept);
    let same = snapshot.still_same().map_err(|err| err.to_string())?;

    assert!(!same.contains(&taken), "the taken-over pid was kept");
    assert!(same.contains(&kept), "an untouched pid was dropped");
    Ok(())
}

#[test]
fn a_process_that_exits_while_its_sockets_are_read_is_dropped() -> Result<(), String> {
    let fake = source();
    fake.add(FakeProcess::new(10, 1, 501, 1_000, "/usr/bin/node").listen("127.0.0.1", 3000));
    let snapshot = take(&fake)?;
    let id = id_of(&snapshot, 10)?;

    let _ = snapshot.listeners(id);
    fake.exit(10);

    let same = snapshot.still_same().map_err(|err| err.to_string())?;
    assert!(!same.contains(&id));
    Ok(())
}

#[test]
fn a_refused_socket_read_is_reported_as_an_error() -> Result<(), String> {
    let fake = source();
    fake.add(FakeProcess::new(10, 1, 501, 1_000, "/usr/bin/node").fds_err(ReadError::Denied));
    let snapshot = take(&fake)?;
    let id = id_of(&snapshot, 10)?;

    assert_eq!(snapshot.listeners(id), Err(Unavailable::PermissionDenied));
    Ok(())
}

#[test]
fn the_short_name_is_read_only_while_the_process_is_still_the_same() -> Result<(), String> {
    let fake = source();
    fake.add(FakeProcess::new(10, 1, 501, 1_000, "/usr/bin/node").name("node"));
    fake.add(FakeProcess::new(11, 1, 501, 1_100, "/usr/bin/node").name("node"));
    let snapshot = take(&fake)?;
    let same = id_of(&snapshot, 10)?;
    let replaced = id_of(&snapshot, 11)?;
    fake.replace(11, 9_999, 501);

    assert_eq!(snapshot.name(same), Ok("node".to_owned()));
    assert_eq!(snapshot.name(replaced), Err(Unavailable::Exited));
    assert!(fake.reads().contains(&(10, "name")));
    Ok(())
}

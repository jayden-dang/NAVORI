//! Which Harness started a process, from its ancestors or its environment.

use std::sync::Arc;

use navori_macos::fake::{FakeProcess, FakeProcesses};
use navori_procs::{Harness, MARKERS, ProcId, Snapshot, Unavailable, started_by};

fn id_of(snapshot: &Snapshot, pid: i32) -> Result<ProcId, String> {
    snapshot
        .get(pid)
        .map(|process| process.id)
        .ok_or_else(|| format!("pid {pid} is not in the snapshot"))
}

fn started(
    fake: &Arc<FakeProcesses>,
    pid: i32,
) -> Result<Result<Option<Harness>, Unavailable>, String> {
    let snapshot = Snapshot::take(Arc::clone(fake) as _).map_err(|err| err.to_string())?;
    Ok(started_by(&snapshot, id_of(&snapshot, pid)?))
}

fn listener(pid: i32, ppid: i32) -> FakeProcess {
    FakeProcess::new(pid, ppid, 501, 10_000 + pid as u64, "/Users/u/bin/server")
}

#[test]
fn a_live_claude_ancestor_wins_over_a_marker() -> Result<(), String> {
    let fake = Arc::new(FakeProcesses::new(501));
    fake.add(FakeProcess::new(
        100,
        1,
        501,
        1_000,
        "/usr/local/bin/claude",
    ));
    fake.add(listener(101, 100).env(&["GROK_AGENT=1"]));

    assert_eq!(started(&fake, 101)?, Ok(Some(Harness::ClaudeCode)));
    assert!(!fake.reads().contains(&(101, "env")));
    Ok(())
}

#[test]
fn with_no_ancestor_each_marker_gives_its_harness() -> Result<(), String> {
    let cases = [
        ("CLAUDE_CODE_CHILD_SESSION=1", Harness::ClaudeCode),
        ("CODEX_THREAD_ID=abc", Harness::Codex),
        ("GROK_AGENT=1", Harness::Grok),
    ];
    for (entry, harness) in cases {
        let fake = Arc::new(FakeProcesses::new(501));
        fake.add(listener(101, 1).env(&["PATH=/bin", entry]));
        assert_eq!(started(&fake, 101)?, Ok(Some(harness)), "{entry}");
    }
    Ok(())
}

#[test]
fn the_markers_are_the_three_names_in_a_fixed_order() {
    let names = MARKERS.map(|(name, _)| name);
    assert_eq!(
        names,
        ["CLAUDE_CODE_CHILD_SESSION", "CODEX_THREAD_ID", "GROK_AGENT"]
    );
}

#[test]
fn two_markers_give_no_harness() -> Result<(), String> {
    let fake = Arc::new(FakeProcesses::new(501));
    fake.add(listener(101, 1).env(&["CODEX_THREAD_ID=abc", "GROK_AGENT=1"]));

    assert_eq!(started(&fake, 101)?, Ok(None));
    Ok(())
}

#[test]
fn an_environment_without_a_marker_gives_no_harness() -> Result<(), String> {
    let fake = Arc::new(FakeProcesses::new(501));
    fake.add(listener(101, 1).env(&["PATH=/bin", "GROK_AGENT_X=1"]));

    assert_eq!(started(&fake, 101)?, Ok(None));
    Ok(())
}

#[test]
fn an_unreadable_environment_is_an_error_not_an_answer() -> Result<(), String> {
    let fake = Arc::new(FakeProcesses::new(501));
    fake.add(listener(101, 1));

    assert_eq!(started(&fake, 101)?, Err(Unavailable::PermissionDenied));
    Ok(())
}

#[test]
fn a_pid_reused_during_the_read_is_exited() -> Result<(), String> {
    let fake = Arc::new(FakeProcesses::new(501));
    fake.add(listener(101, 1).env(&["GROK_AGENT=1"]));
    fake.replace_on_read(101, "env", 99_999, 501);

    assert_eq!(started(&fake, 101)?, Err(Unavailable::Exited));
    Ok(())
}

#[test]
fn a_process_owned_by_another_user_is_never_read() -> Result<(), String> {
    let fake = Arc::new(FakeProcesses::new(501));
    fake.add(listener(101, 1).env(&["GROK_AGENT=1"]));
    let snapshot = Snapshot::take(Arc::clone(&fake) as _).map_err(|err| err.to_string())?;
    let id = id_of(&snapshot, 101)?;
    fake.replace(101, 10_101, 0);

    assert_eq!(started_by(&snapshot, id), Err(Unavailable::Exited));
    assert!(!fake.reads().contains(&(101, "env")));
    Ok(())
}

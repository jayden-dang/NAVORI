//! Which harness a calling process belongs to.

use std::sync::Arc;

use navori_macos::StartTime;
use navori_macos::fake::{FakeProcess, FakeProcesses};
use navori_procs::{Harness, HarnessLookup, ProcId, Snapshot, harness_of};

fn source() -> Arc<FakeProcesses> {
    Arc::new(FakeProcesses::new(501))
}

fn take(fake: &Arc<FakeProcesses>) -> Result<Snapshot, String> {
    let owned = Arc::clone(fake);
    Snapshot::take(owned).map_err(|err| err.to_string())
}

fn add(fake: &FakeProcesses, pid: i32, ppid: i32, start_us: u64, exe: &str) {
    fake.add(FakeProcess::new(pid, ppid, 501, start_us, exe));
}

fn found(harness: Harness, pid: i32, start_us: u64) -> HarnessLookup {
    HarnessLookup::Found {
        harness,
        process: ProcId {
            pid,
            start: StartTime(start_us),
        },
    }
}

/// `claude(100) → sh(101) → navori(102)`.
fn hook_shell(fake: &FakeProcesses) {
    add(fake, 100, 1, 100_000, "/usr/local/bin/claude");
    add(fake, 101, 100, 101_000, "/bin/sh");
    add(fake, 102, 101, 102_000, "/usr/local/bin/navori");
}

#[test]
fn a_hook_shell_resolves_to_its_claude_ancestor() -> Result<(), String> {
    let fake = source();
    hook_shell(&fake);
    let snapshot = take(&fake)?;

    assert_eq!(
        harness_of(&snapshot, 102),
        found(Harness::ClaudeCode, 100, 100_000)
    );
    Ok(())
}

#[test]
fn the_harness_process_itself_resolves_to_itself() -> Result<(), String> {
    let fake = source();
    add(&fake, 40, 1, 40_000, "/bin/sh");
    add(&fake, 41, 40, 41_000, "/usr/local/bin/claude");
    let snapshot = take(&fake)?;

    assert_eq!(
        harness_of(&snapshot, 41),
        found(Harness::ClaudeCode, 41, 41_000)
    );
    Ok(())
}

#[test]
fn the_nearest_harness_wins_when_two_are_nested() -> Result<(), String> {
    let fake = source();
    add(&fake, 200, 1, 200_000, "/opt/homebrew/bin/codex");
    add(&fake, 201, 200, 201_000, "/bin/sh");
    add(&fake, 202, 201, 202_000, "/usr/local/bin/claude");
    add(&fake, 203, 202, 203_000, "/bin/sh");
    add(&fake, 204, 203, 204_000, "/usr/local/bin/navori");
    let snapshot = take(&fake)?;

    assert_eq!(
        harness_of(&snapshot, 204),
        found(Harness::ClaudeCode, 202, 202_000)
    );
    Ok(())
}

#[test]
fn a_process_with_no_harness_ancestor_is_not_a_harness() -> Result<(), String> {
    let fake = source();
    add(&fake, 50, 1, 50_000, "/usr/bin/login");
    add(&fake, 51, 50, 51_000, "/bin/zsh");
    add(&fake, 52, 51, 52_000, "/usr/local/bin/navori");
    let snapshot = take(&fake)?;

    assert_eq!(harness_of(&snapshot, 52), HarnessLookup::NotAHarness);
    Ok(())
}

#[test]
fn a_pid_not_in_the_snapshot_is_not_running() -> Result<(), String> {
    let fake = source();
    hook_shell(&fake);
    fake.add(FakeProcess::new(70, 1, 0, 70_000, "/usr/local/bin/claude"));
    let snapshot = take(&fake)?;

    assert_eq!(harness_of(&snapshot, 404), HarnessLookup::NotRunning);
    assert_eq!(harness_of(&snapshot, 70), HarnessLookup::NotRunning);
    Ok(())
}

#[test]
fn a_pid_that_exited_since_the_snapshot_is_not_running() -> Result<(), String> {
    let fake = source();
    hook_shell(&fake);
    let snapshot = take(&fake)?;
    fake.exit(102);

    assert_eq!(harness_of(&snapshot, 102), HarnessLookup::NotRunning);
    Ok(())
}

#[test]
fn a_pid_replaced_since_the_snapshot_is_not_running() -> Result<(), String> {
    let fake = source();
    hook_shell(&fake);
    let snapshot = take(&fake)?;
    fake.replace(102, 999_000, 501);

    assert_eq!(harness_of(&snapshot, 102), HarnessLookup::NotRunning);
    Ok(())
}

#[test]
fn a_pid_now_owned_by_another_user_is_not_running() -> Result<(), String> {
    let fake = source();
    hook_shell(&fake);
    let snapshot = take(&fake)?;
    fake.replace(102, 102_000, 0);

    assert_eq!(harness_of(&snapshot, 102), HarnessLookup::NotRunning);
    Ok(())
}

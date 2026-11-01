use std::sync::Arc;

use navori_macos::{AssertionKind, MacPower, MacProcesses, PowerAssertions};
use navori_procs::{Snapshot, sessions};

fn thread_count() -> usize {
    let output = std::process::Command::new("ps")
        .args(["-M", "-p", &std::process::id().to_string()])
        .output()
        .unwrap_or_else(|err| panic!("ps: {err}"));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "ps exited {}: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    stdout.lines().count() - 1
}

#[test]
fn taking_a_snapshot_resolving_sessions_and_holding_an_assertion_starts_no_thread()
-> Result<(), String> {
    let before = thread_count();
    let snapshot = Snapshot::take(Arc::new(
        MacProcesses::new().map_err(|err| err.to_string())?,
    ))
    .map_err(|err| err.to_string())?;
    let _sessions = sessions(&snapshot);
    MacPower
        .hold(AssertionKind::System, "navori-test threads")
        .map_err(|err| err.to_string())?
        .release()
        .map_err(|err| err.to_string())?;
    let after = thread_count();
    assert_eq!(before, after);
    Ok(())
}

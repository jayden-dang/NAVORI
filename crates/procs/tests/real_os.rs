//! Snapshot cost and the agent sessions actually running on this Mac.

use std::sync::Arc;
use std::time::{Duration, Instant};

use navori_macos::{MacProcesses, ProcessSource};
use navori_procs::{
    AgentSession, Branch, Field, Harness, Location, Snapshot, TerminalHost, sessions,
};

/// Far above the 10 ms budget and above the 33 ms measured with a compile running beside
/// the test, so a busy machine cannot turn it red and a real slowdown still does.
const BUSY_MACHINE_LIMIT: Duration = Duration::from_millis(100);

#[test]
fn a_snapshot_stays_under_a_hundred_milliseconds_even_on_a_busy_machine() -> Result<(), String> {
    let median = snapshot_median()?;
    assert!(
        median < BUSY_MACHINE_LIMIT,
        "median {median:?} is not under {BUSY_MACHINE_LIMIT:?}"
    );
    Ok(())
}

#[test]
#[ignore = "prints the median for the 10 ms budget; cargo xtask perf runs it on an idle machine"]
fn snapshot_cost_for_the_perf_gate() -> Result<(), String> {
    snapshot_median().map(|_| ())
}

fn snapshot_median() -> Result<Duration, String> {
    let source = mac_processes()?;
    let mut samples = Vec::with_capacity(5);
    for _ in 0..5 {
        let started = Instant::now();
        let snapshot = Snapshot::take(Arc::clone(&source)).map_err(|err| err.to_string())?;
        let found = sessions(&snapshot);
        let elapsed = started.elapsed();
        let count = snapshot.processes().count();
        std::hint::black_box(found);
        samples.push((elapsed, count));
    }
    samples.sort_by_key(|(elapsed, _)| *elapsed);
    let (median, count) = samples[2];
    println!(
        "median {} µs ({median:?}); processes {count}",
        median.as_micros()
    );
    Ok(median)
}

#[test]
#[ignore = "lists the agent sessions actually running on this Mac"]
fn the_running_agent_sessions_on_this_mac_are_listed() -> Result<(), String> {
    let source = mac_processes()?;
    let snapshot = Snapshot::take(source).map_err(|err| err.to_string())?;
    let found = sessions(&snapshot);
    for session in &found {
        println!("{}", session_line(session));
    }
    assert!(!found.is_empty(), "no agent session is running on this Mac");
    Ok(())
}

fn mac_processes() -> Result<Arc<dyn ProcessSource>, String> {
    let mac = MacProcesses::new().map_err(|err| err.to_string())?;
    Ok(Arc::new(mac))
}

fn session_line(session: &AgentSession) -> String {
    format!(
        "{} · {} · pid {} · {}",
        harness_name(session.harness),
        place(&session.location),
        session.process.pid,
        terminal_name(session.terminal),
    )
}

fn harness_name(harness: Harness) -> &'static str {
    match harness {
        Harness::ClaudeCode => "Claude Code",
        Harness::Codex => "Codex",
        Harness::Grok => "Grok",
    }
}

fn place(location: &Field<Location>) -> String {
    let Ok(location) = location else {
        return "unavailable/unavailable".to_string();
    };
    let second = location
        .worktree
        .clone()
        .unwrap_or_else(|| match &location.branch {
            Some(Branch::Named(name)) => name.clone(),
            Some(Branch::Detached { abbrev }) => abbrev.clone(),
            None => "unknown".to_string(),
        });
    format!("{}/{}", location.project, second)
}

fn terminal_name(host: Option<TerminalHost>) -> &'static str {
    match host {
        None => "unknown",
        Some(TerminalHost::Terminal) => "Terminal",
        Some(TerminalHost::ITerm2) => "iTerm2",
        Some(TerminalHost::Ghostty) => "Ghostty",
        Some(TerminalHost::WezTerm) => "WezTerm",
        Some(TerminalHost::Kitty) => "Kitty",
        Some(TerminalHost::Alacritty) => "Alacritty",
        Some(TerminalHost::Warp) => "Warp",
        Some(TerminalHost::VsCode) => "VS Code",
        Some(TerminalHost::Cursor) => "Cursor",
        Some(TerminalHost::Zed) => "Zed",
    }
}

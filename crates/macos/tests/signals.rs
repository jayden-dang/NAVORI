//! Signalling seam: scripted behaviour of the fake and one real child on this Mac.

use std::os::unix::process::CommandExt;
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use navori_macos::fake::{FakeProcess, FakeProcesses, FakeSignals};
use navori_macos::{
    MacSignals, ProcessSource, ReadError, Signal, SignalError, SignalTarget, Signals,
};

fn processes_with(pid: i32) -> Arc<FakeProcesses> {
    let processes = Arc::new(FakeProcesses::new(501));
    processes.add(FakeProcess::new(pid, 1, 501, 10, "/bin/sleep"));
    processes
}

#[test]
fn a_fake_signal_to_an_obedient_process_ends_it_and_is_recorded() {
    let processes = processes_with(300);
    let signals = FakeSignals::new(100, 100);
    signals.obeys(300, &processes);

    let result = signals.send(SignalTarget::Process(300), Signal::Term);

    assert_eq!(result, Ok(()));
    assert_eq!(processes.entry(300), Err(ReadError::Gone));
    assert_eq!(
        signals.sent(),
        vec![(SignalTarget::Process(300), Signal::Term)]
    );
}

#[test]
fn a_fake_signal_to_a_process_that_ignores_term_leaves_it_running() {
    let processes = processes_with(300);
    let signals = FakeSignals::new(100, 100);
    signals.obeys(300, &processes);
    signals.ignore_term(300);

    let term = signals.send(SignalTarget::Process(300), Signal::Term);
    let still_there = processes.entry(300).is_ok();
    let kill = signals.send(SignalTarget::Process(300), Signal::Kill);

    assert_eq!(term, Ok(()));
    assert!(still_there);
    assert_eq!(kill, Ok(()));
    assert_eq!(processes.entry(300), Err(ReadError::Gone));
}

#[test]
fn a_denied_signal_reports_denied_and_is_not_recorded_as_delivered() {
    let processes = processes_with(300);
    let signals = FakeSignals::new(100, 100);
    signals.obeys(300, &processes);
    signals.deny(300);

    let result = signals.send(SignalTarget::Process(300), Signal::Kill);

    assert_eq!(result, Err(SignalError::Denied));
    assert!(signals.sent().is_empty());
    assert!(processes.entry(300).is_ok());
}

#[test]
fn a_group_signal_is_recorded_as_a_group_target() {
    let processes = processes_with(300);
    let signals = FakeSignals::new(100, 100);
    signals.set_group(300, 300);
    signals.obeys(300, &processes);

    let group = signals.group_of(300);
    let result = signals.send(SignalTarget::Group(300), Signal::Term);

    assert_eq!(group, Ok(300));
    assert_eq!(result, Ok(()));
    assert_eq!(
        signals.sent(),
        vec![(SignalTarget::Group(300), Signal::Term)]
    );
    assert_eq!(processes.entry(300), Err(ReadError::Gone));
}

#[test]
fn a_fake_reports_its_own_identity_and_an_unknown_process_as_gone() {
    let signals = FakeSignals::new(100, 90);

    assert_eq!(signals.own_pid(), 100);
    assert_eq!(signals.own_group(), 90);
    assert_eq!(signals.group_of(555), Err(ReadError::Gone));
}

#[test]
fn a_hook_runs_before_the_signal_is_recorded() {
    let signals = FakeSignals::new(100, 100);
    let ran = Arc::new(AtomicBool::new(false));
    let seen = Arc::clone(&ran);
    signals.before_send(Box::new(move || seen.store(true, Ordering::SeqCst)));

    let result = signals.send(SignalTarget::Process(300), Signal::Term);

    assert_eq!(result, Ok(()));
    assert!(ran.load(Ordering::SeqCst));
}

#[test]
#[ignore]
fn a_real_child_started_in_its_own_group_reports_that_group_and_dies_on_kill() {
    let mut child = Command::new("sleep")
        .arg("60")
        .process_group(0)
        .spawn()
        .unwrap_or_else(|err| panic!("spawn sleep: {err}"));
    let pid = i32::try_from(child.id()).unwrap_or_else(|err| panic!("pid: {err}"));
    let signals = MacSignals::new();

    let group = signals.group_of(pid);
    let sent = signals.send(SignalTarget::Group(pid), Signal::Kill);
    let waited = child.wait();
    let again = signals.send(SignalTarget::Group(pid), Signal::Kill);

    assert_eq!(group, Ok(pid));
    assert_eq!(sent, Ok(()));
    assert!(waited.is_ok());
    assert_eq!(again, Err(SignalError::Gone));
    assert_ne!(signals.own_pid(), pid);
}

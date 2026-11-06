use std::sync::mpsc;

use navori_macos::fake::{
    FakeExitsHandle, FakeLid, FakePower, FakePowerSource, FakeProcess, FakeProcesses, LidCall,
};
use navori_macos::{
    AssertionKind, LidError, LidSetting, PlatformError, PowerAssertions, PowerError, PowerKind,
    PowerSource, PowerSourceError, ProcessSource,
};

#[test]
fn refuse_still_refuses_every_kind() {
    let fake = FakePower::new();
    fake.refuse(5);
    for kind in [AssertionKind::System, AssertionKind::Display] {
        let err = fake.hold(kind, "navori-test").expect_err("refused");
        assert_eq!(err, PowerError::Refused { kind, code: 5 });
    }
    assert!(fake.held().is_empty());
}

#[test]
fn a_refused_kind_leaves_the_other_kind_held() {
    let fake = FakePower::new();
    fake.refuse_kind(AssertionKind::Display, 7);
    let _guard = fake
        .hold(AssertionKind::System, "navori-test")
        .expect("system");
    assert_eq!(
        fake.held(),
        vec![(AssertionKind::System, "navori-test".to_owned())]
    );
    let err = fake
        .hold(AssertionKind::Display, "navori-test")
        .expect_err("display refused");
    assert_eq!(
        err,
        PowerError::Refused {
            kind: AssertionKind::Display,
            code: 7,
        }
    );
    assert_eq!(
        fake.held(),
        vec![(AssertionKind::System, "navori-test".to_owned())]
    );
}

#[test]
fn allow_lifts_a_refusal() {
    let fake = FakePower::new();
    fake.refuse(5);
    fake.hold(AssertionKind::System, "navori-test")
        .expect_err("refused");
    fake.allow();
    let _guard = fake
        .hold(AssertionKind::System, "navori-test")
        .expect("hold after allow");
    assert_eq!(
        fake.held(),
        vec![(AssertionKind::System, "navori-test".to_owned())]
    );
}

#[test]
fn a_failing_release_returns_its_code_and_still_counts_the_release() {
    let fake = FakePower::new();
    let guard = fake
        .hold(AssertionKind::System, "navori-test")
        .expect("hold");
    fake.fail_release(9);
    let err = guard.release().expect_err("release failed");
    assert_eq!(err, PowerError::ReleaseFailed { code: 9 });
    assert_eq!(fake.releases(), 1);
    assert!(fake.held().is_empty());
}

#[test]
fn panic_on_hold_panics_that_many_times_then_holds() {
    let fake = FakePower::new();
    fake.panic_on_hold(2);
    for _ in 0..2 {
        let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = fake.hold(AssertionKind::System, "navori-test");
        }));
        assert!(panicked.is_err(), "hold should panic");
        assert!(fake.held().is_empty());
    }
    let _guard = fake
        .hold(AssertionKind::System, "navori-test")
        .expect("hold after panics");
    assert_eq!(
        fake.held(),
        vec![(AssertionKind::System, "navori-test".to_owned())]
    );
}

#[test]
fn list_calls_counts_each_list() {
    let processes = FakeProcesses::new(501);
    assert_eq!(processes.list_calls(), 0);
    processes.list().expect("list");
    processes.list().expect("list");
    assert_eq!(processes.list_calls(), 2);
}

#[test]
fn fail_list_makes_a_snapshot_fail() {
    let processes = FakeProcesses::new(501);
    processes.add(FakeProcess::new(10, 1, 501, 1_000, "/bin/zsh"));
    processes.fail_list(true);
    let err = processes.list().expect_err("list failed");
    assert!(matches!(err, PlatformError::Sysctl { .. }));
}

#[test]
fn close_makes_the_watcher_return_none() {
    let handle = FakeExitsHandle::new();
    let (tx, rx) = mpsc::channel();
    handle.factory()(Box::new(move |pid, token| {
        let _ = tx.send((pid, token));
    }))
    .expect("factory");
    handle.close();
    assert_eq!(rx.try_recv(), Err(mpsc::TryRecvError::Disconnected));
}

#[test]
fn a_held_lid_is_set_until_released() {
    let fake = FakeLid::new();
    let guard = fake.hold().expect("held");
    assert!(fake.is_set());
    guard.release().expect("released");
    assert!(!fake.is_set());
}

#[test]
fn releasing_a_lid_guard_clears_the_setting() {
    let fake = FakeLid::new();
    let guard = fake.hold().expect("held");
    guard.release().expect("released");
    assert_eq!(fake.calls(), vec![LidCall::Set, LidCall::Clear]);
}

#[test]
fn dropping_a_lid_guard_clears_the_setting() {
    let fake = FakeLid::new();
    drop(fake.hold().expect("held"));
    assert!(!fake.is_set());
    assert_eq!(fake.calls().last(), Some(&LidCall::Clear));
}

#[test]
fn a_refused_set_leaves_the_setting_clear() {
    let fake = FakeLid::new();
    fake.refuse_set(9);
    let err = fake.hold().expect_err("refused");
    assert!(matches!(err, LidError::Refused { code: 9 }));
    assert!(!fake.is_set());
}

#[test]
fn a_refused_clear_keeps_the_setting() {
    let fake = FakeLid::new();
    let guard = fake.hold().expect("held");
    fake.refuse_clear(4);
    let err = guard.release().expect_err("refused");
    assert!(matches!(err, LidError::Refused { code: 4 }));
    assert!(fake.is_set());
    fake.allow();
    fake.give_back().expect("cleared");
    assert!(!fake.is_set());
}

#[test]
fn give_back_clears_without_a_guard() {
    let fake = FakeLid::new();
    std::mem::forget(fake.hold().expect("held"));
    assert!(fake.is_set());
    fake.give_back().expect("cleared");
    assert!(!fake.is_set());
    assert_eq!(fake.calls(), vec![LidCall::Set, LidCall::Clear]);
}

#[test]
fn the_probe_sees_every_call_in_order() {
    let fake = FakeLid::new();
    let (tx, rx) = mpsc::channel();
    let tx = std::sync::Mutex::new(tx);
    fake.on_call(Box::new(move |call| {
        tx.lock().expect("probe").send(call).expect("send");
    }));
    fake.hold().expect("held").release().expect("released");
    fake.give_back().expect("cleared");
    let seen: Vec<LidCall> = rx.try_iter().collect();
    assert_eq!(seen, vec![LidCall::Set, LidCall::Clear, LidCall::Clear]);
}

#[test]
fn the_scripted_sleep_reason_is_returned() {
    let fake = FakeLid::new();
    assert_eq!(fake.sleep_reason(), None);
    fake.set_reason(Some("Clamshell Sleep"));
    assert_eq!(fake.sleep_reason().as_deref(), Some("Clamshell Sleep"));
}

#[test]
fn the_fake_counts_current_reads() {
    let fake = FakePowerSource::new(PowerKind::Adapter);
    assert_eq!(fake.current().expect("adapter"), PowerKind::Adapter);
    fake.set(PowerKind::Battery);
    assert_eq!(fake.current().expect("battery"), PowerKind::Battery);
    assert_eq!(fake.read_count(), 2);
    assert_eq!(fake.watch_count(), 0);
}

#[test]
fn a_failed_watch_returns_an_error() {
    let fake = FakePowerSource::new(PowerKind::Battery);
    fake.fail_watch();
    let err = fake.watch().expect_err("watch fails");
    assert!(matches!(err, PowerSourceError::Register { .. }));
    assert_eq!(fake.watch_count(), 1);
    assert_eq!(
        fake.current().expect("current still works"),
        PowerKind::Battery
    );
}

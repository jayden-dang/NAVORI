use std::sync::mpsc;

use navori_macos::fake::{FakeExitsHandle, FakePower, FakeProcess, FakeProcesses};
use navori_macos::{AssertionKind, PlatformError, PowerAssertions, PowerError, ProcessSource};

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

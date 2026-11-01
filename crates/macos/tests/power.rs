use navori_macos::fake::FakePower;
use navori_macos::{AssertionKind, MacPower, PowerAssertions, PowerError};

fn pmset_assertions() -> String {
    let output = std::process::Command::new("pmset")
        .args(["-g", "assertions"])
        .output()
        .unwrap_or_else(|err| panic!("pmset: {err}"));
    assert!(
        output.status.success(),
        "pmset exited {}: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn line_lists(output: &str, assertion_type: &str, reason: &str) -> bool {
    output
        .lines()
        .any(|line| line.contains(assertion_type) && line.contains(reason))
}

fn hold_is_listed_until_released(kind: AssertionKind, assertion_type: &str, reason: &str) {
    let guard = MacPower
        .hold(kind, reason)
        .unwrap_or_else(|err| panic!("hold {assertion_type}: {err}"));
    let while_held = pmset_assertions();
    let listed = line_lists(&while_held, assertion_type, reason);
    let released = guard.release();
    let after = pmset_assertions();
    released.unwrap_or_else(|err| panic!("release {assertion_type}: {err}"));
    assert!(
        listed,
        "pmset did not list {assertion_type} named {reason}:\n{while_held}"
    );
    assert!(
        !after.lines().any(|line| line.contains(reason)),
        "pmset still lists {reason}:\n{after}"
    );
}

#[test]
fn a_held_assertion_is_listed_by_pmset_until_released() {
    let reason = format!("navori-test system {}", std::process::id());
    hold_is_listed_until_released(AssertionKind::System, "PreventUserIdleSystemSleep", &reason);
    let reason = format!("navori-test display {}", std::process::id());
    hold_is_listed_until_released(
        AssertionKind::Display,
        "PreventUserIdleDisplaySleep",
        &reason,
    );
}

#[test]
fn dropping_a_guard_releases_its_assertion() {
    let reason = format!("navori-test drop {}", std::process::id());
    let guard = MacPower
        .hold(AssertionKind::System, &reason)
        .unwrap_or_else(|err| panic!("hold: {err}"));
    let while_held = pmset_assertions();
    let listed = line_lists(&while_held, "PreventUserIdleSystemSleep", &reason);
    drop(guard);
    let after = pmset_assertions();
    assert!(
        listed,
        "pmset did not list the assertion before drop:\n{while_held}"
    );
    assert!(
        !after.lines().any(|line| line.contains(&reason)),
        "pmset still lists {reason} after drop:\n{after}"
    );
}

#[test]
fn a_refused_create_is_an_error_carrying_the_code() {
    assert_eq!(PowerError::from_create(AssertionKind::System, 0), None);
    assert_eq!(
        PowerError::from_create(AssertionKind::System, -536_870_206),
        Some(PowerError::Refused {
            kind: AssertionKind::System,
            code: -536_870_206,
        })
    );
}

#[test]
fn the_fake_releases_each_guard_exactly_once() {
    let fake = FakePower::new();
    let guard = fake
        .hold(AssertionKind::System, "navori-test")
        .expect("hold");
    assert_eq!(
        fake.held(),
        vec![(AssertionKind::System, "navori-test".to_owned())]
    );
    guard.release().expect("release");
    assert!(fake.held().is_empty());
    assert_eq!(fake.releases(), 1);

    let guard = fake
        .hold(AssertionKind::Display, "navori-test display")
        .expect("hold display");
    drop(guard);
    assert!(fake.held().is_empty());
    assert_eq!(fake.releases(), 2);
}

#[test]
fn a_refusing_fake_returns_the_code() {
    let fake = FakePower::new();
    fake.refuse(5);
    let err = fake
        .hold(AssertionKind::Display, "navori-test")
        .expect_err("refused");
    assert!(matches!(
        err,
        PowerError::Refused {
            kind: AssertionKind::Display,
            code: 5,
        }
    ));
    assert!(fake.held().is_empty());
}

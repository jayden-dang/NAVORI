#![allow(clippy::unwrap_used)] // helpers outside #[test] fns

use std::process::Command;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use navori_kernel::{Envelope, Kernel};
use navori_mod_awake::{Awake, Platform, Timing};
use navori_store::Store;
use serde_json::json;

#[tokio::test]
async fn a_real_child_exit_reaches_the_module() {
    let kernel = Kernel::builder(Store::open_in_memory().unwrap())
        .pause_journal_sweep()
        .register(Awake::new(
            Platform::mac(),
            "Navori test: keeping this Mac awake",
            Timing::FIXED,
        ))
        .start();
    let mut child = Command::new("sleep").arg("1").spawn().unwrap();
    let pid = i32::try_from(child.id()).unwrap();
    let held = kernel
        .dispatch(Envelope {
            module: "awake".into(),
            action: json!({ "hold_command": { "pid": pid } }),
        })
        .await;

    let mut saw_lease = matches!(&held, Ok(state) if lease_count(state) == 1);
    let deadline = Instant::now() + Duration::from_secs(8);
    let mut last = held
        .as_ref()
        .map(|state| state.to_string())
        .unwrap_or_else(|err| err.to_string());
    let mut ended = false;
    while Instant::now() < deadline {
        let state = kernel.state("awake").unwrap();
        last = state.to_string();
        let count = lease_count(&state);
        if count == 1 {
            saw_lease = true;
        }
        if saw_lease && count == 0 && state["system_held"] == false {
            ended = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let _status = child.wait();
    kernel.shutdown().await;

    held.expect("hold_command");
    assert!(saw_lease, "no command lease was held: {last}");
    assert!(
        ended,
        "the command lease did not end when the child exited: {last}"
    );
}

fn lease_count(state: &serde_json::Value) -> usize {
    state["leases"].as_array().map(Vec::len).unwrap_or(0)
}

fn assertion_name(label: &str) -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!("navori-real-os-{label}-{}-{nanos}", std::process::id())
}

fn started(name: &str) -> Kernel {
    Kernel::builder(Store::open_in_memory().unwrap())
        .pause_journal_sweep()
        .register(Awake::new(Platform::mac(), name, Timing::FIXED))
        .start()
}

fn pmset_assertions() -> String {
    let output = Command::new("pmset")
        .args(["-g", "assertions"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "pmset failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

fn listed(pmset: &str, name: &str, kind: &str) -> Vec<String> {
    let owner = format!("pid {}(", std::process::id());
    let named = format!("named: \"{name}\"");
    pmset
        .lines()
        .filter(|line| line.contains(&owner) && line.contains(kind) && line.contains(&named))
        .map(str::trim)
        .map(ToOwned::to_owned)
        .collect()
}

const SYSTEM_SLEEP: &str = "PreventUserIdleSystemSleep";
const DISPLAY_SLEEP: &str = "PreventUserIdleDisplaySleep";

async fn dispatch(kernel: &Kernel, action: serde_json::Value) -> serde_json::Value {
    kernel
        .dispatch(Envelope {
            module: "awake".into(),
            action,
        })
        .await
        .unwrap()
}

#[tokio::test]
async fn pmset_lists_one_system_assertion_for_a_lease() {
    let name = assertion_name("one-system");
    let kernel = started(&name);
    let mut child = Command::new("sleep").arg("30").spawn().unwrap();
    let pid = i32::try_from(child.id()).unwrap();
    let until_off = dispatch(&kernel, json!("hold_until_off")).await;
    let held = dispatch(&kernel, json!({ "hold_command": { "pid": pid } })).await;

    let pmset = pmset_assertions();
    let system = listed(&pmset, &name, SYSTEM_SLEEP);
    let display = listed(&pmset, &name, DISPLAY_SLEEP);
    child.kill().unwrap();
    let _status = child.wait();
    kernel.shutdown().await;

    assert_eq!(lease_count(&until_off), 1, "{until_off}");
    assert_eq!(lease_count(&held), 2, "two leases were not held: {held}");
    assert_eq!(held["system_held"], true, "{held}");
    assert_eq!(held["display_held"], false, "{held}");
    assert_eq!(
        system.len(),
        1,
        "pmset should list one system assertion for {name}:\n{pmset}"
    );
    assert!(
        display.is_empty(),
        "a lease that does not ask for the display listed one:\n{display:?}"
    );
}

#[tokio::test]
async fn pmset_lists_system_and_display_together() {
    let name = assertion_name("system-and-display");
    let kernel = started(&name);
    dispatch(&kernel, json!("hold_until_off")).await;
    let held = dispatch(&kernel, json!({ "display": { "on": true } })).await;

    let pmset = pmset_assertions();
    let system = listed(&pmset, &name, SYSTEM_SLEEP);
    let display = listed(&pmset, &name, DISPLAY_SLEEP);
    kernel.shutdown().await;

    assert_eq!(lease_count(&held), 2, "{held}");
    assert_eq!(held["system_held"], true, "{held}");
    assert_eq!(held["display_held"], true, "{held}");
    assert_eq!(
        system.len(),
        1,
        "pmset should list one system assertion beside display for {name}:\n{pmset}"
    );
    assert_eq!(
        display.len(),
        1,
        "pmset should list one display assertion for {name}:\n{pmset}"
    );
}

#[tokio::test]
async fn releasing_display_leaves_system_listed() {
    let name = assertion_name("release-display");
    let kernel = started(&name);
    dispatch(&kernel, json!("hold_until_off")).await;
    let both = dispatch(&kernel, json!({ "display": { "on": true } })).await;
    let before = pmset_assertions();
    let held = dispatch(&kernel, json!({ "display": { "on": false } })).await;
    let pmset = pmset_assertions();
    let system = listed(&pmset, &name, SYSTEM_SLEEP);
    let display = listed(&pmset, &name, DISPLAY_SLEEP);
    kernel.shutdown().await;

    assert_eq!(listed(&before, &name, SYSTEM_SLEEP).len(), 1, "{before}");
    assert_eq!(listed(&before, &name, DISPLAY_SLEEP).len(), 1, "{before}");
    assert_eq!(both["display_held"], true, "{both}");
    assert_eq!(lease_count(&held), 1, "{held}");
    assert_eq!(held["system_held"], true, "{held}");
    assert_eq!(held["display_held"], false, "{held}");
    assert_eq!(
        system.len(),
        1,
        "releasing display dropped the system assertion for {name}:\n{pmset}"
    );
    assert!(
        display.is_empty(),
        "display stayed listed after release:\n{display:?}"
    );
}

#[tokio::test]
async fn ending_the_last_lease_clears_pmset() {
    let name = assertion_name("last-lease");
    let kernel = started(&name);
    let held = dispatch(&kernel, json!("hold_until_off")).await;
    let before = pmset_assertions();
    let ended = dispatch(&kernel, json!("release_all")).await;
    let pmset = pmset_assertions();
    kernel.shutdown().await;

    assert_eq!(lease_count(&held), 1, "{held}");
    assert_eq!(listed(&before, &name, SYSTEM_SLEEP).len(), 1, "{before}");
    assert_eq!(lease_count(&ended), 0, "{ended}");
    assert_eq!(ended["system_held"], false, "{ended}");
    assert_eq!(ended["display_held"], false, "{ended}");
    assert!(
        listed(&pmset, &name, SYSTEM_SLEEP).is_empty(),
        "the system assertion stayed after the last lease for {name}:\n{pmset}"
    );
    assert!(
        listed(&pmset, &name, DISPLAY_SLEEP).is_empty(),
        "a display assertion stayed after the last lease for {name}:\n{pmset}"
    );
}

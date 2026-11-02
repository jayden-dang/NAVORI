use std::process::Command;
use std::time::{Duration, Instant};

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

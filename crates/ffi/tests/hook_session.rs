//! A session Lease against a real kernel whose power assertions and process table are fake.
#![allow(clippy::unwrap_used)]

use std::sync::Arc;

use navori_kernel::{Envelope, Kernel, RestartPolicy};
use navori_macos::fake::{FakeExitsHandle, FakePower, FakeProcess, FakeProcesses};
use navori_macos::{PowerAssertions, ProcessSource};
use navori_mod_awake::{Awake, ExitsFactory, Platform, Timing};
use navori_store::Store;
use serde_json::{Value, json};

const NAME: &str = "Navori: keeping this Mac awake";
const HARNESS: i32 = 100;
const SHELL: i32 = 101;
const HOOK: i32 = 102;

fn start_kernel(power: Arc<FakePower>, processes: Arc<FakeProcesses>) -> Kernel {
    let source = Arc::clone(&processes) as Arc<dyn ProcessSource>;
    let exits = FakeExitsHandle::new();
    let factory = {
        let exits = exits.clone();
        Box::new(move |sink| exits.clone().factory()(sink)) as ExitsFactory
    };
    let platform = Platform::new(
        Arc::clone(&power) as Arc<dyn PowerAssertions>,
        Box::new(move || Ok(Arc::clone(&source))),
        factory,
    );
    let store = Store::open_in_memory().unwrap();
    Kernel::builder(store)
        .pause_journal_sweep()
        .restart_policy(RestartPolicy::default())
        .register(Awake::new(platform, NAME, Timing::FIXED))
        .start()
}

async fn dispatch(kernel: &Kernel, action: Value) {
    kernel
        .dispatch(Envelope {
            module: "awake".into(),
            action,
        })
        .await
        .unwrap();
}

#[tokio::test]
async fn a_new_prompt_after_release_all_makes_a_new_lease() {
    let processes = Arc::new(FakeProcesses::new(501));
    processes.add(FakeProcess::new(
        HARNESS,
        1,
        501,
        100_000,
        "/usr/local/bin/claude",
    ));
    processes.add(FakeProcess::new(SHELL, HARNESS, 501, 101_000, "/bin/sh"));
    processes.add(FakeProcess::new(
        HOOK,
        SHELL,
        501,
        102_000,
        "/usr/local/bin/navori",
    ));
    let kernel = start_kernel(Arc::new(FakePower::new()), processes);
    let hold = json!({ "hold_session": { "session_id": "s-1", "pid": HOOK, "ppid": SHELL } });

    dispatch(&kernel, hold.clone()).await;
    dispatch(&kernel, json!("release_all")).await;
    dispatch(&kernel, hold).await;

    let state = kernel.state("awake").unwrap();
    assert_eq!(state["leases"].as_array().unwrap().len(), 1);
    let started = kernel
        .recent_events(50)
        .unwrap()
        .into_iter()
        .filter(|event| event.kind == "awake.lease_started")
        .count();
    assert_eq!(started, 2);
}

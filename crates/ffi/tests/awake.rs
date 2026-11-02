//! Awake doctor row and snapshot view, on a kernel whose power assertions are fake.
#![allow(clippy::unwrap_used)]

use std::sync::Arc;
use std::time::Duration;

use navori_ffi::{awake_check, awake_view};
use navori_kernel::{DispatchError, Envelope, Kernel, ModuleState, RestartPolicy};
use navori_macos::fake::{FakeExitsHandle, FakePower, FakeProcesses};
use navori_macos::{PowerAssertions, ProcessSource};
use navori_mod_awake::{Awake, ExitsFactory, Platform, Timing};
use navori_store::Store;

const SYSTEM_PROBLEM: &str = "macOS refused the request. Navori keeps trying.";
const STOPPED: &str = "The Awake module stopped.";
const NAME: &str = "Navori: keeping this Mac awake";

fn kernel_with(power: Arc<FakePower>) -> Kernel {
    start_kernel(power, RestartPolicy::default())
}

fn start_kernel(power: Arc<FakePower>, policy: RestartPolicy) -> Kernel {
    let processes = Arc::new(FakeProcesses::new(501));
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
        .restart_policy(policy)
        .register(Awake::new(platform, NAME, Timing::FIXED))
        .start()
}

fn fast_fail() -> RestartPolicy {
    RestartPolicy {
        max_restarts: 0,
        base_delay: Duration::from_millis(1),
        max_delay: Duration::from_millis(1),
    }
}

async fn until_failed(kernel: &Kernel) {
    for _ in 0..50 {
        let failed = kernel.modules().into_iter().any(|module| {
            module.id == "awake" && matches!(module.state, ModuleState::Failed { .. })
        });
        if failed {
            return;
        }
        tokio::task::yield_now().await;
    }
    panic!("awake did not fail: {:?}", kernel.modules());
}

async fn hold_for_one_hour(kernel: &Kernel) {
    kernel
        .dispatch(Envelope {
            module: "awake".into(),
            action: serde_json::json!({ "hold_for": { "seconds": 3600 } }),
        })
        .await
        .unwrap();
}

#[tokio::test]
async fn the_awake_row_fails_while_system_is_refused() {
    let power = Arc::new(FakePower::new());
    power.refuse(4);
    let kernel = kernel_with(power);
    hold_for_one_hour(&kernel).await;

    let view = awake_view(&kernel).unwrap();
    let row = awake_check(Some(&view));

    assert_eq!(row.name, "awake");
    assert!(!row.ok);
    assert_eq!(row.detail, SYSTEM_PROBLEM);
    assert_ne!(row.detail, "1 Lease");
    kernel.shutdown().await;
}

#[tokio::test]
async fn the_view_never_reports_held_while_refused() {
    let power = Arc::new(FakePower::new());
    power.refuse(4);
    let kernel = kernel_with(power);
    hold_for_one_hour(&kernel).await;

    let view = awake_view(&kernel).unwrap();
    assert_eq!(view.leases.len(), 1);
    assert!(!view.system_held);
    assert!(!view.display_held);
    assert_eq!(view.problem.as_deref(), Some(SYSTEM_PROBLEM));

    let state = kernel.state("awake").unwrap();
    assert_eq!(state["system_held"], false);
    assert_eq!(state["display_held"], false);
    assert_eq!(state["problem"], serde_json::json!(SYSTEM_PROBLEM));

    let row = awake_check(Some(&view));
    assert!(!row.ok);
    assert_eq!(row.detail, SYSTEM_PROBLEM);
    kernel.shutdown().await;
}

#[tokio::test]
async fn a_failed_module_is_never_reported_as_holding() {
    let power = Arc::new(FakePower::new());
    let kernel = start_kernel(Arc::clone(&power), fast_fail());
    hold_for_one_hour(&kernel).await;
    assert_eq!(
        kernel.state("awake").unwrap()["system_held"],
        serde_json::json!(true)
    );

    power.panic_on_hold(1);
    let error = kernel
        .dispatch(Envelope {
            module: "awake".into(),
            action: serde_json::json!({ "display": { "on": true } }),
        })
        .await
        .unwrap_err();
    assert_eq!(error, DispatchError::Unavailable("awake".into()));
    until_failed(&kernel).await;

    let published = kernel.state("awake").unwrap();
    assert_eq!(published["system_held"], serde_json::json!(true));
    let view = awake_view(&kernel).unwrap();
    assert!(!view.system_held);
    assert!(!view.display_held);
    assert_eq!(view.problem.as_deref(), Some(STOPPED));
    assert_eq!(view.leases.len(), 1);
    kernel.shutdown().await;

    let power = Arc::new(FakePower::new());
    let kernel = kernel_with(power);
    hold_for_one_hour(&kernel).await;
    assert_eq!(
        kernel.state("awake").unwrap()["system_held"],
        serde_json::json!(true)
    );
    kernel.shutdown().await;
    assert!(
        kernel
            .modules()
            .into_iter()
            .any(|module| { module.id == "awake" && module.state == ModuleState::Stopped })
    );
    let published = kernel.state("awake").unwrap();
    assert_eq!(published["system_held"], serde_json::json!(true));
    let view = awake_view(&kernel).unwrap();
    assert!(!view.system_held);
    assert!(!view.display_held);
    assert_eq!(view.problem.as_deref(), Some(STOPPED));
    assert_eq!(view.leases.len(), 1);
}

#[tokio::test]
async fn the_awake_row_fails_when_the_module_failed() {
    let power = Arc::new(FakePower::new());
    let kernel = start_kernel(Arc::clone(&power), fast_fail());
    hold_for_one_hour(&kernel).await;
    power.panic_on_hold(1);
    kernel
        .dispatch(Envelope {
            module: "awake".into(),
            action: serde_json::json!({ "display": { "on": true } }),
        })
        .await
        .unwrap_err();
    until_failed(&kernel).await;

    let view = awake_view(&kernel).unwrap();
    let row = awake_check(Some(&view));
    assert_eq!(row.name, "awake");
    assert!(!row.ok);
    assert_eq!(row.detail, STOPPED);
    assert_ne!(row.detail, "1 Lease");
    assert_ne!(row.detail, "Awake's state could not be read.");
    kernel.shutdown().await;
}

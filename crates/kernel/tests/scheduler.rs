//! Wait handle behavior observed through the kernel's public API.
#![allow(clippy::unwrap_used)] // helpers outside #[test] fns

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::task::Poll;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use navori_kernel::{
    ActionDoc, DispatchError, Envelope, Kernel, Module, ModuleAction, ModuleCtx, RestartPolicy,
    Risk, Wake, WallClock,
};
use navori_store::Store;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

const ORIGIN_MS: u64 = 1_700_000_000_000;

fn wall(millis: u64) -> SystemTime {
    UNIX_EPOCH + Duration::from_millis(millis)
}

#[derive(Clone, Debug)]
struct FakeClock {
    millis: Arc<AtomicU64>,
}

impl FakeClock {
    fn at(millis: u64) -> Self {
        Self {
            millis: Arc::new(AtomicU64::new(millis)),
        }
    }

    fn advance(&self, by: Duration) {
        let millis = u64::try_from(by.as_millis()).unwrap();
        self.millis.fetch_add(millis, Ordering::SeqCst);
    }
}

impl WallClock for FakeClock {
    fn now(&self) -> SystemTime {
        wall(self.millis.load(Ordering::SeqCst))
    }
}

fn kernel_with(clock: &FakeClock) -> Kernel {
    Kernel::builder_with_clock(Store::open_in_memory().unwrap(), Arc::new(clock.clone()))
        .dispatch_timeout(Duration::from_secs(30))
        .start()
}

fn state_of(kernel: &Kernel, module: &str) -> Value {
    kernel.state(module).unwrap()
}

async fn until_state(kernel: &Kernel, module: &str, pred: impl Fn(&Value) -> bool) {
    for _ in 0..50 {
        if kernel.state(module).as_ref().is_some_and(&pred) {
            return;
        }
        tokio::task::yield_now().await;
    }
    panic!("state not reached: {:?}", kernel.state(module));
}

async fn drain() {
    for _ in 0..20 {
        tokio::task::yield_now().await;
    }
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
enum PingAction {
    /// Answer while a wait is in progress.
    Ping,
}

impl ModuleAction for PingAction {
    fn risk(&self) -> Risk {
        Risk::Read
    }

    fn catalog() -> Vec<ActionDoc> {
        vec![ActionDoc {
            name: "ping",
            description: "Answer while a wait is in progress.",
            risk: Risk::Read,
        }]
    }
}

#[derive(Debug, Clone, Default, Serialize)]
struct PingState {
    starts: u32,
    due: bool,
}

#[derive(Debug)]
struct Pinger {
    deadline: SystemTime,
}

impl Module for Pinger {
    const ID: &'static str = "pinger";
    type Action = PingAction;
    type State = PingState;

    fn initial_state(&self) -> PingState {
        PingState::default()
    }

    async fn run(&mut self, ctx: &mut ModuleCtx<Self>) -> anyhow::Result<()> {
        ctx.update_state(|s| s.starts += 1);
        let mut handle = ctx.wait_handle();
        loop {
            tokio::select! {
                req = ctx.recv() => {
                    let Some(req) = req else { return Ok(()) };
                    match req.action {
                        PingAction::Ping => req.respond(Ok("ok")),
                    }
                }
                wake = handle.wait_until(Some(self.deadline)) => {
                    if matches!(wake, Wake::Due) {
                        ctx.update_state(|s| s.due = true);
                    }
                    return Ok(());
                }
            }
        }
    }
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
enum CrashAction {
    /// Panic while a wait is armed.
    Crash,
}

impl ModuleAction for CrashAction {
    fn risk(&self) -> Risk {
        Risk::Destructive
    }

    fn catalog() -> Vec<ActionDoc> {
        vec![ActionDoc {
            name: "crash",
            description: "Panic while a wait is armed.",
            risk: Risk::Destructive,
        }]
    }
}

#[derive(Debug, Clone, Default, Serialize)]
struct CrashState {
    starts: u32,
    old_due: bool,
    new_due: bool,
}

#[derive(Debug)]
struct Crasher {
    origin: SystemTime,
    runs: u32,
}

impl Module for Crasher {
    const ID: &'static str = "crasher";
    type Action = CrashAction;
    type State = CrashState;

    fn initial_state(&self) -> CrashState {
        CrashState::default()
    }

    async fn run(&mut self, ctx: &mut ModuleCtx<Self>) -> anyhow::Result<()> {
        let first = self.runs == 0;
        self.runs += 1;
        ctx.update_state(|s| s.starts = self.runs);
        let deadline = if first {
            self.origin + Duration::from_secs(5)
        } else {
            self.origin + Duration::from_secs(30)
        };
        let mut handle = ctx.wait_handle();
        tokio::select! {
            biased;
            req = ctx.recv() => {
                let Some(req) = req else { return Ok(()) };
                match req.action {
                    CrashAction::Crash => panic!("boom"),
                }
            }
            wake = handle.wait_until(Some(deadline)) => {
                if matches!(wake, Wake::Due) {
                    ctx.update_state(|s| {
                        if first {
                            s.old_due = true;
                        } else {
                            s.new_due = true;
                        }
                    });
                }
                Ok(())
            }
        }
    }
}

#[tokio::test(start_paused = true)]
async fn a_wait_returns_due_at_its_deadline() {
    let clock = FakeClock::at(ORIGIN_MS);
    let kernel = kernel_with(&clock);
    let mut handle = kernel.scheduler().wait_handle();
    let deadline = wall(ORIGIN_MS + 10_000);
    let mut wait = std::pin::pin!(handle.wait_until(Some(deadline)));

    assert_eq!(futures::poll!(wait.as_mut()), Poll::Pending);
    clock.advance(Duration::from_secs(9));
    tokio::time::advance(Duration::from_secs(9)).await;
    assert_eq!(futures::poll!(wait.as_mut()), Poll::Pending);

    clock.advance(Duration::from_secs(1));
    tokio::time::advance(Duration::from_secs(1)).await;
    assert_eq!(futures::poll!(wait.as_mut()), Poll::Ready(Wake::Due));
    kernel.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn a_wait_with_no_deadline_stays_pending() {
    let clock = FakeClock::at(ORIGIN_MS);
    let kernel = kernel_with(&clock);
    let mut handle = kernel.scheduler().wait_handle();
    let mut wait = std::pin::pin!(handle.wait_until(None));

    assert_eq!(futures::poll!(wait.as_mut()), Poll::Pending);
    tokio::time::advance(Duration::from_secs(24 * 60 * 60)).await;
    assert_eq!(futures::poll!(wait.as_mut()), Poll::Pending);
    kernel.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn a_timer_that_ends_before_the_wall_deadline_arms_again() {
    let clock = FakeClock::at(ORIGIN_MS);
    let kernel = kernel_with(&clock);
    let mut handle = kernel.scheduler().wait_handle();
    let deadline = wall(ORIGIN_MS + 10_000);
    let mut wait = std::pin::pin!(handle.wait_until(Some(deadline)));

    assert_eq!(futures::poll!(wait.as_mut()), Poll::Pending);
    tokio::time::advance(Duration::from_secs(10)).await;
    assert_eq!(futures::poll!(wait.as_mut()), Poll::Pending);

    clock.advance(Duration::from_secs(10));
    assert_eq!(futures::poll!(wait.as_mut()), Poll::Pending);
    tokio::time::advance(Duration::from_secs(10)).await;
    assert_eq!(futures::poll!(wait.as_mut()), Poll::Ready(Wake::Due));
    kernel.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn a_wait_runs_in_the_same_select_as_recv() {
    let clock = FakeClock::at(ORIGIN_MS);
    let kernel =
        Kernel::builder_with_clock(Store::open_in_memory().unwrap(), Arc::new(clock.clone()))
            .register(Pinger {
                deadline: wall(ORIGIN_MS + 5_000),
            })
            .start();

    until_state(&kernel, "pinger", |s| s["starts"] == 1).await;
    assert_eq!(state_of(&kernel, "pinger")["due"], false);

    let answer = kernel
        .dispatch(Envelope {
            module: "pinger".into(),
            action: json!("ping"),
        })
        .await
        .unwrap();
    assert_eq!(answer, json!("ok"));
    assert_eq!(state_of(&kernel, "pinger")["due"], false);

    clock.advance(Duration::from_secs(5));
    tokio::time::advance(Duration::from_secs(5)).await;
    until_state(&kernel, "pinger", |s| s["due"] == true).await;
    kernel.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn dropping_a_wait_and_waiting_again_still_returns_due() {
    let clock = FakeClock::at(ORIGIN_MS);
    let kernel = kernel_with(&clock);
    let mut handle = kernel.scheduler().wait_handle();
    let deadline = wall(ORIGIN_MS + 10_000);
    {
        let mut wait = std::pin::pin!(handle.wait_until(Some(deadline)));
        assert_eq!(futures::poll!(wait.as_mut()), Poll::Pending);
    }

    let mut wait = std::pin::pin!(handle.wait_until(Some(deadline)));
    assert_eq!(futures::poll!(wait.as_mut()), Poll::Pending);
    clock.advance(Duration::from_secs(10));
    tokio::time::advance(Duration::from_secs(10)).await;
    assert_eq!(futures::poll!(wait.as_mut()), Poll::Ready(Wake::Due));
    kernel.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn a_panicking_run_leaves_no_timer_armed_and_restarts_with_a_new_handle() {
    let clock = FakeClock::at(ORIGIN_MS);
    let kernel =
        Kernel::builder_with_clock(Store::open_in_memory().unwrap(), Arc::new(clock.clone()))
            .restart_policy(RestartPolicy {
                max_restarts: 3,
                base_delay: Duration::ZERO,
                max_delay: Duration::ZERO,
            })
            .register(Crasher {
                origin: wall(ORIGIN_MS),
                runs: 0,
            })
            .start();

    until_state(&kernel, "crasher", |s| s["starts"] == 1).await;
    let err = kernel
        .dispatch(Envelope {
            module: "crasher".into(),
            action: json!("crash"),
        })
        .await
        .unwrap_err();
    assert_eq!(err, DispatchError::Unavailable("crasher".into()));
    until_state(&kernel, "crasher", |s| s["starts"] == 2).await;

    clock.advance(Duration::from_secs(5));
    tokio::time::advance(Duration::from_secs(5)).await;
    drain().await;
    let mid = state_of(&kernel, "crasher");
    assert_eq!(mid["old_due"], false);
    assert_eq!(mid["new_due"], false);

    clock.advance(Duration::from_secs(25));
    tokio::time::advance(Duration::from_secs(25)).await;
    until_state(&kernel, "crasher", |s| s["new_due"] == true).await;
    assert_eq!(state_of(&kernel, "crasher")["old_due"], false);
    kernel.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn wait_handles_add_no_runtime_task() {
    let clock = FakeClock::at(ORIGIN_MS);
    let kernel = kernel_with(&clock);
    let before = tokio::runtime::Handle::current()
        .metrics()
        .num_alive_tasks();
    let mut handles = Vec::new();
    for _ in 0..10 {
        handles.push(kernel.scheduler().wait_handle());
    }
    let after = tokio::runtime::Handle::current()
        .metrics()
        .num_alive_tasks();
    assert_eq!(before, after, "live tasks {before} then {after}");
    assert_eq!(handles.len(), 10);
    kernel.shutdown().await;
}

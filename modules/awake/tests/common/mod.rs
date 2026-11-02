#![allow(dead_code, clippy::unwrap_used)]

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use navori_kernel::{DispatchError, Envelope, Kernel, WallClock};
use navori_macos::fake::{FakeExitsHandle, FakePower, FakeProcesses};
use navori_macos::{PowerAssertions, ProcessSource};
use navori_mod_awake::{Awake, ExitsFactory, Platform, Timing};
use navori_store::Store;
use serde_json::Value;

pub const ORIGIN_MS: u64 = 1_700_000_000_000;
pub const ASSERTION_NAME: &str = "Navori: keeping this Mac awake";

fn wall(millis: u64) -> SystemTime {
    UNIX_EPOCH + Duration::from_millis(millis)
}

#[derive(Clone, Debug)]
pub struct FakeClock {
    millis: Arc<AtomicU64>,
}

impl FakeClock {
    pub fn at(millis: u64) -> Self {
        Self {
            millis: Arc::new(AtomicU64::new(millis)),
        }
    }

    pub fn advance(&self, by: Duration) {
        let millis = u64::try_from(by.as_millis()).unwrap();
        self.millis.fetch_add(millis, Ordering::SeqCst);
    }
}

impl WallClock for FakeClock {
    fn now(&self) -> SystemTime {
        wall(self.millis.load(Ordering::SeqCst))
    }
}

pub struct Rig {
    pub kernel: Kernel,
    pub power: Arc<FakePower>,
    pub processes: Arc<FakeProcesses>,
    pub exits: FakeExitsHandle,
    pub clock: FakeClock,
    source: Arc<dyn ProcessSource>,
    store: Store,
    timing: Timing,
}

impl Rig {
    pub fn new() -> Self {
        Self::with_timing(Timing::FIXED)
    }

    pub fn with_timing(timing: Timing) -> Self {
        let processes = Arc::new(FakeProcesses::new(501));
        let source = Arc::clone(&processes) as Arc<dyn ProcessSource>;
        let exits = FakeExitsHandle::new();
        let factory = {
            let exits = exits.clone();
            Box::new(move |sink| exits.clone().factory()(sink)) as ExitsFactory
        };
        Self::assemble(processes, source, exits, factory, timing)
    }

    pub fn with_source(processes: Arc<FakeProcesses>, source: Arc<dyn ProcessSource>) -> Self {
        let exits = FakeExitsHandle::new();
        let factory = {
            let exits = exits.clone();
            Box::new(move |sink| exits.clone().factory()(sink)) as ExitsFactory
        };
        Self::assemble(processes, source, exits, factory, Timing::FIXED)
    }

    pub fn with_exits_factory(factory: ExitsFactory) -> Self {
        let processes = Arc::new(FakeProcesses::new(501));
        let source = Arc::clone(&processes) as Arc<dyn ProcessSource>;
        Self::assemble(
            processes,
            source,
            FakeExitsHandle::new(),
            factory,
            Timing::FIXED,
        )
    }

    fn assemble(
        processes: Arc<FakeProcesses>,
        source: Arc<dyn ProcessSource>,
        exits: FakeExitsHandle,
        factory: ExitsFactory,
        timing: Timing,
    ) -> Self {
        let clock = FakeClock::at(ORIGIN_MS);
        let power = Arc::new(FakePower::new());
        let store = Store::open_in_memory().unwrap();
        let platform = Platform::new(
            Arc::clone(&power) as Arc<dyn PowerAssertions>,
            Box::new({
                let source = Arc::clone(&source);
                move || Ok(Arc::clone(&source))
            }),
            factory,
        );
        let kernel = Kernel::builder_with_clock(store.clone(), Arc::new(clock.clone()))
            .pause_journal_sweep()
            .register(Awake::new(platform, ASSERTION_NAME, timing))
            .start();
        Self {
            kernel,
            power,
            processes,
            exits,
            clock,
            source,
            store,
            timing,
        }
    }

    /// Shut the running kernel down and start another on the same store and fakes.
    pub async fn reopen(&mut self) {
        self.shutdown().await;
        let source = Arc::clone(&self.source);
        let exits = self.exits.clone();
        let factory = Box::new(move |sink| exits.factory()(sink)) as ExitsFactory;
        let platform = Platform::new(
            Arc::clone(&self.power) as Arc<dyn PowerAssertions>,
            Box::new(move || Ok(Arc::clone(&source))),
            factory,
        );
        self.kernel = Kernel::builder_with_clock(self.store.clone(), Arc::new(self.clock.clone()))
            .pause_journal_sweep()
            .register(Awake::new(platform, ASSERTION_NAME, self.timing))
            .start();
        self.idle().await;
    }

    pub async fn dispatch(&self, action: Value) -> Result<Value, DispatchError> {
        self.kernel
            .dispatch(Envelope {
                module: "awake".into(),
                action,
            })
            .await
    }

    pub async fn advance(&self, by: Duration) {
        self.clock.advance(by);
        tokio::time::advance(by).await;
    }

    pub async fn idle(&self) {
        for _ in 0..30 {
            tokio::task::yield_now().await;
        }
    }

    pub async fn shutdown(&self) {
        self.kernel.shutdown().await;
    }
}

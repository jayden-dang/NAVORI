#![allow(dead_code, clippy::unwrap_used)]

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use navori_config::JournalSweep;
use navori_kernel::{Kernel, WallClock};
use navori_mod_todo::{LocalCalendar, Todo, TodoEnv};
use navori_store::Store;
use serde_json::{Value, json};

pub const ORIGIN_MS: u64 = 1_700_000_000_000;
/// Fixed offset used by the fake calendar: ten hours east of UTC.
pub const OFFSET_SECS: i32 = 10 * 60 * 60;

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

    pub fn millis(&self) -> u64 {
        self.millis.load(Ordering::SeqCst)
    }

    pub fn advance(&self, by: Duration) {
        let millis = u64::try_from(by.as_millis()).unwrap();
        self.millis.fetch_add(millis, Ordering::SeqCst);
    }
}

impl WallClock for FakeClock {
    fn now(&self) -> SystemTime {
        UNIX_EPOCH + Duration::from_millis(self.millis.load(Ordering::SeqCst))
    }
}

#[derive(Debug)]
pub struct FixedCalendar {
    offset_secs: i32,
}

impl FixedCalendar {
    pub fn new(offset_secs: i32) -> Self {
        Self { offset_secs }
    }
}

impl LocalCalendar for FixedCalendar {
    fn utc_offset(&self, _at: SystemTime) -> i32 {
        self.offset_secs
    }
}

pub struct Harness {
    pub kernel: Kernel,
    pub clock: FakeClock,
    pub store: Store,
}

pub fn harness() -> Harness {
    let clock = FakeClock::at(ORIGIN_MS);
    let store = Store::open_in_memory().unwrap();
    let calendar: Arc<dyn LocalCalendar> = Arc::new(FixedCalendar::new(OFFSET_SECS));
    let kernel = Kernel::builder_with_clock(store.clone(), Arc::new(clock.clone()))
        .pause_journal_sweep()
        .register(Todo::new(TodoEnv { calendar }, JournalSweep::Paused))
        .start();
    Harness {
        kernel,
        clock,
        store,
    }
}

impl Harness {
    pub async fn add(
        &self,
        title: &str,
        due_on: Option<&str>,
    ) -> Result<Value, navori_kernel::DispatchError> {
        let mut body = json!({ "title": title });
        if let Some(due_on) = due_on {
            body["due_on"] = json!(due_on);
        }
        self.kernel
            .dispatch(navori_kernel::Envelope {
                module: "todo".into(),
                action: json!({ "add": body }),
            })
            .await
    }

    pub fn state(&self) -> Value {
        self.kernel.state("todo").unwrap()
    }

    pub async fn idle(&self) {
        for _ in 0..30 {
            tokio::task::yield_now().await;
        }
    }

    pub async fn settle(&self) {
        for _ in 0..100 {
            tokio::task::yield_now().await;
        }
    }

    pub async fn reopen(&mut self) {
        self.kernel.shutdown().await;
        let calendar: Arc<dyn LocalCalendar> = Arc::new(FixedCalendar::new(OFFSET_SECS));
        self.kernel = Kernel::builder_with_clock(self.store.clone(), Arc::new(self.clock.clone()))
            .pause_journal_sweep()
            .register(Todo::new(TodoEnv { calendar }, JournalSweep::Paused))
            .start();
        self.idle().await;
    }

    pub async fn shutdown(self) {
        self.kernel.shutdown().await;
    }

    pub async fn advance(&self, by: Duration) {
        self.clock.advance(by);
        tokio::time::advance(by).await;
    }
}

/// Local calendar day at ORIGIN_MS under OFFSET_SECS: 2023-11-15.
pub fn today() -> &'static str {
    "2023-11-15"
}

pub fn day_plus(days: i64) -> String {
    // Independent of production date math: fixed literals around today().
    match days {
        -1 => "2023-11-14".into(),
        0 => today().into(),
        1 => "2023-11-16".into(),
        2 => "2023-11-17".into(),
        3 => "2023-11-18".into(),
        4 => "2023-11-19".into(),
        _ => panic!("day_plus fixture only covers -1..=4"),
    }
}

/// Wall duration from ORIGIN_MS to the next local midnight under OFFSET_SECS.
pub fn until_next_local_midnight() -> Duration {
    // Local now: 2023-11-15 08:13:20 (+10). Next midnight local: 2023-11-16 00:00 +10
    // = 2023-11-15 14:00:00 UTC = ORIGIN + 56_800_000 ms.
    Duration::from_millis(56_800_000)
}

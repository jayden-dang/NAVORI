#![allow(dead_code, clippy::unwrap_used)]

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use navori_kernel::{DispatchError, Envelope, Kernel, WallClock};
use navori_macos::fake::FakeProcesses;
use navori_macos::{Listener, Marker, PlatformError, ProcEntry, ProcessSource, ReadError};
use navori_mod_ports::{Ports, PortsEnv};
use navori_store::Store;
use serde_json::Value;

pub const ORIGIN_MS: u64 = 1_700_000_000_000;

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
        UNIX_EPOCH + Duration::from_millis(self.millis.load(Ordering::SeqCst))
    }
}

/// A temporary directory and its canonical path, since macOS reports `/var/...` as `/private/var/...`.
pub fn canonical_temp_home() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let home = std::fs::canonicalize(dir.path()).unwrap();
    (dir, home)
}

/// A process source that can be made to refuse, to wait, or to panic once.
pub struct Gate {
    inner: Arc<dyn ProcessSource>,
    denied: AtomicBool,
    env_denied: AtomicBool,
    panic_once: AtomicBool,
    hold: Mutex<()>,
}

impl Gate {
    pub fn new(inner: Arc<dyn ProcessSource>) -> Self {
        Self {
            inner,
            denied: AtomicBool::new(false),
            env_denied: AtomicBool::new(false),
            panic_once: AtomicBool::new(false),
            hold: Mutex::new(()),
        }
    }

    pub fn deny(&self, denied: bool) {
        self.denied.store(denied, Ordering::SeqCst);
    }

    /// The next environment read is refused; the ones after it are not.
    pub fn deny_env_once(&self) {
        self.env_denied.store(true, Ordering::SeqCst);
    }

    pub fn panic_once(&self) {
        self.panic_once.store(true, Ordering::SeqCst);
    }

    /// Every `list` waits until the returned guard is dropped.
    pub fn block(&self) -> MutexGuard<'_, ()> {
        self.hold.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl ProcessSource for Gate {
    fn current_uid(&self) -> u32 {
        self.inner.current_uid()
    }

    fn list(&self) -> Result<Vec<ProcEntry>, PlatformError> {
        drop(self.hold.lock().unwrap_or_else(PoisonError::into_inner));
        assert!(
            !self.panic_once.swap(false, Ordering::SeqCst),
            "the gate was set to panic"
        );
        if self.denied.load(Ordering::SeqCst) {
            return Err(PlatformError::Sysctl { errno: 1 });
        }
        self.inner.list()
    }

    fn entry(&self, pid: i32) -> Result<ProcEntry, ReadError> {
        self.inner.entry(pid)
    }

    fn exe(&self, pid: i32) -> Result<PathBuf, ReadError> {
        self.inner.exe(pid)
    }

    fn argv(&self, pid: i32) -> Result<Vec<String>, ReadError> {
        self.inner.argv(pid)
    }

    fn cwd(&self, pid: i32) -> Result<PathBuf, ReadError> {
        self.inner.cwd(pid)
    }

    fn cpu_time(&self, pid: i32) -> Result<Duration, ReadError> {
        self.inner.cpu_time(pid)
    }

    fn listeners(&self, pid: i32) -> Result<Vec<Listener>, ReadError> {
        self.inner.listeners(pid)
    }

    fn env_marker(&self, pid: i32, names: &[&str]) -> Result<Marker, ReadError> {
        if self.env_denied.swap(false, Ordering::SeqCst) {
            return Err(ReadError::Denied);
        }
        self.inner.env_marker(pid, names)
    }

    fn name(&self, pid: i32) -> Result<String, ReadError> {
        self.inner.name(pid)
    }
}

pub struct Rig {
    pub kernel: Kernel,
    pub processes: Arc<FakeProcesses>,
    pub gate: Arc<Gate>,
    pub clock: FakeClock,
    pub home: PathBuf,
    _home: tempfile::TempDir,
}

impl Rig {
    pub fn new() -> Self {
        let processes = Arc::new(FakeProcesses::new(501));
        let gate = Arc::new(Gate::new(Arc::clone(&processes) as Arc<dyn ProcessSource>));
        let (guard, home) = canonical_temp_home();
        let clock = FakeClock::at(ORIGIN_MS);
        let source = Arc::clone(&gate) as Arc<dyn ProcessSource>;
        let env = PortsEnv {
            home: home.clone(),
            processes: Box::new(move || Ok(Arc::clone(&source))),
        };
        let kernel =
            Kernel::builder_with_clock(Store::open_in_memory().unwrap(), Arc::new(clock.clone()))
                .pause_journal_sweep()
                .register(Ports::new(env))
                .start();
        Self {
            kernel,
            processes,
            gate,
            clock,
            home,
            _home: guard,
        }
    }

    pub async fn dispatch(&self, action: Value) -> Result<Value, DispatchError> {
        self.kernel
            .dispatch(Envelope {
                module: "ports".into(),
                action,
            })
            .await
    }

    pub async fn advance(&self, by: Duration) {
        self.idle().await;
        self.clock.advance(by);
        tokio::time::advance(by).await;
        self.idle().await;
    }

    pub async fn idle(&self) {
        for _ in 0..30 {
            tokio::task::yield_now().await;
        }
    }

    /// Waits for the blocking scan thread to hand its result back.
    pub async fn settle(&self) {
        for _ in 0..200 {
            self.idle().await;
            if self.state()["scanned_at_ms"].is_number() || self.state()["problem"].is_string() {
                return;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        panic!("no scan finished: {}", self.state());
    }

    pub fn state(&self) -> Value {
        self.kernel.state("ports").unwrap()
    }
}

pub fn under(home: &Path, rest: &str) -> PathBuf {
    home.join(rest)
}

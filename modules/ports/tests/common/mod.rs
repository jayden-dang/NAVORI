#![allow(dead_code, clippy::unwrap_used)]

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use futures::future::BoxFuture;
use navori_kernel::{DispatchError, Envelope, Kernel, WallClock};
use navori_macos::fake::{FakeProcesses, FakeSignals};
use navori_macos::{Listener, Marker, PlatformError, ProcEntry, ProcessSource, ReadError, Signals};
use navori_mod_ports::{Container, Containers, DockerError, Ports, PortsEnv, Stopped};
use navori_store::Store;
use serde_json::Value;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixListener;
use tokio::task::JoinHandle;

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
    attempts: AtomicUsize,
    finished: AtomicUsize,
}

impl Gate {
    pub fn new(inner: Arc<dyn ProcessSource>) -> Self {
        Self {
            inner,
            denied: AtomicBool::new(false),
            env_denied: AtomicBool::new(false),
            panic_once: AtomicBool::new(false),
            hold: Mutex::new(()),
            attempts: AtomicUsize::new(0),
            finished: AtomicUsize::new(0),
        }
    }

    /// How many times the process list was asked for, whether or not it was refused.
    pub fn attempts(&self) -> usize {
        self.attempts.load(Ordering::SeqCst)
    }

    /// Waits until `count` process-list reads have returned to their callers.
    pub async fn finished(&self, count: usize) {
        for _ in 0..400 {
            if self.finished.load(Ordering::SeqCst) >= count {
                return;
            }
            tokio::task::yield_now().await;
            std::thread::sleep(Duration::from_millis(1));
        }
        panic!("the gate never finished {count} reads");
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
        self.attempts.fetch_add(1, Ordering::SeqCst);
        drop(self.hold.lock().unwrap_or_else(PoisonError::into_inner));
        assert!(
            !self.panic_once.swap(false, Ordering::SeqCst),
            "the gate was set to panic"
        );
        let listed = if self.denied.load(Ordering::SeqCst) {
            Err(PlatformError::Sysctl { errno: 1 })
        } else {
            self.inner.list()
        };
        self.finished.fetch_add(1, Ordering::SeqCst);
        listed
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

/// A Unix-socket server that records the request head, then lets `reply` answer.
pub struct Served {
    pub _dir: tempfile::TempDir,
    pub path: PathBuf,
    pub request: JoinHandle<String>,
}

pub fn serve(
    reply: impl FnOnce(tokio::net::UnixStream) -> JoinHandle<()> + Send + 'static,
) -> Served {
    let guard = tempfile::Builder::new()
        .prefix("nv")
        .tempdir_in("/tmp")
        .unwrap();
    let path = std::fs::canonicalize(guard.path()).unwrap().join("d.sock");
    let listener = UnixListener::bind(&path).unwrap();
    let request = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut head = Vec::new();
        let mut byte = [0u8; 1];
        while !head.ends_with(b"\r\n\r\n") {
            if stream.read(&mut byte).await.unwrap() == 0 {
                break;
            }
            head.push(byte[0]);
        }
        let _ = reply(stream).await;
        String::from_utf8(head).unwrap()
    });
    Served {
        _dir: guard,
        path,
        request,
    }
}

pub fn say(
    bytes: Vec<u8>,
) -> impl FnOnce(tokio::net::UnixStream) -> JoinHandle<()> + Send + 'static {
    move |mut stream| {
        tokio::spawn(async move {
            let _ = stream.write_all(&bytes).await;
            let _ = stream.shutdown().await;
        })
    }
}

/// A Docker engine that answers from a script and counts the reads.
pub struct FakeContainers {
    reply: Mutex<Result<Vec<Container>, DockerError>>,
    calls: AtomicUsize,
    stop_reply: Mutex<Result<Stopped, DockerError>>,
    stop_delay: Mutex<Duration>,
    stopped: Mutex<Vec<String>>,
}

impl FakeContainers {
    pub fn new() -> Self {
        Self {
            reply: Mutex::new(Ok(Vec::new())),
            calls: AtomicUsize::new(0),
            stop_reply: Mutex::new(Ok(Stopped::Stopped)),
            stop_delay: Mutex::new(Duration::ZERO),
            stopped: Mutex::new(Vec::new()),
        }
    }

    pub fn reply_stop(&self, reply: Result<Stopped, DockerError>) {
        *self
            .stop_reply
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = reply;
    }

    pub fn delay_stop(&self, delay: Duration) {
        *self
            .stop_delay
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = delay;
    }

    pub fn stops(&self) -> Vec<String> {
        self.stopped
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    pub fn reply(&self, reply: Result<Vec<Container>, DockerError>) {
        *self.reply.lock().unwrap_or_else(PoisonError::into_inner) = reply;
    }

    pub fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

impl Containers for FakeContainers {
    fn running(&self) -> BoxFuture<'_, Result<Vec<Container>, DockerError>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let reply = self
            .reply
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        Box::pin(async move { reply })
    }

    fn stop(&self, id: &str) -> BoxFuture<'_, Result<Stopped, DockerError>> {
        self.stopped
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(id.to_owned());
        let reply = *self
            .stop_reply
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let delay = *self
            .stop_delay
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        Box::pin(async move {
            if !delay.is_zero() {
                tokio::time::sleep(delay).await;
            }
            reply
        })
    }
}

pub struct Rig {
    pub kernel: Kernel,
    pub containers: Arc<FakeContainers>,
    pub processes: Arc<FakeProcesses>,
    pub signals: Arc<FakeSignals>,
    pub gate: Arc<Gate>,
    pub clock: FakeClock,
    pub home: PathBuf,
    _home: tempfile::TempDir,
}

impl Rig {
    pub fn new() -> Self {
        Self::opening(false, serde_json::Map::new())
    }

    /// A Module started with these config sections.
    pub fn with_config(sections: serde_json::Map<String, Value>) -> Self {
        Self::opening(false, sections)
    }

    /// A Module whose process source can never be opened.
    pub fn unopenable() -> Self {
        Self::opening(true, serde_json::Map::new())
    }

    fn opening(refused: bool, sections: serde_json::Map<String, Value>) -> Self {
        let processes = Arc::new(FakeProcesses::new(501));
        let gate = Arc::new(Gate::new(Arc::clone(&processes) as Arc<dyn ProcessSource>));
        let (guard, home) = canonical_temp_home();
        let clock = FakeClock::at(ORIGIN_MS);
        let source = Arc::clone(&gate) as Arc<dyn ProcessSource>;
        let containers = Arc::new(FakeContainers::new());
        let signals = Arc::new(FakeSignals::new(4242, 4242));
        let env = PortsEnv {
            home: home.clone(),
            processes: Box::new(move || {
                if refused {
                    Err(PlatformError::Sysctl { errno: 1 })
                } else {
                    Ok(Arc::clone(&source))
                }
            }),
            containers: Arc::clone(&containers) as Arc<dyn Containers>,
            signals: Arc::clone(&signals) as Arc<dyn Signals>,
            helper: None,
        };
        let kernel =
            Kernel::builder_with_clock(Store::open_in_memory().unwrap(), Arc::new(clock.clone()))
                .pause_journal_sweep()
                .config_sections(sections)
                .register(Ports::new(env))
                .start();
        Self {
            kernel,
            containers,
            processes,
            signals,
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

    /// Waits until a scan that began after the gate's attempt count was `mark` has published.
    pub async fn settle_scan_after(&self, mark: usize) {
        for _ in 0..200 {
            self.idle().await;
            if self.gate.attempts() > mark {
                std::thread::sleep(Duration::from_millis(20));
                self.idle().await;
                return;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        panic!("no scan began: {}", self.state());
    }

    /// Waits until a failed scan has published its problem.
    pub async fn settle_problem(&self) {
        for _ in 0..200 {
            self.idle().await;
            if self.state()["problem"].is_string() {
                return;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        panic!("no scan failed: {}", self.state());
    }

    /// Waits until the published scan carries this time.
    pub async fn settle_at(&self, scanned_at_ms: u64) {
        for _ in 0..200 {
            self.idle().await;
            if self.state()["scanned_at_ms"] == scanned_at_ms {
                return;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        panic!("no scan at {scanned_at_ms}: {}", self.state());
    }

    /// Lets a crashed Module restart, which publishes the not-scanned state.
    pub async fn settle_restart(&self) {
        for _ in 0..200 {
            self.advance(Duration::from_secs(1)).await;
            if self.state()["scanned_at_ms"].is_null() {
                return;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        panic!("the module did not restart: {}", self.state());
    }

    /// How many process-list reads happened after the count was `mark`.
    pub fn list_calls_since(&self, mark: usize) -> usize {
        self.processes.list_calls() - mark
    }

    pub fn state(&self) -> Value {
        self.kernel.state("ports").unwrap()
    }
}

pub fn under(home: &Path, rest: &str) -> PathBuf {
    home.join(rest)
}

/// Runs git in `dir` with no config from the machine and no `GIT_*` variable from the caller.
pub fn git(dir: &Path, args: &[&str]) {
    let mut cmd = Command::new("git");
    cmd.current_dir(dir)
        .args([
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@example.com",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(args);
    for (key, _) in std::env::vars() {
        if key.starts_with("GIT_") {
            cmd.env_remove(key);
        }
    }
    cmd.env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@example.com")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@example.com");
    let out = cmd.output().unwrap();
    assert!(out.status.success(), "git {args:?}: {out:?}");
}

pub fn init_repo(dir: &Path) {
    std::fs::create_dir_all(dir).unwrap();
    git(dir, &["init", "-b", "main"]);
    git(dir, &["commit", "--allow-empty", "-m", "init"]);
}

pub struct Captured(Arc<Mutex<Vec<u8>>>);

impl Write for Captured {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// Collects this thread's log lines, at every level, until the guard is dropped.
pub struct LogCapture {
    written: Arc<Mutex<Vec<u8>>>,
    _guard: tracing::subscriber::DefaultGuard,
}

impl LogCapture {
    pub fn start() -> Self {
        let written = Arc::new(Mutex::new(Vec::<u8>::new()));
        let sink = Arc::clone(&written);
        let subscriber = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::TRACE)
            .with_ansi(false)
            .with_writer(move || Captured(Arc::clone(&sink)))
            .finish();
        let guard = tracing::subscriber::set_default(subscriber);
        let capture = Self {
            written,
            _guard: guard,
        };
        capture.refresh();
        capture
    }

    /// Makes call sites that other tests already hit report to this capture too.
    pub fn refresh(&self) {
        tracing::callsite::rebuild_interest_cache();
    }

    pub fn text(&self) -> String {
        let bytes = self.written.lock().unwrap_or_else(PoisonError::into_inner);
        String::from_utf8_lossy(&bytes).into_owned()
    }
}

//! The UniFFI surface of the Rust core — the only crate the Swift shell sees — and the
//! composition root of Navori.app.
//!
//! Threading contract for Swift:
//! - `NavoriCore` owns its own tokio runtime; Swift never needs one.
//! - `SnapshotListener::on_snapshot` is called on a core worker thread. Hop to the main
//!   actor before touching UI state.
//! - `shutdown()` blocks (bounded, ≈ 5 s worst case); call it from `applicationWillTerminate`.

mod app;
mod logging;
mod session;

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock, PoisonError};
use std::time::Duration;

use navori_config::{Paths, Profile};
use navori_ipc::{IpcError, Server};
use navori_kernel::{DispatchError, Envelope, Risk, Severity};
use navori_store::Store;
use tokio::runtime::Runtime;
use tokio_util::sync::CancellationToken;

use crate::app::{Inner, IpcBridge};

pub use app::config_notice;

uniffi::setup_scaffolding!();

const RECENT_EVENTS_IN_SNAPSHOT: usize = 20;

#[derive(Debug, Clone, uniffi::Record)]
pub struct CoreConfig {
    /// `"release"` or `"dev"`.
    pub profile: String,
    pub home_dir: String,
    pub uid: u32,
    pub app_version: String,
    /// Write logs under `~/Library/Logs/Navori` (or `Navori Dev`). Off in tests.
    pub log_to_file: bool,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct ModuleView {
    pub id: String,
    /// `starting` | `running` | `restarting` | `failed` | `stopped`.
    pub state: String,
    /// Last error when restarting or failed.
    pub detail: Option<String>,
    pub restarts: u32,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct EventView {
    pub id: String,
    pub ts_ms: i64,
    pub module: String,
    pub kind: String,
    /// `debug` | `info` | `warn` | `error`.
    pub severity: String,
    pub summary: String,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct ActionView {
    pub module: String,
    pub name: String,
    pub description: String,
    /// `read` | `write` | `destructive`.
    pub risk: String,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct ConfigNotice {
    pub title: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct CheckView {
    pub name: String,
    pub ok: bool,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct CoreSnapshot {
    pub version: String,
    pub profile: String,
    pub pid: u32,
    pub started_at_ms: i64,
    pub uptime_ms: u64,
    pub data_dir: String,
    pub logs_dir: String,
    pub socket_path: String,
    /// Set when `config.toml` has bad values or fell back as a whole file.
    pub config_notice: Option<ConfigNotice>,
    /// Set when the previous session crashed or was killed.
    pub last_unclean_exit: Option<String>,
    pub modules: Vec<ModuleView>,
    /// Newest first.
    pub recent_events: Vec<EventView>,
}

#[derive(Debug, Clone, PartialEq, thiserror::Error, uniffi::Error)]
pub enum CoreError {
    #[error("startup failed: {message}")]
    Startup { message: String },
    #[error("invalid JSON: {message}")]
    InvalidJson { message: String },
    #[error("{message}")]
    Dispatch { message: String },
    #[error("core is shut down")]
    ShutDown,
}

/// Implemented by Swift (`CoreStore`). Called on a core worker thread.
#[uniffi::export(with_foreign)]
pub trait SnapshotListener: Send + Sync {
    fn on_snapshot(&self, snapshot: CoreSnapshot);
}

/// Background tasks and the runtime, torn down by `shutdown()`.
struct Running {
    runtime: Runtime,
    server: Server,
    pump_cancel: CancellationToken,
}

#[derive(uniffi::Object)]
pub struct NavoriCore {
    inner: Arc<Inner>,
    handle: tokio::runtime::Handle,
    running: Mutex<Option<Running>>,
    shut_down: AtomicBool,
}

impl std::fmt::Debug for NavoriCore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NavoriCore")
            .field("profile", &self.inner.profile)
            .finish_non_exhaustive()
    }
}

fn startup(context: &str, e: impl std::fmt::Display) -> CoreError {
    CoreError::Startup {
        message: format!("{context}: {e}"),
    }
}

#[uniffi::export]
impl NavoriCore {
    /// Boots the core: config → logging → store → kernel → IPC → session marker → pump.
    #[uniffi::constructor]
    pub fn start(
        config: CoreConfig,
        listener: Arc<dyn SnapshotListener>,
    ) -> Result<Arc<Self>, CoreError> {
        let profile = Profile::parse(&config.profile)
            .ok_or_else(|| startup("profile", format!("unknown profile {:?}", config.profile)))?;
        let paths = Paths::new(profile, &PathBuf::from(&config.home_dir), config.uid);
        for dir in [&paths.data_dir, &paths.logs_dir] {
            std::fs::create_dir_all(dir).map_err(|e| startup(&dir.display().to_string(), e))?;
        }

        let loaded = app::load_config(&paths);
        if config.log_to_file
            && let Err(e) = logging::init(&paths.logs_dir, &loaded.config.log_level)
        {
            eprintln!("navori: {e}");
        }

        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name("navori-core")
            .enable_all()
            .build()
            .map_err(|e| startup("runtime", e))?;
        let handle = runtime.handle().clone();

        let store =
            Store::open(&paths.db_file, &paths.backups_dir).map_err(|e| startup("database", e))?;
        let kernel = handle
            .block_on(async { app::build_kernel(store.clone(), &loaded.config, loaded.sections) });
        let config_notice = app::config_notice(
            &loaded.report,
            &loaded.sweep,
            kernel.config_issues(),
            &paths.config_file,
        );
        if let Some(notice) = &config_notice {
            tracing::warn!("{}. {}", notice.title, notice.message);
        }

        // The socket doubles as the single-instance lock: bind before touching the session marker.
        let pid = std::process::id();
        let inner = Arc::new(Inner {
            kernel,
            store,
            paths: paths.clone(),
            profile,
            version: config.app_version.clone(),
            pid,
            config_notice,
            last_unclean_exit: OnceLock::new(),
            panel_visible: AtomicBool::new(false),
        });
        let server = match handle.block_on(Server::bind(
            &paths.socket,
            Arc::new(IpcBridge(inner.clone())),
        )) {
            Ok(server) => server,
            Err(e) => {
                handle.block_on(inner.kernel.shutdown());
                runtime.shutdown_timeout(Duration::from_secs(1));
                return Err(match e {
                    IpcError::AlreadyRunning(_) => startup(
                        "single instance",
                        format!("Navori ({}) is already running", profile.as_str()),
                    ),
                    other => startup("ipc", other),
                });
            }
        };

        let previous = session::begin(&paths.data_dir, pid, inner.kernel.started_at_ms())
            .map_err(|e| startup("session marker", e))?;
        let _ = inner.last_unclean_exit.set(previous);
        app::announce_start(&inner);

        let pump_cancel = CancellationToken::new();
        let pump_inner = inner.clone();
        handle.spawn(app::snapshot_pump(
            inner.clone(),
            pump_cancel.clone(),
            move || {
                listener.on_snapshot(snapshot(&pump_inner));
            },
        ));

        tracing::info!(profile = profile.as_str(), version = %config.app_version, "core started");
        Ok(Arc::new(Self {
            inner,
            handle,
            running: Mutex::new(Some(Running {
                runtime,
                server,
                pump_cancel,
            })),
            shut_down: AtomicBool::new(false),
        }))
    }

    /// Current snapshot (also pushed to the listener on every change).
    pub fn status(&self) -> CoreSnapshot {
        snapshot(&self.inner)
    }

    pub fn actions(&self) -> Vec<ActionView> {
        self.inner
            .kernel
            .actions()
            .into_iter()
            .map(|a| ActionView {
                module: a.module,
                name: a.name,
                description: a.description,
                risk: risk_name(a.risk).to_owned(),
            })
            .collect()
    }

    pub fn doctor(&self) -> Vec<CheckView> {
        self.inner
            .doctor()
            .into_iter()
            .map(|c| CheckView {
                name: c.name,
                ok: c.ok,
                detail: c.detail,
            })
            .collect()
    }

    /// Sends `action_json` (the externally tagged action, e.g. `"ping"`) to `module` and
    /// returns the module's JSON answer.
    pub async fn dispatch(&self, module: String, action_json: String) -> Result<String, CoreError> {
        if self.shut_down.load(Ordering::SeqCst) {
            return Err(CoreError::ShutDown);
        }
        let action = serde_json::from_str(&action_json).map_err(|e| CoreError::InvalidJson {
            message: e.to_string(),
        })?;
        let (tx, rx) = futures::channel::oneshot::channel();
        let inner = self.inner.clone();
        self.handle.spawn(async move {
            let _ = tx.send(inner.kernel.dispatch(Envelope { module, action }).await);
        });
        let result = rx.await.map_err(|_| CoreError::ShutDown)?;
        let value = result.map_err(|e: DispatchError| CoreError::Dispatch {
            message: e.to_string(),
        })?;
        serde_json::to_string(&value).map_err(|e| CoreError::InvalidJson {
            message: e.to_string(),
        })
    }

    /// Faster snapshot pushes while the menubar panel is open.
    pub fn set_panel_visible(&self, visible: bool) {
        self.inner.panel_visible.store(visible, Ordering::Relaxed);
    }

    /// Stops modules, closes the socket, removes the session marker. Idempotent.
    pub fn shutdown(&self) {
        if self.shut_down.swap(true, Ordering::SeqCst) {
            return;
        }
        let running = self
            .running
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
        let Some(Running {
            runtime,
            server,
            pump_cancel,
        }) = running
        else {
            return;
        };
        self.inner.kernel.bus().emit(
            "core",
            "core.stopped",
            Severity::Info,
            "Navori stopped".to_owned(),
            serde_json::Value::Null,
        );
        pump_cancel.cancel();
        runtime.block_on(async {
            self.inner.kernel.shutdown().await;
            server.shutdown().await;
        });
        runtime.shutdown_timeout(Duration::from_secs(1));
        session::end(&self.inner.paths.data_dir);
        tracing::info!("core stopped");
    }
}

fn snapshot(inner: &Inner) -> CoreSnapshot {
    let status = inner.status();
    CoreSnapshot {
        version: status.version,
        profile: status.profile,
        pid: status.pid,
        started_at_ms: status.started_at_ms,
        uptime_ms: status.uptime_ms,
        data_dir: status.data_dir,
        logs_dir: inner.paths.logs_dir.display().to_string(),
        socket_path: status.socket_path,
        config_notice: inner.config_notice.clone(),
        last_unclean_exit: inner.last_unclean_exit.get().cloned().flatten(),
        modules: status
            .modules
            .iter()
            .map(|m| ModuleView {
                id: m.id.clone(),
                state: app::state_name(&m.state).to_owned(),
                detail: app::state_detail(m),
                restarts: m.restarts,
            })
            .collect(),
        recent_events: inner
            .recent_events(RECENT_EVENTS_IN_SNAPSHOT)
            .into_iter()
            .map(|e| EventView {
                severity: severity_name(e.severity).to_owned(),
                id: e.id,
                ts_ms: e.ts_ms,
                module: e.module,
                kind: e.kind,
                summary: e.summary,
            })
            .collect(),
    }
}

fn risk_name(r: Risk) -> &'static str {
    match r {
        Risk::Read => "read",
        Risk::Write => "write",
        Risk::Destructive => "destructive",
    }
}

fn severity_name(s: Severity) -> &'static str {
    match s {
        Severity::Debug => "debug",
        Severity::Info => "info",
        Severity::Warn => "warn",
        Severity::Error => "error",
    }
}

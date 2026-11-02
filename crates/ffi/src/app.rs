//! Composition root: wires config, store, kernel, IPC and the snapshot pump together.

use std::os::unix::net::UnixStream as StdUnixStream;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use futures::FutureExt;
use futures::future::BoxFuture;
use navori_config::{
    Config, ConfigLoad, ConfigReport, JournalSweep, Paths, Profile, WholeFileReason,
};
use navori_ipc::Handler;
use navori_kernel::{DispatchError, Kernel, ModuleState};
use navori_protocol::{Check, CoreStatus, Envelope, Event, ModuleInfo, Severity, rpc};
use navori_store::Store;
use serde_json::{Value, json};
use tokio::sync::broadcast::error::RecvError;
use tokio_util::sync::CancellationToken;

/// Everything the running core owns. Shared by the FFI object, the IPC handler and the pump.
#[derive(Debug)]
pub struct Inner {
    pub kernel: Kernel,
    pub store: Store,
    pub paths: Paths,
    pub profile: Profile,
    pub version: String,
    pub pid: u32,
    pub config_notice: Option<crate::ConfigNotice>,
    /// Set once, right after the single-instance check passes.
    pub last_unclean_exit: std::sync::OnceLock<Option<String>>,
}

impl Inner {
    pub fn status(&self) -> CoreStatus {
        CoreStatus {
            version: self.version.clone(),
            profile: self.profile.as_str().to_owned(),
            pid: self.pid,
            started_at_ms: self.kernel.started_at_ms(),
            uptime_ms: u64::try_from(self.kernel.uptime().as_millis()).unwrap_or(u64::MAX),
            data_dir: self.paths.data_dir.display().to_string(),
            socket_path: self.paths.socket.display().to_string(),
            modules: self.kernel.modules(),
        }
    }

    pub fn recent_events(&self, limit: usize) -> Vec<Event> {
        self.kernel.recent_events(limit).unwrap_or_else(|e| {
            tracing::warn!(error = %e, "cannot read journal");
            Vec::new()
        })
    }

    /// Health checks. Synchronous and fast (no network); safe to call from any thread.
    pub fn doctor(&self) -> Vec<Check> {
        let mut checks = Vec::new();

        checks.push(match &self.config_notice {
            None => check(
                "config",
                true,
                format!("{} is valid", self.paths.config_file.display()),
            ),
            Some(notice) => check(
                "config",
                false,
                format!("{}. {}", notice.title, notice.message),
            ),
        });

        checks.push(match self.store.integrity_ok() {
            Ok(true) => check(
                "database",
                true,
                format!(
                    "{} ok, core schema v{}",
                    self.paths.db_file.display(),
                    self.store.schema_version("core").unwrap_or(0)
                ),
            ),
            Ok(false) => check("database", false, "integrity check failed".to_owned()),
            Err(e) => check("database", false, e.to_string()),
        });

        checks.push(match StdUnixStream::connect(&self.paths.socket) {
            Ok(_) => check(
                "ipc",
                true,
                format!("listening on {}", self.paths.socket.display()),
            ),
            Err(e) => check(
                "ipc",
                false,
                format!("{}: {e}", self.paths.socket.display()),
            ),
        });

        let unhealthy: Vec<String> = self
            .kernel
            .modules()
            .into_iter()
            .filter(|m| m.state != ModuleState::Running)
            .map(|m| format!("{} is {}", m.id, state_name(&m.state)))
            .collect();
        checks.push(if unhealthy.is_empty() {
            check(
                "modules",
                true,
                format!("{} running", self.kernel.modules().len()),
            )
        } else {
            check("modules", false, unhealthy.join(", "))
        });

        let probe = self.paths.logs_dir.join(".write-probe");
        checks.push(
            match std::fs::write(&probe, b"").and_then(|()| std::fs::remove_file(&probe)) {
                Ok(()) => check("logs", true, self.paths.logs_dir.display().to_string()),
                Err(e) => check(
                    "logs",
                    false,
                    format!("{}: {e}", self.paths.logs_dir.display()),
                ),
            },
        );

        checks
    }
}

fn check(name: &str, ok: bool, detail: String) -> Check {
    Check {
        name: name.to_owned(),
        ok,
        detail,
    }
}

pub fn state_name(state: &ModuleState) -> &'static str {
    match state {
        ModuleState::Starting => "starting",
        ModuleState::Running => "running",
        ModuleState::Restarting { .. } => "restarting",
        ModuleState::Failed { .. } => "failed",
        ModuleState::Stopped => "stopped",
    }
}

pub fn state_detail(info: &ModuleInfo) -> Option<String> {
    match &info.state {
        ModuleState::Restarting { last_error, .. } | ModuleState::Failed { last_error } => {
            Some(last_error.clone())
        }
        _ => None,
    }
}

/// The load is infallible: a bad value falls back alone, and a bad journal or a whole-file fallback pauses the sweep.
pub fn load_config(paths: &Paths) -> ConfigLoad {
    Config::load(&paths.config_file)
}

pub fn config_notice(
    report: &ConfigReport,
    sweep: &JournalSweep,
    module_issues: &[&str],
    file: &Path,
) -> Option<crate::ConfigNotice> {
    let name = file_label(file);
    let mut notice = match report {
        ConfigReport::WholeFile(reason) => Some(crate::ConfigNotice {
            title: format!("{name} is on defaults"),
            message: whole_file_message(reason),
        }),
        ConfigReport::Clean | ConfigReport::BadValues(_) => {
            let keys = bad_keys(report, module_issues);
            if keys.is_empty() {
                None
            } else {
                Some(crate::ConfigNotice {
                    title: format!("{name} has bad values"),
                    message: format!(
                        "{} use their defaults. Fix them and reopen Navori.",
                        keys.join(", ")
                    ),
                })
            }
        }
    };
    if let Some(notice) = notice.as_mut()
        && matches!(sweep, JournalSweep::Paused)
    {
        notice.message.push(' ');
        notice
            .message
            .push_str("Journal cleanup is paused until the file is fixed and Navori is reopened.");
    }
    notice
}

fn file_label(file: &Path) -> std::borrow::Cow<'_, str> {
    match file.file_name() {
        Some(name) => name.to_string_lossy(),
        None => file.to_string_lossy(),
    }
}

fn whole_file_message(reason: &WholeFileReason) -> String {
    match reason {
        WholeFileReason::Unreadable => "It cannot be read.".to_owned(),
        WholeFileReason::NotToml {
            position: Some((line, column)),
        } => format!("Not valid TOML at line {line}, column {column}."),
        WholeFileReason::NotToml { position: None } => "Not valid TOML.".to_owned(),
    }
}

fn bad_keys(report: &ConfigReport, module_issues: &[&str]) -> Vec<String> {
    let mut keys = match report {
        ConfigReport::BadValues(keys) => keys.clone(),
        ConfigReport::Clean | ConfigReport::WholeFile(_) => Vec::new(),
    };
    for issue in module_issues {
        if !keys.iter().any(|key| key == issue) {
            keys.push((*issue).to_owned());
        }
    }
    keys
}

pub fn build_kernel(
    store: Store,
    sections: serde_json::Map<String, serde_json::Value>,
    sweep: &JournalSweep,
) -> Kernel {
    let builder = match sweep {
        JournalSweep::KeepDays(days) => Kernel::builder(store).journal_retention_days(*days),
        JournalSweep::Paused => Kernel::builder(store).pause_journal_sweep(),
    };
    builder.config_sections(sections).start()
}

/// JSON-RPC handler for the local socket.
#[derive(Debug)]
pub struct IpcBridge(pub Arc<Inner>);

impl Handler for IpcBridge {
    fn handle<'a>(
        &'a self,
        method: &'a str,
        params: Value,
    ) -> BoxFuture<'a, Result<Value, rpc::Error>> {
        async move {
            let inner = &self.0;
            match method {
                rpc::method::STATUS => to_json(&inner.status()),
                rpc::method::ACTIONS => to_json(&inner.kernel.actions()),
                rpc::method::DOCTOR => to_json(&inner.doctor()),
                rpc::method::EVENTS => {
                    let limit = params
                        .get("limit")
                        .and_then(Value::as_u64)
                        .unwrap_or(50)
                        .min(1000);
                    to_json(&inner.recent_events(usize::try_from(limit).unwrap_or(1000)))
                }
                rpc::method::DISPATCH => {
                    let envelope: Envelope = serde_json::from_value(params)
                        .map_err(|e| rpc_err(rpc::code::INVALID_PARAMS, e))?;
                    inner
                        .kernel
                        .dispatch(envelope)
                        .await
                        .map_err(dispatch_to_rpc)
                }
                other => Err(rpc_err(
                    rpc::code::METHOD_NOT_FOUND,
                    format!("unknown method {other}"),
                )),
            }
        }
        .boxed()
    }
}

fn to_json<T: serde::Serialize>(v: &T) -> Result<Value, rpc::Error> {
    serde_json::to_value(v).map_err(|e| rpc_err(rpc::code::INTERNAL, e))
}

fn rpc_err(code: i64, e: impl std::fmt::Display) -> rpc::Error {
    rpc::Error {
        code,
        message: e.to_string(),
        data: None,
    }
}

pub fn dispatch_to_rpc(e: DispatchError) -> rpc::Error {
    let code = match &e {
        DispatchError::UnknownModule(_) => rpc::code::UNKNOWN_MODULE,
        DispatchError::InvalidAction { .. } => rpc::code::INVALID_PARAMS,
        DispatchError::Unavailable(_) | DispatchError::Timeout(_) => rpc::code::MODULE_UNAVAILABLE,
        DispatchError::Action(_) => rpc::code::ACTION_FAILED,
    };
    rpc_err(code, e)
}

/// Emits the lifecycle events of a fresh start.
pub fn announce_start(inner: &Inner) {
    let bus = inner.kernel.bus();
    bus.emit(
        "core",
        "core.started",
        Severity::Info,
        format!(
            "Navori {} started ({})",
            inner.version,
            inner.profile.as_str()
        ),
        json!({ "pid": inner.pid }),
    );
    if let Some(Some(prev)) = inner.last_unclean_exit.get() {
        bus.emit(
            "core",
            "core.unclean_exit",
            Severity::Warn,
            "The previous Navori session ended unexpectedly".to_owned(),
            json!({ "marker": prev }),
        );
    }
}

/// Pushes snapshots to `notify` whenever module state changes or an event arrives,
/// coalesced to ~30/s while the panel is visible and 4/s otherwise.
pub async fn snapshot_pump(
    inner: Arc<Inner>,
    cancel: CancellationToken,
    notify: impl Fn() + Send + 'static,
) {
    let mut revision = inner.kernel.revision();
    let mut events = inner.kernel.bus().subscribe();
    notify();
    loop {
        tokio::select! {
            () = cancel.cancelled() => return,
            changed = revision.changed() => if changed.is_err() { return },
            ev = events.recv() => if matches!(ev, Err(RecvError::Closed)) { return },
        }
        let delay = if inner.kernel.scheduler().panel_visible() {
            Duration::from_millis(33)
        } else {
            Duration::from_millis(250)
        };
        tokio::select! {
            () = cancel.cancelled() => return,
            () = tokio::time::sleep(delay) => {}
        }
        revision.borrow_and_update();
        while events.try_recv().is_ok() {}
        notify();
    }
}

//! File logging: `navori.YYYY-MM-DD.log` in the profile logs dir, daily rotation, 14 files.

use std::path::Path;
use std::sync::OnceLock;

use tracing_appender::non_blocking::WorkerGuard;
use tracing_appender::rolling::{Builder, Rotation};
use tracing_subscriber::EnvFilter;

static GUARD: OnceLock<WorkerGuard> = OnceLock::new();

/// Installs the global subscriber once per process. `NAVORI_LOG` overrides `level`.
pub fn init(logs_dir: &Path, level: &str) -> Result<(), String> {
    if GUARD.get().is_some() {
        return Ok(());
    }
    let appender = Builder::new()
        .rotation(Rotation::DAILY)
        .filename_prefix("navori")
        .filename_suffix("log")
        .max_log_files(14)
        .build(logs_dir)
        .map_err(|e| format!("cannot open log directory {}: {e}", logs_dir.display()))?;
    let (writer, guard) = tracing_appender::non_blocking(appender);
    let filter = EnvFilter::try_from_env("NAVORI_LOG").unwrap_or_else(|_| EnvFilter::new(level));
    tracing_subscriber::fmt()
        .with_writer(writer)
        .with_ansi(false)
        .with_env_filter(filter)
        .try_init()
        .map_err(|e| format!("cannot install logger: {e}"))?;
    let _ = GUARD.set(guard);
    Ok(())
}

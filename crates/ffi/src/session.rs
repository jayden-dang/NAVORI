//! Unclean-exit detection.
//!
//! A marker file exists while the core runs and is removed on clean shutdown. If it is
//! still there at the next start, the previous session crashed or was killed; its content
//! (plus any panic messages appended by the panic hook) is reported once.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, Once, PoisonError};

const MARKER: &str = "session.marker";

static ACTIVE: Mutex<Option<PathBuf>> = Mutex::new(None);
static HOOK: Once = Once::new();

/// Returns the previous session's marker content (if it ended uncleanly) and writes a new one.
pub fn begin(data_dir: &Path, pid: u32, started_at_ms: i64) -> std::io::Result<Option<String>> {
    let path = data_dir.join(MARKER);
    let previous = match std::fs::read_to_string(&path) {
        Ok(s) => Some(s.trim().to_owned()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(e),
    };
    std::fs::write(&path, format!("pid {pid} started {started_at_ms}\n"))?;
    *ACTIVE.lock().unwrap_or_else(PoisonError::into_inner) = Some(path);
    install_panic_hook();
    Ok(previous)
}

pub fn end(data_dir: &Path) {
    let path = data_dir.join(MARKER);
    let mut active = ACTIVE.lock().unwrap_or_else(PoisonError::into_inner);
    if active.as_deref() == Some(path.as_path()) {
        *active = None;
    }
    if let Err(e) = std::fs::remove_file(&path)
        && e.kind() != std::io::ErrorKind::NotFound
    {
        tracing::warn!(error = %e, "cannot remove session marker");
    }
}

fn install_panic_hook() {
    HOOK.call_once(|| {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            let path = ACTIVE
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .clone();
            if let Some(path) = path
                && let Ok(mut f) = std::fs::OpenOptions::new().append(true).open(path)
            {
                let line = navori_redact::redact_text(&format!("panic: {info}")).text;
                let _ = writeln!(f, "{line}");
            }
            previous(info);
        }));
    });
}

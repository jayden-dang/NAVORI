use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::IntegrationsEnv;
use crate::action::Harness;

#[derive(Debug)]
pub(crate) enum Read {
    NoDir,
    NoFile,
    Doc(Value),
    Unreadable(String),
}

fn config_dir(home: &Path, harness: Harness) -> PathBuf {
    home.join(match harness {
        Harness::Claude => ".claude",
        Harness::Codex => ".codex",
        Harness::Grok => ".grok",
    })
}

pub(crate) fn hook_file(home: &Path, harness: Harness) -> PathBuf {
    let dir = config_dir(home, harness);
    match harness {
        Harness::Claude => dir.join("settings.json"),
        Harness::Codex => dir.join("hooks.json"),
        Harness::Grok => dir.join("hooks").join("navori.json"),
    }
}

pub(crate) fn read(env: &IntegrationsEnv, harness: Harness) -> Read {
    if !config_dir(&env.home, harness).is_dir() {
        return Read::NoDir;
    }
    let path = hook_file(&env.home, harness);
    let meta = match std::fs::symlink_metadata(&path) {
        Ok(meta) => meta,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Read::NoFile,
        Err(e) => return Read::Unreadable(format!("cannot look at the file ({e})")),
    };
    let resolved = if meta.file_type().is_symlink() {
        match std::fs::canonicalize(&path) {
            Ok(resolved) => resolved,
            Err(_) => return Read::Unreadable("the file is a link that leads nowhere".to_owned()),
        }
    } else {
        path
    };
    let bytes = match std::fs::read(&resolved) {
        Ok(bytes) => bytes,
        Err(e) => return Read::Unreadable(format!("cannot read the file ({e})")),
    };
    match serde_json::from_slice::<Value>(&bytes) {
        Ok(doc) => Read::Doc(doc),
        Err(e) => Read::Unreadable(format!("it is not valid JSON ({e})")),
    }
}

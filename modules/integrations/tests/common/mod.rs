#![allow(dead_code, clippy::unwrap_used)]

use std::path::{Path, PathBuf};

use navori_kernel::{DispatchError, Envelope, Kernel};
use navori_mod_integrations::{Integrations, IntegrationsEnv};
use navori_store::Store;
use serde_json::{Value, json};
use tempfile::TempDir;

pub const RELEASE_HELPER: &str = "/Applications/Navori.app/Contents/Helpers/navori";
pub const DEV_HELPER: &str = "/Applications/Navori Dev.app/Contents/Helpers/navori";

pub struct Rig {
    pub home: TempDir,
    pub kernel: Kernel,
    pub env: IntegrationsEnv,
}

impl Rig {
    pub fn release() -> Self {
        Self::new(false, RELEASE_HELPER)
    }

    pub fn dev() -> Self {
        Self::new(true, DEV_HELPER)
    }

    fn new(dev: bool, helper: &str) -> Self {
        let home = tempfile::tempdir().unwrap();
        let env = IntegrationsEnv {
            home: home.path().to_path_buf(),
            dev,
            helper: PathBuf::from(helper),
        };
        let kernel = Kernel::builder(Store::open_in_memory().unwrap())
            .pause_journal_sweep()
            .register(Integrations::new(env.clone()))
            .start();
        Self { home, kernel, env }
    }

    pub fn path(&self, relative: &str) -> PathBuf {
        self.home.path().join(relative)
    }

    pub fn make_dir(&self, relative: &str) {
        std::fs::create_dir_all(self.path(relative)).unwrap();
    }

    pub fn write(&self, relative: &str, body: &str) {
        let path = self.path(relative);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, body).unwrap();
    }

    pub fn write_json(&self, relative: &str, doc: &Value) {
        self.write(relative, &serde_json::to_string_pretty(doc).unwrap());
    }

    pub async fn status(&self) -> Value {
        self.dispatch(json!("status")).await.unwrap()
    }

    pub async fn dispatch(&self, action: Value) -> Result<Value, DispatchError> {
        self.kernel
            .dispatch(Envelope {
                module: "integrations".into(),
                action,
            })
            .await
    }

    pub async fn row(&self, harness: &str) -> Value {
        let state = self.status().await;
        state["harnesses"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["harness"] == harness)
            .unwrap()
            .clone()
    }
}

pub fn command(helper: &str, dev: bool) -> String {
    let quoted = helper.replace('\'', "'\\''");
    if dev {
        format!("'{quoted}' --profile dev hook")
    } else {
        format!("'{quoted}' hook")
    }
}

pub fn handler(command: &str, timeout: u32) -> Value {
    json!({ "type": "command", "command": command, "timeout": timeout })
}

pub fn group(handlers: Vec<Value>) -> Value {
    json!({ "hooks": handlers })
}

/// The three events Claude Code and Codex use, written as Navori would.
pub fn complete(command: &str, events: &[&str]) -> Value {
    let mut hooks = serde_json::Map::new();
    for event in events {
        let timeout = if *event == "SessionEnd" { 1 } else { 2 };
        hooks.insert(
            (*event).to_owned(),
            json!([group(vec![handler(command, timeout)])]),
        );
    }
    json!({ "hooks": hooks })
}

pub const THREE: [&str; 3] = ["UserPromptSubmit", "Stop", "SessionEnd"];
pub const FIVE: [&str; 5] = [
    "UserPromptSubmit",
    "Stop",
    "StopCancelled",
    "StopFailure",
    "SessionEnd",
];

pub fn tree(dir: &Path) -> Vec<String> {
    let mut out = Vec::new();
    walk(dir, dir, &mut out);
    out.sort();
    out
}

fn walk(root: &Path, dir: &Path, out: &mut Vec<String>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        let meta = std::fs::symlink_metadata(&path).unwrap();
        let rel = path.strip_prefix(root).unwrap().display().to_string();
        if meta.is_dir() {
            out.push(format!("{rel}/"));
            walk(root, &path, out);
        } else {
            out.push(format!(
                "{rel} {} {:?}",
                meta.len(),
                meta.modified().unwrap()
            ));
        }
    }
}

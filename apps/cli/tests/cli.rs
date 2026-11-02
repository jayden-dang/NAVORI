//! The real `navori` binary against a real core running in this test process.
#![allow(clippy::unwrap_used)]

use std::os::unix::fs::MetadataExt;
use std::sync::Arc;

use assert_cmd::Command;
use navori_ffi::{CoreConfig, CoreSnapshot, NavoriCore, SnapshotListener};
use predicates::prelude::*;
use serde_json::Value;

struct Ignore;
impl SnapshotListener for Ignore {
    fn on_snapshot(&self, _: CoreSnapshot) {}
}

struct Running {
    home: tempfile::TempDir,
    core: Arc<NavoriCore>,
}

impl Drop for Running {
    fn drop(&mut self) {
        self.core.shutdown();
    }
}

fn short_home() -> tempfile::TempDir {
    tempfile::Builder::new()
        .prefix("nv")
        .tempdir_in("/tmp")
        .unwrap()
}

fn running_core() -> Running {
    start_core(None)
}

fn running_core_with_config(body: &str) -> Running {
    start_core(Some(body))
}

fn start_core(config_toml: Option<&str>) -> Running {
    let home = short_home();
    if let Some(body) = config_toml {
        let data = home.path().join("Library/Application Support/Navori Dev");
        std::fs::create_dir_all(&data).unwrap();
        std::fs::write(data.join("config.toml"), body).unwrap();
    }
    let core = NavoriCore::start(
        CoreConfig {
            profile: "dev".into(),
            home_dir: home.path().to_string_lossy().into_owned(),
            uid: std::fs::metadata(home.path()).unwrap().uid(),
            app_version: "9.9.9-test".into(),
            log_to_file: false,
        },
        Arc::new(Ignore),
    )
    .unwrap();
    Running { home, core }
}

fn navori(home: &tempfile::TempDir) -> Command {
    let mut cmd = Command::cargo_bin("navori").unwrap();
    cmd.env("HOME", home.path())
        .env("NAVORI_PROFILE", "dev")
        .env_remove("NAVORI_LOG");
    cmd
}

#[test]
fn status_json_describes_the_running_core() {
    let r = running_core();
    let out = navori(&r.home)
        .args(["status", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let v: Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(v["pid"], std::process::id());
    assert_eq!(v["version"], "9.9.9-test");
    assert_eq!(v["profile"], "dev");
}

#[test]
fn status_plain_is_human_readable() {
    let r = running_core();
    navori(&r.home)
        .arg("status")
        .assert()
        .success()
        .stdout(predicate::str::contains("Navori 9.9.9-test (dev)"))
        .stdout(predicate::str::contains("system"));
}

#[test]
fn dispatch_prints_the_module_answer_as_json() {
    let r = running_core();
    navori(&r.home)
        .args(["dispatch", "system", "\"ping\""])
        .assert()
        .success()
        .stdout("\"pong\"\n");
}

#[test]
fn dispatch_errors_exit_nonzero_with_a_message() {
    let r = running_core();
    navori(&r.home)
        .args(["dispatch", "ghost", "\"ping\""])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("ghost"));
    navori(&r.home)
        .args(["dispatch", "system", "{broken"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("JSON"));
}

#[test]
fn actions_lists_every_action_with_its_risk() {
    let r = running_core();
    navori(&r.home)
        .arg("actions")
        .assert()
        .success()
        .stdout(predicate::str::contains("system.ping"))
        .stdout(predicate::str::contains("read"));
}

#[test]
fn events_show_the_journal_newest_first() {
    let r = running_core();
    let out = navori(&r.home)
        .args(["events", "--json", "--limit", "5"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let v: Vec<Value> = serde_json::from_slice(&out).unwrap();
    assert!(v.len() <= 5);
    assert!(v.iter().any(|e| e["kind"] == "core.started"), "{v:?}");
}

#[test]
fn doctor_fails_on_a_bad_config_value() {
    let r = running_core_with_config("log_level = \"loud\"\n");
    navori(&r.home)
        .arg("doctor")
        .assert()
        .code(1)
        .stdout(predicate::str::contains(
            "config.toml has bad values. log_level use their defaults. Fix them and reopen Navori.",
        ))
        .stdout(predicate::str::contains("✗").and(predicate::str::contains("config")));
}

#[test]
fn doctor_passes_on_a_healthy_core() {
    let r = running_core();
    navori(&r.home)
        .arg("doctor")
        .assert()
        .success()
        .stdout(predicate::str::contains("✓ app"))
        .stdout(predicate::str::contains("✓ database"));
}

#[test]
fn every_command_reports_when_the_app_is_not_running() {
    let home = short_home();
    navori(&home)
        .arg("status")
        .assert()
        .code(3)
        .stderr(predicate::str::contains("not running"));
    navori(&home)
        .arg("doctor")
        .assert()
        .code(3)
        .stdout(predicate::str::contains("✗ app"));
}

#[test]
fn an_unknown_profile_is_a_usage_error() {
    let home = short_home();
    navori(&home)
        .args(["--profile", "prod", "status"])
        .assert()
        .code(2);
}

//! The real `navori` binary against a real core running in this test process.
#![allow(clippy::unwrap_used)]

use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::sync::Arc;
use std::time::{Duration, Instant};

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

fn navori_process(home: &tempfile::TempDir) -> std::process::Command {
    let mut cmd = std::process::Command::new(assert_cmd::cargo::cargo_bin("navori"));
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
fn status_json_carries_the_icon_state() {
    let r = running_core();
    let out = navori(&r.home)
        .args(["status", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let v: Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(v["tray"]["glyph"], "normal");
    assert_eq!(v["tray"]["attention"], false);
    assert_eq!(v["tray"]["summary"], "Navori");
    assert_eq!(v["pid"], std::process::id());
    assert_eq!(v["version"], "9.9.9-test");
    assert_eq!(v["profile"], "dev");
    assert!(v["data_dir"].is_string());
    assert!(v["socket_path"].is_string());
    assert!(v["modules"].is_array());
}

#[test]
fn status_plain_output_has_no_icon_line() {
    let r = running_core();
    navori(&r.home)
        .arg("status")
        .assert()
        .success()
        .stdout(predicate::str::contains("glyph").not())
        .stdout(predicate::str::contains("attention").not());
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

#[test]
fn awake_hold_for_an_hour_shows_in_status() {
    let r = running_core();
    navori(&r.home)
        .args(["awake", "hold", "--for", "1h"])
        .assert()
        .success();
    navori(&r.home)
        .args(["awake", "status"])
        .assert()
        .success()
        .stdout(predicate::str::contains("1 Lease"))
        .stdout(predicate::str::contains("1 hour"));
}

#[test]
fn awake_hold_with_no_flag_is_until_off() {
    let r = running_core();
    navori(&r.home).args(["awake", "hold"]).assert().success();
    navori(&r.home)
        .args(["awake", "status"])
        .assert()
        .success()
        .stdout(predicate::str::contains("1 Lease"))
        .stdout(predicate::str::contains("Until off"));
}

#[test]
fn awake_release_all_ends_the_lease() {
    let r = running_core();
    navori(&r.home)
        .args(["awake", "hold", "--for", "1h"])
        .assert()
        .success();
    navori(&r.home)
        .args(["awake", "release-all"])
        .assert()
        .success();
    navori(&r.home)
        .args(["awake", "status"])
        .assert()
        .success()
        .stdout(predicate::str::contains("0 Leases"))
        .stdout(predicate::str::contains("1 hour").not());
}

#[test]
fn a_bad_duration_is_a_usage_error() {
    let home = short_home();
    navori(&home)
        .args(["awake", "hold", "--for", "nope"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("duration"));
}

#[test]
fn awake_status_with_the_app_down_exits_3() {
    let home = short_home();
    navori(&home)
        .args(["awake", "status"])
        .assert()
        .code(3)
        .stderr(predicate::str::contains("not running"));
}

#[test]
fn awake_run_holds_while_the_command_runs() {
    let r = running_core();
    let mut child = navori_process(&r.home)
        .args(["awake", "run", "--", "sleep", "5"])
        .spawn()
        .unwrap();

    let deadline = Instant::now() + Duration::from_secs(3);
    let mut status_out = String::new();
    while Instant::now() < deadline {
        let output = navori(&r.home).args(["awake", "status"]).output().unwrap();
        status_out = String::from_utf8_lossy(&output.stdout).into_owned();
        if status_out.contains("1 Lease") && status_out.contains("sleep") {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(
        status_out.contains("1 Lease") && status_out.contains("sleep"),
        "status while sleep runs: {status_out}"
    );

    let finished = child.wait().unwrap();
    assert!(finished.success(), "{finished:?}");
}

#[test]
fn awake_run_exits_with_the_commands_code() {
    let r = running_core();
    for code in [0, 3, 7] {
        navori(&r.home)
            .args(["awake", "run", "--", "sh", "-c", &format!("exit {code}")])
            .assert()
            .code(code);
    }
}

#[test]
fn awake_run_with_the_app_down_still_runs_and_warns_once_on_stderr() {
    let home = short_home();
    let marker = home.path().join("ran");
    navori(&home)
        .args(["awake", "run", "--", "/usr/bin/touch"])
        .arg(&marker)
        .assert()
        .code(0)
        .stderr(predicate::eq(
            "navori: no Lease held because Navori is not running\n",
        ));
    assert!(marker.exists(), "command should still have run");
}

#[test]
fn awake_run_with_a_missing_command_exits_127() {
    let home = short_home();
    navori(&home)
        .args(["awake", "run", "--", "navori-awake-run-missing-command"])
        .assert()
        .code(127);
}

#[test]
fn awake_run_on_a_non_executable_file_exits_126() {
    let home = short_home();
    let path = home.path().join("not-executable");
    std::fs::write(&path, b"#!/bin/sh\nexit 0\n").unwrap();
    let mut perms = std::fs::metadata(&path).unwrap().permissions();
    perms.set_mode(0o644);
    std::fs::set_permissions(&path, perms).unwrap();
    navori(&home)
        .args(["awake", "run", "--"])
        .arg(&path)
        .assert()
        .code(126);
}

#[test]
fn awake_run_exits_128_plus_the_signal() {
    let home = short_home();
    navori(&home)
        .args(["awake", "run", "--", "sh", "-c", "kill -s TERM $$"])
        .assert()
        .code(143);
}

#[test]
fn awake_run_with_the_app_down_creates_no_socket() {
    let home = short_home();
    navori(&home)
        .args(["awake", "run", "--", "/usr/bin/true"])
        .assert()
        .code(0);
    let socket = home
        .path()
        .join("Library/Application Support/Navori Dev/run/navori.sock");
    assert!(!socket.exists(), "CLI must not create {}", socket.display());
}

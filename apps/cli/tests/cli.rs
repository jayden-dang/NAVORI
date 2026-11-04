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
            helper_path: home
                .path()
                .join("Navori Dev.app/Contents/Helpers/navori")
                .to_string_lossy()
                .into_owned(),
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
fn doctor_prints_the_integrations_row_and_exits_1_when_it_fails() {
    let r = running_core();
    let hooks = r.home.path().join(".grok/hooks");
    std::fs::create_dir_all(&hooks).unwrap();
    std::fs::write(hooks.join("navori.json"), "{ \"hooks\": ").unwrap();
    navori(&r.home)
        .arg("doctor")
        .assert()
        .code(1)
        .stdout(predicate::str::contains("✗ integrations"))
        .stdout(predicate::str::contains("grok: unreadable"));
}

#[test]
fn doctor_with_the_app_down_still_prints_only_the_app_row_and_exits_3() {
    let home = short_home();
    std::fs::create_dir_all(home.path().join(".claude")).unwrap();
    navori(&home)
        .arg("doctor")
        .assert()
        .code(3)
        .stdout(predicate::str::contains("✗ app"))
        .stdout(predicate::str::contains("integrations").not());
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
fn awake_status_states_tracking_agents() {
    let r = running_core();
    navori(&r.home)
        .args(["awake", "status"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Tracking agents: on"));
    navori(&r.home)
        .args(["dispatch", "awake", r#"{"tracking":{"on":false}}"#])
        .assert()
        .success();
    navori(&r.home)
        .args(["awake", "status"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Tracking agents: off"));
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

struct Recorder {
    home: tempfile::TempDir,
    requests: Arc<std::sync::Mutex<Vec<Value>>>,
}

#[derive(Clone, Copy)]
enum Reply {
    Result,
    Invalid,
    Never,
}

fn recording_server() -> Recorder {
    let home = short_home();
    let requests = serve(home.path(), navori_config::Profile::Dev, Reply::Result);
    Recorder { home, requests }
}

fn recording_server_replying(reply: Reply) -> Recorder {
    let home = short_home();
    let requests = serve(home.path(), navori_config::Profile::Dev, reply);
    Recorder { home, requests }
}

fn serve(
    home: &std::path::Path,
    profile: navori_config::Profile,
    reply: Reply,
) -> Arc<std::sync::Mutex<Vec<Value>>> {
    use std::io::{BufRead, BufReader, Write};

    let uid = std::fs::metadata(home).unwrap().uid();
    let socket = navori_config::Paths::new(profile, home, uid).socket;
    std::fs::create_dir_all(socket.parent().unwrap()).unwrap();
    let listener = std::os::unix::net::UnixListener::bind(&socket).unwrap();
    let requests = Arc::new(std::sync::Mutex::new(Vec::new()));
    let seen = Arc::clone(&requests);
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(stream) = stream else { break };
            let seen = Arc::clone(&seen);
            std::thread::spawn(move || {
                let mut writer = stream.try_clone().unwrap();
                for line in BufReader::new(stream).lines() {
                    let Ok(line) = line else { break };
                    let request: Value = serde_json::from_str(&line).unwrap();
                    let answer = match reply {
                        Reply::Result => serde_json::json!({
                            "jsonrpc": "2.0",
                            "id": request["id"],
                            "result": {},
                        }),
                        Reply::Invalid => serde_json::json!({
                            "jsonrpc": "2.0",
                            "id": request["id"],
                            "error": { "code": -32000, "message": "invalid" },
                        }),
                        Reply::Never => {
                            seen.lock().unwrap().push(request);
                            std::thread::sleep(Duration::from_secs(30));
                            break;
                        }
                    };
                    seen.lock().unwrap().push(request);
                    if writeln!(writer, "{answer}").is_err() {
                        break;
                    }
                }
            });
        }
    });
    requests
}

impl Recorder {
    fn hook(&self, payload: &str) -> Vec<Value> {
        navori(&self.home)
            .args(["--profile", "dev", "hook"])
            .write_stdin(payload.to_owned())
            .assert()
            .success()
            .stdout("")
            .stderr("");
        self.requests.lock().unwrap().clone()
    }
}

fn dispatched_action(request: &Value) -> &Value {
    assert_eq!(request["method"], "core.dispatch");
    assert_eq!(request["params"]["module"], "awake");
    &request["params"]["action"]
}

#[test]
fn a_prompt_submit_sends_hold_session_with_its_own_pids() {
    let server = recording_server();

    let requests = server.hook(r#"{"hook_event_name":"UserPromptSubmit","session_id":"s-1"}"#);

    assert_eq!(requests.len(), 1);
    let hold = &dispatched_action(&requests[0])["hold_session"];
    assert_eq!(hold["session_id"], "s-1");
    assert!(hold["pid"].as_i64().unwrap() > 0);
    assert_ne!(hold["pid"].as_i64().unwrap(), i64::from(std::process::id()));
    assert_eq!(
        hold["ppid"].as_i64().unwrap(),
        i64::from(std::process::id())
    );
}

#[test]
fn a_subagent_start_sends_its_ids_and_own_pids() {
    let server = recording_server();

    let requests = server.hook(
        r#"{"hook_event_name":"SubagentStart","session_id":"s-1","agent_id":"a-1","agent_type":"secret-type","cwd":"/secret"}"#,
    );

    assert_eq!(requests.len(), 1);
    let start = &dispatched_action(&requests[0])["subagent_start"];
    assert_eq!(start["session_id"], "s-1");
    assert_eq!(start["agent_id"], "a-1");
    assert!(start["pid"].as_i64().unwrap() > 0);
    assert_eq!(
        start["ppid"].as_i64().unwrap(),
        i64::from(std::process::id())
    );
    assert!(!requests[0].to_string().contains("secret"));
}

#[test]
fn a_subagent_stop_sends_only_its_ids() {
    let server = recording_server();

    let requests = server.hook(
        r#"{"hook_event_name":"SubagentStop","session_id":"s-1","agent_id":"a-1","last_assistant_message":"secret"}"#,
    );

    assert_eq!(requests.len(), 1);
    assert_eq!(
        dispatched_action(&requests[0]),
        &serde_json::json!({"subagent_stop": {"session_id": "s-1", "agent_id": "a-1"}})
    );
}

#[test]
fn a_stop_with_background_tasks_says_so_and_sends_none_of_them() {
    let server = recording_server();

    let requests = server.hook(
        r#"{"hook_event_name":"Stop","session_id":"s-1","background_tasks":[{"id":"a1","type":"subagent","status":"running","description":"secret"}]}"#,
    );

    assert_eq!(
        dispatched_action(&requests[0]),
        &serde_json::json!({"stop_session": {"session_id": "s-1", "background": true}})
    );
    assert!(!requests[0].to_string().contains("secret"));
}

#[test]
fn a_stop_with_an_empty_background_list_reads_as_none() {
    let server = recording_server();

    let requests =
        server.hook(r#"{"hook_event_name":"Stop","session_id":"s-1","background_tasks":[]}"#);

    assert_eq!(
        dispatched_action(&requests[0]),
        &serde_json::json!({"stop_session": {"session_id": "s-1"}})
    );
}

#[test]
fn a_background_list_of_another_shape_does_not_lose_the_event() {
    let server = recording_server();

    let requests =
        server.hook(r#"{"hook_event_name":"Stop","session_id":"s-1","background_tasks":"odd"}"#);

    assert_eq!(
        dispatched_action(&requests[0]),
        &serde_json::json!({"stop_session": {"session_id": "s-1"}})
    );
}

#[test]
fn a_subagent_stop_with_background_tasks_says_so() {
    let server = recording_server();

    let requests = server.hook(
        r#"{"hook_event_name":"SubagentStop","session_id":"s-1","agent_id":"a-1","background_tasks":[{"id":"a-1"}]}"#,
    );

    assert_eq!(
        dispatched_action(&requests[0]),
        &serde_json::json!({"subagent_stop": {"session_id": "s-1", "agent_id": "a-1", "background": true}})
    );
}

fn transcript(lines: &[String]) -> (tempfile::TempDir, String) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("session.jsonl");
    std::fs::write(&path, lines.join("\n") + "\n").unwrap();
    let text = path.to_str().unwrap().to_owned();
    (dir, text)
}

fn ai_title(title: &str) -> String {
    serde_json::json!({"type": "ai-title", "aiTitle": title, "sessionId": "s-1"}).to_string()
}

fn custom_title(title: &str) -> String {
    serde_json::json!({"type": "custom-title", "customTitle": title, "sessionId": "s-1"})
        .to_string()
}

fn hook_payload(event: &str, transcript: &str, extra: &str) -> String {
    format!(
        r#"{{"hook_event_name":"{event}","session_id":"s-1","transcript_path":{}{extra}}}"#,
        serde_json::json!(transcript)
    )
}

#[test]
fn a_named_session_sends_its_title_field_first() {
    let server = recording_server();
    let (_dir, path) = transcript(&[custom_title("older name"), ai_title("ai title")]);

    let requests = server.hook(&hook_payload(
        "UserPromptSubmit",
        &path,
        r#","session_title":"My name""#,
    ));

    assert_eq!(
        dispatched_action(&requests[0])["hold_session"]["title"],
        "My name"
    );
}

#[test]
fn with_no_title_field_a_custom_title_beats_a_later_ai_title() {
    let server = recording_server();
    let (_dir, path) = transcript(&[custom_title("My name"), ai_title("ai title")]);

    let requests = server.hook(&hook_payload("Stop", &path, ""));

    assert_eq!(
        dispatched_action(&requests[0])["stop_session"]["title"],
        "My name"
    );
}

#[test]
fn an_unnamed_session_sends_its_latest_ai_title() {
    let server = recording_server();
    let (_dir, path) = transcript(&[ai_title("old"), ai_title("Road-128 ticket shaping")]);

    let requests = server.hook(&hook_payload("UserPromptSubmit", &path, ""));

    assert_eq!(
        dispatched_action(&requests[0])["hold_session"]["title"],
        "Road-128 ticket shaping"
    );
}

#[test]
fn only_the_end_of_a_long_transcript_is_read() {
    let server = recording_server();
    let filler = serde_json::json!({"type": "user", "text": "x".repeat(4096)}).to_string();
    let mut lines = vec![ai_title("too far back")];
    lines.extend(std::iter::repeat_n(filler, 100));
    let (_dir, path) = transcript(&lines);

    let requests = server.hook(&hook_payload("Stop", &path, ""));

    assert!(
        dispatched_action(&requests[0])["stop_session"]
            .get("title")
            .is_none()
    );
}

#[test]
fn a_title_comes_only_from_a_title_record_and_nothing_else_leaves_the_transcript() {
    let server = recording_server();
    let prompt = serde_json::json!({
        "type": "user",
        "text": r#"SECRET-PROMPT "type":"ai-title","aiTitle":"forged""#
    })
    .to_string();
    let (_dir, path) = transcript(&[prompt, r#"not json "type":"ai-title""#.to_owned()]);

    let requests = server.hook(&hook_payload("UserPromptSubmit", &path, ""));

    assert!(
        dispatched_action(&requests[0])["hold_session"]
            .get("title")
            .is_none()
    );
    assert!(!requests[0].to_string().contains("SECRET-PROMPT"));
}

#[test]
fn a_missing_transcript_still_sends_the_event() {
    let server = recording_server();

    let requests = server.hook(&hook_payload("Stop", "/nonexistent/session.jsonl", ""));

    assert_eq!(
        dispatched_action(&requests[0]),
        &serde_json::json!({"stop_session": {"session_id": "s-1"}})
    );
}

#[test]
fn a_subagent_event_with_no_agent_id_sends_nothing() {
    let server = recording_server();
    assert!(
        server
            .hook(r#"{"hook_event_name":"SubagentStart","session_id":"s-1"}"#)
            .is_empty()
    );
    assert!(
        server
            .hook(r#"{"hook_event_name":"SubagentStop","session_id":"s-1"}"#)
            .is_empty()
    );
}

#[test]
fn a_stop_without_a_reason_sends_stop_session() {
    let server = recording_server();

    let requests = server.hook(r#"{"hook_event_name":"Stop","session_id":"s-1"}"#);

    assert_eq!(requests.len(), 1);
    assert_eq!(
        dispatched_action(&requests[0]),
        &serde_json::json!({"stop_session": {"session_id": "s-1"}})
    );
}

#[test]
fn a_stop_with_reason_end_turn_sends_stop_session() {
    let server = recording_server();

    let requests =
        server.hook(r#"{"hook_event_name":"Stop","session_id":"s-1","reason":"end_turn"}"#);

    assert_eq!(requests.len(), 1);
    assert_eq!(
        dispatched_action(&requests[0]),
        &serde_json::json!({"stop_session": {"session_id": "s-1"}})
    );
}

#[test]
fn a_stop_with_another_reason_sends_nothing() {
    let server = recording_server();

    let requests =
        server.hook(r#"{"hook_event_name":"Stop","session_id":"s-1","reason":"tool_use"}"#);

    assert!(requests.is_empty());
}

#[test]
fn a_stop_cancelled_sends_stop_session() {
    let server = recording_server();

    let requests = server.hook(r#"{"hook_event_name":"StopCancelled","session_id":"s-1"}"#);

    assert_eq!(requests.len(), 1);
    assert_eq!(
        dispatched_action(&requests[0]),
        &serde_json::json!({"stop_session": {"session_id": "s-1"}})
    );
}

#[test]
fn a_codex_interrupt_sends_stop_session() {
    let server = recording_server();

    let requests = server.hook(
        r#"{"hook_event_name":"Interrupt","session_id":"s-1","turn_id":"t-1","transcript_path":null,"cwd":"/x","model":"m","permission_mode":"default"}"#,
    );

    assert_eq!(requests.len(), 1);
    assert_eq!(
        dispatched_action(&requests[0]),
        &serde_json::json!({"stop_session": {"session_id": "s-1"}})
    );
}

#[test]
fn a_stop_failure_sends_stop_session() {
    let server = recording_server();

    let requests = server.hook(r#"{"hook_event_name":"StopFailure","session_id":"s-1"}"#);

    assert_eq!(requests.len(), 1);
    assert_eq!(
        dispatched_action(&requests[0]),
        &serde_json::json!({"stop_session": {"session_id": "s-1"}})
    );
}

#[test]
fn a_session_end_sends_end_session_with_no_pid() {
    let server = recording_server();

    let requests = server.hook(r#"{"hook_event_name":"SessionEnd","session_id":"s-1"}"#);

    assert_eq!(requests.len(), 1);
    assert_eq!(
        dispatched_action(&requests[0]),
        &serde_json::json!({"end_session": {"session_id": "s-1"}})
    );
}

#[test]
fn an_unknown_event_sends_nothing() {
    let server = recording_server();

    let requests = server.hook(r#"{"hook_event_name":"PreToolUse","session_id":"s-1"}"#);

    assert!(requests.is_empty());
}

#[test]
fn a_payload_sends_nothing_but_the_session_id_pids_and_working_directory() {
    let server = recording_server();

    let requests = server.hook(
        r#"{"hook_event_name":"UserPromptSubmit","session_id":"s-1","prompt":"SECRET-PROMPT-TEXT","cwd":"/private/place"}"#,
    );

    assert_eq!(requests.len(), 1);
    assert!(!requests[0].to_string().contains("SECRET-PROMPT-TEXT"));
    let hold = dispatched_action(&requests[0])["hold_session"]
        .as_object()
        .unwrap();
    let mut keys: Vec<_> = hold.keys().map(String::as_str).collect();
    keys.sort_unstable();
    assert_eq!(keys, ["cwd", "pid", "ppid", "session_id"]);
    assert_eq!(hold["cwd"], "/private/place");
}

#[test]
fn a_stop_does_not_send_the_working_directory() {
    let server = recording_server();

    let requests =
        server.hook(r#"{"hook_event_name":"Stop","session_id":"s-1","cwd":"/private/place"}"#);

    assert_eq!(requests.len(), 1);
    assert!(!requests[0].to_string().contains("/private/place"));
}

#[test]
fn a_grok_payload_with_both_key_spellings_is_read_by_the_snake_case_keys() {
    let server = recording_server();

    let requests = server.hook(
        r#"{"hook_event_name":"SessionEnd","hookEventName":"Stop","session_id":"snake","sessionId":"camel"}"#,
    );

    assert_eq!(requests.len(), 1);
    assert_eq!(
        dispatched_action(&requests[0]),
        &serde_json::json!({"end_session": {"session_id": "snake"}})
    );
}

fn hook_cmd(home: &tempfile::TempDir) -> std::process::Command {
    let mut cmd = navori_process(home);
    cmd.args(["--profile", "dev", "hook"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    cmd
}

fn run_hook_with(
    mut cmd: std::process::Command,
    payload: &str,
) -> (std::process::Output, Duration) {
    use std::io::Write;

    let started = Instant::now();
    let mut child = cmd.spawn().unwrap();
    let mut stdin = child.stdin.take().unwrap();
    stdin.write_all(payload.as_bytes()).unwrap();
    drop(stdin);
    let output = child.wait_with_output().unwrap();
    (output, started.elapsed())
}

fn process_start_cost() -> Duration {
    let home = short_home();
    run_hook_with(hook_cmd(&home), SESSION_END).1
}

fn assert_silent_success(output: &std::process::Output) {
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
}

const PROMPT: &str = r#"{"hook_event_name":"UserPromptSubmit","session_id":"s-1"}"#;
const SESSION_END: &str = r#"{"hook_event_name":"SessionEnd","session_id":"s-1"}"#;
const STOP: &str = r#"{"hook_event_name":"Stop","session_id":"s-1"}"#;

#[test]
fn a_core_that_is_not_running_gives_exit_0_and_no_output() {
    let home = short_home();
    let (output, _) = run_hook_with(hook_cmd(&home), PROMPT);
    assert_silent_success(&output);
    let socket = home
        .path()
        .join("Library/Application Support/Navori Dev/run/navori.sock");
    assert!(!socket.exists());
}

#[test]
fn a_core_that_never_answers_gives_exit_0_within_two_seconds() {
    let start_cost = process_start_cost();
    let server = recording_server_replying(Reply::Never);
    let (output, took) = run_hook_with(hook_cmd(&server.home), PROMPT);
    assert_silent_success(&output);
    assert!(
        took >= Duration::from_millis(1000),
        "gave up early: {took:?}"
    );
    assert!(took < start_cost + Duration::from_secs(2), "took {took:?}");
}

#[test]
fn a_session_end_gives_up_within_one_second_when_the_core_never_answers() {
    let start_cost = process_start_cost();
    let server = recording_server_replying(Reply::Never);
    let (output, took) = run_hook_with(hook_cmd(&server.home), SESSION_END);
    assert_silent_success(&output);
    assert_eq!(server.requests.lock().unwrap().len(), 1);
    assert!(
        took >= Duration::from_millis(500),
        "gave up early: {took:?}"
    );
    assert!(took < start_cost + Duration::from_secs(1), "took {took:?}");
}

struct Relay {
    home: tempfile::TempDir,
    requests: Arc<std::sync::Mutex<Vec<Value>>>,
    replies: Arc<std::sync::Mutex<Vec<Value>>>,
}

fn relay_to(core_home: &std::path::Path) -> Relay {
    use std::io::{BufRead, BufReader, Write};
    use std::os::unix::net::{UnixListener, UnixStream};

    let paths = |home: &std::path::Path| {
        let uid = std::fs::metadata(home).unwrap().uid();
        navori_config::Paths::new(navori_config::Profile::Dev, home, uid).socket
    };
    let upstream = paths(core_home);
    let home = short_home();
    let socket = paths(home.path());
    std::fs::create_dir_all(socket.parent().unwrap()).unwrap();
    let listener = UnixListener::bind(&socket).unwrap();
    let requests = Arc::new(std::sync::Mutex::new(Vec::new()));
    let replies = Arc::new(std::sync::Mutex::new(Vec::new()));
    let (seen_requests, seen_replies) = (Arc::clone(&requests), Arc::clone(&replies));
    std::thread::spawn(move || {
        for client in listener.incoming() {
            let Ok(client) = client else { break };
            let core = UnixStream::connect(&upstream).unwrap();
            let (mut to_client, mut to_core) =
                (client.try_clone().unwrap(), core.try_clone().unwrap());
            let seen = Arc::clone(&seen_replies);
            std::thread::spawn(move || {
                for line in BufReader::new(core).lines() {
                    let Ok(line) = line else { break };
                    seen.lock()
                        .unwrap()
                        .push(serde_json::from_str(&line).unwrap());
                    if writeln!(to_client, "{line}").is_err() {
                        break;
                    }
                }
            });
            let seen = Arc::clone(&seen_requests);
            std::thread::spawn(move || {
                for line in BufReader::new(client).lines() {
                    let Ok(line) = line else { break };
                    seen.lock()
                        .unwrap()
                        .push(serde_json::from_str(&line).unwrap());
                    if writeln!(to_core, "{line}").is_err() {
                        break;
                    }
                }
            });
        }
    });
    Relay {
        home,
        requests,
        replies,
    }
}

#[test]
fn a_stop_against_a_running_core_finishes_inside_its_deadline() {
    let start_cost = process_start_cost();
    let running = start_core(None);
    let relay = relay_to(running.home.path());
    let (output, took) = run_hook_with(hook_cmd(&relay.home), STOP);
    println!("hook round trip against a running core: {took:?}");
    assert_silent_success(&output);
    let requests = relay.requests.lock().unwrap();
    assert_eq!(requests.len(), 1, "{requests:?}");
    assert_eq!(
        dispatched_action(&requests[0]),
        &serde_json::json!({"stop_session": {"session_id": "s-1"}})
    );
    let replies = relay.replies.lock().unwrap();
    assert_eq!(replies.len(), 1, "{replies:?}");
    assert_eq!(replies[0]["id"], requests[0]["id"]);
    assert!(replies[0].get("result").is_some(), "{replies:?}");
    assert!(
        took < start_cost + Duration::from_millis(1500),
        "took {took:?}"
    );
}

#[test]
fn stdin_that_never_closes_gives_exit_0_within_two_seconds() {
    let start_cost = process_start_cost();
    let server = recording_server();
    let started = Instant::now();
    let mut child = hook_cmd(&server.home).spawn().unwrap();
    let held_open = child.stdin.take().unwrap();
    let output = child.wait_with_output().unwrap();
    let took = started.elapsed();
    drop(held_open);
    assert_silent_success(&output);
    assert!(took < start_cost + Duration::from_secs(2), "took {took:?}");
    assert!(server.requests.lock().unwrap().is_empty());
}

#[test]
fn invalid_json_gives_exit_0_and_no_output() {
    let server = recording_server();
    let (output, _) = run_hook_with(hook_cmd(&server.home), "not json {");
    assert_silent_success(&output);
    assert!(server.requests.lock().unwrap().is_empty());
}

#[test]
fn a_missing_session_id_gives_exit_0_and_no_request() {
    let server = recording_server();
    let (output, _) = run_hook_with(
        hook_cmd(&server.home),
        r#"{"hook_event_name":"UserPromptSubmit"}"#,
    );
    assert_silent_success(&output);
    assert!(server.requests.lock().unwrap().is_empty());
}

#[test]
fn a_reply_of_invalid_for_a_non_harness_pid_gives_exit_0() {
    let server = recording_server_replying(Reply::Invalid);
    let (output, _) = run_hook_with(hook_cmd(&server.home), PROMPT);
    assert_silent_success(&output);
    assert_eq!(server.requests.lock().unwrap().len(), 1);
}

#[test]
fn a_bad_profile_variable_gives_exit_0_and_no_output() {
    let home = short_home();
    let release = serve(home.path(), navori_config::Profile::Release, Reply::Result);
    let mut cmd = navori_process(&home);
    cmd.arg("hook")
        .env("NAVORI_PROFILE", "bogus")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    let (output, _) = run_hook_with(cmd, PROMPT);
    assert_silent_success(&output);
    assert_eq!(release.lock().unwrap().len(), 1);
}

#[test]
fn a_profile_variable_of_dev_does_not_send_a_release_hook_to_the_dev_core() {
    let home = short_home();
    let release = serve(home.path(), navori_config::Profile::Release, Reply::Result);
    let dev = serve(home.path(), navori_config::Profile::Dev, Reply::Result);
    let mut cmd = navori_process(&home);
    cmd.arg("hook")
        .env("NAVORI_PROFILE", "dev")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    let (output, _) = run_hook_with(cmd, PROMPT);
    assert_silent_success(&output);
    assert_eq!(release.lock().unwrap().len(), 1);
    assert!(dev.lock().unwrap().is_empty());
}

#[test]
fn a_profile_flag_dev_reaches_only_the_dev_core() {
    let home = short_home();
    let release = serve(home.path(), navori_config::Profile::Release, Reply::Result);
    let dev = serve(home.path(), navori_config::Profile::Dev, Reply::Result);
    let (output, _) = run_hook_with(hook_cmd(&home), PROMPT);
    assert_silent_success(&output);
    assert_eq!(dev.lock().unwrap().len(), 1);
    assert!(release.lock().unwrap().is_empty());
}

#[test]
fn awake_run_with_an_argument_named_hook_still_runs_its_command() {
    let home = short_home();
    navori(&home)
        .args(["awake", "run", "--", "echo", "hook"])
        .assert()
        .success()
        .stdout("hook\n");
    navori(&home)
        .args(["awake", "run", "--", "sh", "-c", "exit 7"])
        .assert()
        .code(7);
}

#[test]
fn other_commands_keep_their_exit_codes() {
    let home = short_home();
    navori(&home).arg("status").assert().code(3);
    navori(&home).arg("--no-such-flag").assert().code(2);
}

#[test]
fn a_hook_dispatch_is_a_single_awake_call_per_event() {
    let server = recording_server();
    let requests = server.hook(PROMPT);
    assert_eq!(requests.len(), 1);
    assert_eq!(
        dispatched_action(&requests[0])["hold_session"]["session_id"],
        "s-1"
    );
}

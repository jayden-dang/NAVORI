//! NavoriCore driven from Rust exactly as Swift drives it: plain threads, no ambient runtime.
#![allow(clippy::unwrap_used)]

use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use navori_config::{ConfigReport, JournalSweep, Paths, Profile, WholeFileReason};
use navori_ffi::{
    CheckView, ConfigNotice, CoreConfig, CoreError, CoreSnapshot, NavoriCore, SnapshotListener,
};
use navori_protocol::rpc::method;
use navori_protocol::{CoreStatus, Event, Severity};
use navori_store::Store;

#[derive(Default)]
struct Recorder(Mutex<Vec<CoreSnapshot>>);

impl SnapshotListener for Recorder {
    fn on_snapshot(&self, snapshot: CoreSnapshot) {
        self.0.lock().unwrap().push(snapshot);
    }
}

/// A short home path so the socket stays under the 104-byte `sun_path` limit.
fn home() -> tempfile::TempDir {
    tempfile::Builder::new()
        .prefix("nv")
        .tempdir_in("/tmp")
        .unwrap()
}

fn config(home: &tempfile::TempDir) -> CoreConfig {
    CoreConfig {
        profile: "dev".into(),
        home_dir: home.path().to_string_lossy().into_owned(),
        uid: std::os::unix::fs::MetadataExt::uid(&std::fs::metadata(home.path()).unwrap()),
        app_version: "0.1.0-test".into(),
        log_to_file: false,
    }
}

fn write_config(home: &tempfile::TempDir, body: &str) {
    let data = home.path().join("Library/Application Support/Navori Dev");
    std::fs::create_dir_all(&data).unwrap();
    std::fs::write(data.join("config.toml"), body).unwrap();
}

fn start(home: &tempfile::TempDir) -> (Arc<NavoriCore>, Arc<Recorder>) {
    let rec = Arc::new(Recorder::default());
    let core = NavoriCore::start(config(home), rec.clone()).unwrap();
    (core, rec)
}

fn eventually(mut pred: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline {
        if pred() {
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("condition not reached in time");
}

#[test]
fn starting_creates_the_profile_layout_and_reports_status() {
    let home = home();
    let (core, _) = start(&home);
    let s = core.status();
    assert_eq!(s.version, "0.1.0-test");
    assert_eq!(s.profile, "dev");
    assert_eq!(s.pid, std::process::id());
    assert!(
        s.data_dir
            .ends_with("Library/Application Support/Navori Dev"),
        "{}",
        s.data_dir
    );
    assert!(std::path::Path::new(&s.data_dir).join("navori.db").exists());
    assert!(std::path::Path::new(&s.socket_path).exists());
    assert!(s.config_notice.is_none());
    assert!(s.last_unclean_exit.is_none());
    eventually(|| {
        core.status()
            .modules
            .iter()
            .any(|m| m.id == "system" && m.state == "running")
    });
    core.shutdown();
}

#[test]
fn dispatch_takes_and_returns_json() {
    let home = home();
    let (core, _) = start(&home);
    let out =
        futures::executor::block_on(core.dispatch("system".into(), "\"ping\"".into())).unwrap();
    assert_eq!(out, "\"pong\"");

    let err = futures::executor::block_on(core.dispatch("system".into(), "{not json".into()))
        .unwrap_err();
    assert!(matches!(err, CoreError::InvalidJson { .. }), "{err}");

    let err =
        futures::executor::block_on(core.dispatch("ghost".into(), "\"ping\"".into())).unwrap_err();
    assert!(
        matches!(err, CoreError::Dispatch { ref message } if message.contains("ghost")),
        "{err}"
    );
    core.shutdown();
}

#[test]
fn listeners_receive_a_snapshot_when_state_changes() {
    let home = home();
    let (core, rec) = start(&home);
    eventually(|| !rec.0.lock().unwrap().is_empty());
    let before = rec.0.lock().unwrap().len();
    futures::executor::block_on(core.dispatch(
        "system".into(),
        r#"{"prune_journal":{"older_than_days":1}}"#.into(),
    ))
    .unwrap();
    eventually(|| rec.0.lock().unwrap().len() > before);
    core.shutdown();
}

#[test]
fn actions_are_exposed_with_their_risk() {
    let home = home();
    let (core, _) = start(&home);
    let actions = core.actions();
    let ping = actions
        .iter()
        .find(|a| a.module == "system" && a.name == "ping")
        .unwrap();
    assert_eq!(ping.risk, "read");
    let prune = actions.iter().find(|a| a.name == "prune_journal").unwrap();
    assert_eq!(prune.risk, "write");
    core.shutdown();
}

#[test]
fn the_ipc_socket_serves_core_status() {
    let home = home();
    let (core, _) = start(&home);
    let socket = core.status().socket_path;
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let status: CoreStatus = rt.block_on(async {
        let mut client = navori_ipc::Client::connect(std::path::Path::new(&socket))
            .await
            .unwrap();
        serde_json::from_value(
            client
                .call(method::STATUS, serde_json::Value::Null)
                .await
                .unwrap(),
        )
        .unwrap()
    });
    assert_eq!(status.pid, std::process::id());
    assert_eq!(status.profile, "dev");
    core.shutdown();
}

#[test]
fn an_invalid_config_falls_back_to_defaults_and_is_reported() {
    let home = home();
    write_config(&home, "log_level = 42\n");
    let (core, _) = start(&home);
    let notice = core
        .status()
        .config_notice
        .expect("config notice must be surfaced");
    assert!(notice.title.contains("config.toml"), "{}", notice.title);
    assert!(notice.message.contains("log_level"), "{}", notice.message);
    core.shutdown();
}

#[test]
fn a_bad_value_is_named_and_does_not_claim_the_whole_file() {
    let home = home();
    write_config(&home, "log_level = \"loud\"\n");
    let (core, _) = start(&home);
    let notice = core.status().config_notice.expect("bad value is a notice");
    assert_eq!(
        notice,
        ConfigNotice {
            title: "config.toml has bad values".into(),
            message: "log_level use their defaults. Fix them and reopen Navori.".into(),
        }
    );
    core.shutdown();
}

#[test]
fn a_bad_module_section_is_named() {
    let file = Path::new("/tmp/nv/config.toml");
    let alone = navori_ffi::config_notice(
        &ConfigReport::Clean,
        &JournalSweep::KeepDays(30),
        &["awake"],
        file,
    )
    .expect("a module section on its default is a notice");
    assert_eq!(
        alone,
        ConfigNotice {
            title: "config.toml has bad values".into(),
            message: "awake use their defaults. Fix them and reopen Navori.".into(),
        }
    );

    let both = navori_ffi::config_notice(
        &ConfigReport::BadValues(vec!["log_level".into()]),
        &JournalSweep::KeepDays(30),
        &["awake"],
        file,
    )
    .expect("a value and a module section are both named");
    assert_eq!(
        both.message,
        "log_level, awake use their defaults. Fix them and reopen Navori."
    );
    assert!(!both.title.contains("is on defaults"));
    assert!(!both.message.contains("is on defaults"));
}

#[test]
fn invalid_toml_says_the_whole_file_is_on_defaults_and_names_it() {
    let file = Path::new("/tmp/nv/config.toml");
    let no_position = navori_ffi::config_notice(
        &ConfigReport::WholeFile(WholeFileReason::NotToml { position: None }),
        &JournalSweep::Paused,
        &[],
        file,
    )
    .expect("invalid toml is a notice");
    assert_eq!(
        no_position,
        ConfigNotice {
            title: "config.toml is on defaults".into(),
            message: "Not valid TOML. Journal cleanup is paused until the file is fixed and Navori is reopened.".into(),
        }
    );

    let positioned = navori_ffi::config_notice(
        &ConfigReport::WholeFile(WholeFileReason::NotToml {
            position: Some((3, 7)),
        }),
        &JournalSweep::Paused,
        &[],
        file,
    )
    .expect("a toml position is a notice");
    assert_eq!(
        positioned.message,
        "Not valid TOML at line 3, column 7. Journal cleanup is paused until the file is fixed and Navori is reopened."
    );
    assert_eq!(positioned.title, "config.toml is on defaults");

    let home = home();
    write_config(&home, "log_level = \n");
    let (core, _) = start(&home);
    let notice = core
        .status()
        .config_notice
        .expect("broken toml is a notice");
    assert_eq!(notice.title, "config.toml is on defaults");
    assert!(
        notice
            .message
            .starts_with("Not valid TOML at line 1, column "),
        "{}",
        notice.message
    );
    assert!(notice.message.ends_with('.'), "{}", notice.message);
    assert!(!notice.message.contains("log_level"), "{}", notice.message);
    assert!(
        notice
            .message
            .contains("Journal cleanup is paused until the file is fixed and Navori is reopened."),
        "{}",
        notice.message
    );
    core.shutdown();
}

#[test]
fn an_unreadable_file_says_it_cannot_be_read() {
    let home = home();
    let data = home.path().join("Library/Application Support/Navori Dev");
    std::fs::create_dir_all(&data).unwrap();
    std::fs::create_dir(data.join("config.toml")).unwrap();
    let (core, _) = start(&home);
    let notice = core
        .status()
        .config_notice
        .expect("unreadable file is a notice");
    assert_eq!(
        notice,
        ConfigNotice {
            title: "config.toml is on defaults".into(),
            message: "It cannot be read. Journal cleanup is paused until the file is fixed and Navori is reopened.".into(),
        }
    );
    core.shutdown();
}

#[test]
fn doctor_fails_the_config_check_on_a_bad_value() {
    let home = home();
    write_config(&home, "log_level = \"loud\"\n");
    let (core, _) = start(&home);
    let check = config_check(&core);
    assert!(!check.ok);
    assert_eq!(
        check.detail,
        "config.toml has bad values. log_level use their defaults. Fix them and reopen Navori."
    );
    core.shutdown();
}

#[test]
fn a_bad_value_does_not_stop_the_core() {
    let home = home();
    write_config(&home, "log_level = \"loud\"\n");
    let (core, _) = start(&home);
    assert!(core.status().config_notice.is_some());
    let out =
        futures::executor::block_on(core.dispatch("system".into(), "\"ping\"".into())).unwrap();
    assert_eq!(out, "\"pong\"");
    eventually(|| {
        core.status()
            .modules
            .iter()
            .any(|m| m.id == "system" && m.state == "running")
    });
    core.shutdown();
}

fn config_check(core: &NavoriCore) -> CheckView {
    core.doctor()
        .into_iter()
        .find(|check| check.name == "config")
        .unwrap()
}

#[test]
fn a_second_instance_of_the_same_profile_refuses_to_start() {
    let home = home();
    let (core, _) = start(&home);
    let err = NavoriCore::start(config(&home), Arc::new(Recorder::default())).unwrap_err();
    assert!(
        matches!(err, CoreError::Startup { ref message } if message.contains("already")),
        "{err}"
    );
    core.shutdown();
}

#[test]
fn a_corrupt_database_startup_error_display_starts_with_startup_failed() {
    let home = home();
    let data = home.path().join("Library/Application Support/Navori Dev");
    std::fs::create_dir_all(&data).unwrap();
    std::fs::write(data.join("navori.db"), "this is not a sqlite database\n").unwrap();
    let err = NavoriCore::start(config(&home), Arc::new(Recorder::default())).unwrap_err();
    let display = err.to_string();
    assert!(
        display.starts_with("startup failed:"),
        "shell banner must surface Display, got {display:?}"
    );
    assert!(
        display.contains("database"),
        "expected database context in {display:?}"
    );
}

#[test]
fn clean_shutdown_removes_the_socket_and_an_unclean_previous_exit_is_reported() {
    let home = home();
    let (core, _) = start(&home);
    let socket = core.status().socket_path;
    let data_dir = core.status().data_dir;
    core.shutdown();
    assert!(!std::path::Path::new(&socket).exists());
    let err =
        futures::executor::block_on(core.dispatch("system".into(), "\"ping\"".into())).unwrap_err();
    assert_eq!(err, CoreError::ShutDown);

    // Clean exit: the next start reports nothing.
    let (core, _) = start(&home);
    assert!(core.status().last_unclean_exit.is_none());
    core.shutdown();

    // Simulate a crash: the session marker of a previous run is still there.
    std::fs::write(
        std::path::Path::new(&data_dir).join("session.marker"),
        "pid 999 started 1\npanic: boom\n",
    )
    .unwrap();
    let (core, _) = start(&home);
    let note = core
        .status()
        .last_unclean_exit
        .expect("unclean exit must be reported");
    assert!(note.contains("boom"), "{note}");
    core.shutdown();
}

#[test]
fn doctor_reports_a_healthy_core() {
    let home = home();
    let (core, _) = start(&home);
    let checks = core.doctor();
    let names: Vec<_> = checks.iter().map(|c| c.name.as_str()).collect();
    for expected in ["config", "database", "ipc", "modules"] {
        assert!(
            names.contains(&expected),
            "missing check {expected}: {names:?}"
        );
    }
    eventually(|| core.doctor().iter().all(|c| c.ok));
    core.shutdown();
}

const PAUSE_SENTENCE: &str =
    "Journal cleanup is paused until the file is fixed and Navori is reopened.";

fn seed_event_from_forty_days_ago(home: &tempfile::TempDir) {
    let uid = std::os::unix::fs::MetadataExt::uid(&std::fs::metadata(home.path()).unwrap());
    let paths = Paths::new(Profile::Dev, home.path(), uid);
    let store = Store::open(&paths.db_file, &paths.backups_dir).unwrap();
    let day_ms: i64 = 24 * 60 * 60 * 1000;
    let now_ms = i64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis(),
    )
    .unwrap();
    store
        .journal_append(&Event {
            id: "old".into(),
            ts_ms: now_ms - 40 * day_ms,
            module: "test".into(),
            kind: "seed".into(),
            severity: Severity::Info,
            summary: "old".into(),
            payload: serde_json::json!(null),
        })
        .unwrap();
}

fn journal_has_old_event(core: &NavoriCore) -> bool {
    core.status()
        .recent_events
        .iter()
        .any(|event| event.id == "old")
}

fn old_event_is_deleted_within_one_second(core: &NavoriCore) {
    let deadline = Instant::now() + Duration::from_secs(1);
    while Instant::now() < deadline {
        if !journal_has_old_event(core) {
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("the 40-day event was still in the journal");
}

fn old_event_stays_for_one_second(core: &NavoriCore) {
    let deadline = Instant::now() + Duration::from_secs(1);
    while Instant::now() < deadline {
        assert!(journal_has_old_event(core), "the 40-day event was deleted");
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn pause_is_reported(core: &NavoriCore) {
    let notice = core.status().config_notice.unwrap();
    assert!(
        notice.message.contains(PAUSE_SENTENCE),
        "{}",
        notice.message
    );
    let check = config_check(core);
    assert!(
        check.detail.contains("Journal cleanup is paused"),
        "{}",
        check.detail
    );
}

#[test]
fn absent_retention_deletes_events_older_than_30_days() {
    let bodies: [Option<&str>; 3] = [
        None,
        Some("log_level = \"info\"\n"),
        Some("[journal]\n# retention_days = 365\n"),
    ];
    for body in bodies {
        let home = home();
        if let Some(body) = body {
            write_config(&home, body);
        }
        seed_event_from_forty_days_ago(&home);
        let (core, _) = start(&home);
        old_event_is_deleted_within_one_second(&core);
        core.shutdown();
    }
}

#[test]
fn a_broken_retention_or_file_keeps_old_events_and_says_the_sweep_is_paused() {
    let bodies = [
        "[journal]\nretention_days = 0\n",
        "[journal]\nretention_days = \"365\"\n",
        "journal = 3\n",
        "log_level = \n",
    ];
    for body in bodies {
        let home = home();
        write_config(&home, body);
        seed_event_from_forty_days_ago(&home);
        let (core, _) = start(&home);
        old_event_stays_for_one_second(&core);
        pause_is_reported(&core);
        core.shutdown();
    }
}

#[test]
fn a_valid_retention_keeps_what_it_says() {
    let home = home();
    write_config(&home, "[journal]\nretention_days = 90\n");
    seed_event_from_forty_days_ago(&home);
    let (core, _) = start(&home);
    old_event_stays_for_one_second(&core);
    core.shutdown();
}

#[test]
fn a_bad_log_level_alone_does_not_pause_the_sweep() {
    let home = home();
    write_config(&home, "log_level = \"loud\"\n");
    seed_event_from_forty_days_ago(&home);
    let (core, _) = start(&home);
    old_event_is_deleted_within_one_second(&core);
    let notice = core
        .status()
        .config_notice
        .expect("bad log level is a notice");
    assert!(
        !notice.message.contains(PAUSE_SENTENCE),
        "{}",
        notice.message
    );
    core.shutdown();
}

#[test]
fn a_paused_sweep_leaves_other_modules_running() {
    let home = home();
    write_config(&home, "[journal]\nretention_days = 0\n");
    let (core, _) = start(&home);
    eventually(|| {
        core.doctor()
            .into_iter()
            .any(|check| check.name == "modules" && check.ok)
    });
    let out =
        futures::executor::block_on(core.dispatch("system".into(), "\"ping\"".into())).unwrap();
    assert_eq!(out, "\"pong\"");
    core.shutdown();
}

#[test]
fn fixing_the_file_keeps_the_pause_until_the_next_start() {
    let home = home();
    write_config(&home, "[journal]\nretention_days = 0\n");
    seed_event_from_forty_days_ago(&home);
    let (core, _) = start(&home);
    pause_is_reported(&core);
    old_event_stays_for_one_second(&core);
    write_config(&home, "[journal]\nretention_days = 30\n");
    pause_is_reported(&core);
    core.shutdown();

    let (core, _) = start(&home);
    assert!(core.status().config_notice.is_none());
    old_event_is_deleted_within_one_second(&core);
    core.shutdown();
}

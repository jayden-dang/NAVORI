//! NavoriCore driven from Rust exactly as Swift drives it: plain threads, no ambient runtime.
#![allow(clippy::unwrap_used)]

use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use navori_config::{ConfigReport, JournalSweep, Paths, Profile, WholeFileReason};
use navori_ffi::{
    CheckView, ConfigNotice, CoreConfig, CoreError, CoreSnapshot, NavoriCore, SnapshotListener,
    TrayGlyphView, ports_check,
};
use navori_mod_ports::{DockerStatus, PortsState};
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
        helper_path: home
            .path()
            .join("Navori Dev.app/Contents/Helpers/navori")
            .to_string_lossy()
            .into_owned(),
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

fn snapshot_count(rec: &Recorder) -> usize {
    rec.0.lock().unwrap().len()
}

fn wait_until_snapshots_are_quiet(rec: &Recorder) {
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut last = snapshot_count(rec);
    let mut quiet_since = Instant::now();
    while Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
        let count = snapshot_count(rec);
        if count != last {
            last = count;
            quiet_since = Instant::now();
            continue;
        }
        if quiet_since.elapsed() > Duration::from_millis(250) {
            return;
        }
    }
    panic!("snapshots did not stay quiet for over 250 ms");
}

fn ms_until_prune_snapshot(core: &NavoriCore, rec: &Recorder) -> u128 {
    let before = snapshot_count(rec);
    let started = Instant::now();
    futures::executor::block_on(core.dispatch(
        "system".into(),
        r#"{"prune_journal":{"older_than_days":1}}"#.into(),
    ))
    .unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < deadline {
        if snapshot_count(rec) > before {
            return started.elapsed().as_millis();
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    panic!("prune_journal produced no snapshot");
}

#[test]
fn snapshot_pushes_coalesce_at_33ms_with_the_panel_open_and_250ms_closed() {
    let home = home();
    let (core, rec) = start(&home);
    wait_until_snapshots_are_quiet(&rec);
    let closed = ms_until_prune_snapshot(&core, &rec);
    assert!(
        closed >= 240,
        "a closed panel delivered the snapshot in {closed} ms"
    );

    wait_until_snapshots_are_quiet(&rec);
    core.set_panel_visible(true);
    let open = ms_until_prune_snapshot(&core, &rec);
    assert!(
        open < 200,
        "an open panel delivered the snapshot in {open} ms"
    );
    core.shutdown();
}

#[test]
fn a_system_wake_with_a_running_core_is_harmless() {
    let home = home();
    let (core, _) = start(&home);
    core.system_did_wake();
    let out =
        futures::executor::block_on(core.dispatch("system".into(), "\"ping\"".into())).unwrap();
    assert_eq!(out, "\"pong\"");
    core.shutdown();
}

#[test]
fn calling_shutdown_twice_is_harmless() {
    let home = home();
    let (core, _) = start(&home);
    core.shutdown();
    core.shutdown();
    let err =
        futures::executor::block_on(core.dispatch("system".into(), "\"ping\"".into())).unwrap_err();
    assert_eq!(err, CoreError::ShutDown);
}

fn start_profile(home: &tempfile::TempDir, profile: &str) -> (Arc<NavoriCore>, Arc<Recorder>) {
    let rec = Arc::new(Recorder::default());
    let mut cfg = config(home);
    cfg.profile = profile.to_owned();
    let core = NavoriCore::start(cfg, rec.clone()).unwrap();
    (core, rec)
}

fn awake_of(core: &NavoriCore) -> navori_ffi::AwakeView {
    match core.status().awake {
        Some(awake) => awake,
        None => panic!("the snapshot carries awake"),
    }
}

fn dispatch_json(core: &NavoriCore, module: &str, action: &str) -> String {
    futures::executor::block_on(core.dispatch(module.to_owned(), action.to_owned())).unwrap()
}

#[test]
fn the_snapshot_carries_tracking_agents() {
    let home = home();
    let (core, _rec) = start(&home);
    assert!(awake_of(&core).tracking);
    dispatch_json(&core, "awake", r#"{"tracking":{"on":false}}"#);
    assert!(!awake_of(&core).tracking);
}

#[test]
fn the_snapshot_carries_awake_after_a_hold() {
    let home = home();
    let (core, rec) = start(&home);
    let idle = awake_of(&core);
    assert!(idle.leases.is_empty(), "{idle:?}");
    let before = snapshot_count(&rec);
    dispatch_json(&core, "awake", r#"{"hold_for":{"seconds":3600}}"#);
    eventually(|| awake_of(&core).leases.len() == 1 && awake_of(&core).system_held);
    let awake = awake_of(&core);
    assert_eq!(awake.leases.len(), 1);
    let lease = &awake.leases[0];
    assert_eq!(lease.key, "manual");
    assert_eq!(lease.kind, "duration");
    assert_eq!(lease.label.as_deref(), Some("1 hour"));
    assert_eq!(lease.harness, None);
    assert_eq!(lease.pid, None);
    assert!(!lease.stopped);
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;
    let ends = lease.ends_at_ms.expect("a duration lease has an end");
    assert!(
        (now_ms + 3_590_000..=now_ms + 3_610_000).contains(&ends),
        "ends_at_ms {ends} is not about an hour from {now_ms}"
    );
    assert!(awake.system_held);
    assert!(!awake.display_held);
    assert!(awake.problem.is_none(), "{awake:?}");
    eventually(|| {
        snapshot_count(&rec) > before
            && rec
                .0
                .lock()
                .unwrap()
                .last()
                .and_then(|shot| shot.awake.as_ref())
                .is_some_and(|view| view.system_held && view.leases.len() == 1)
    });
    let row = core
        .doctor()
        .into_iter()
        .find(|check| check.name == "awake")
        .unwrap();
    assert!(row.ok);
    assert_eq!(row.detail, "1 Lease");
    core.shutdown();
}

#[test]
fn an_idle_core_reports_a_passing_awake_row_and_keeps_the_other_rows() {
    let home = home();
    let (core, _) = start(&home);
    eventually(|| core.doctor().iter().all(|check| check.ok));
    let checks = core.doctor();
    let names: Vec<_> = checks.iter().map(|check| check.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "config",
            "database",
            "ipc",
            "modules",
            "awake",
            "integrations",
            "logs",
            "ports"
        ]
    );
    let awake = checks.iter().find(|check| check.name == "awake").unwrap();
    assert!(awake.ok);
    assert_eq!(awake.detail, "no problem");
    core.shutdown();
}

#[test]
fn a_stray_awake_table_in_config_raises_no_notice() {
    let home = home();
    write_config(&home, "[awake]\nseconds = 1\nname = \"not a setting\"\n");
    let (core, _) = start(&home);
    assert!(core.status().config_notice.is_none());
    let config = config_check(&core);
    assert!(config.ok, "{}", config.detail);
    assert!(!config.detail.contains("awake"), "{}", config.detail);
    let awake = awake_of(&core);
    assert!(awake.leases.is_empty(), "{awake:?}");
    assert!(!awake.system_held);
    core.shutdown();
}

#[test]
fn dispatching_integrations_status_lists_three_harnesses_over_a_temp_home() {
    let home = home();
    std::fs::create_dir_all(home.path().join(".claude")).unwrap();
    let (core, _) = start(&home);
    let answer: serde_json::Value =
        serde_json::from_str(&dispatch_json(&core, "integrations", "\"status\"")).unwrap();
    let rows = answer["harnesses"].as_array().unwrap();
    let seen: Vec<(&str, &str)> = rows
        .iter()
        .map(|row| {
            (
                row["harness"].as_str().unwrap(),
                row["state"].as_str().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        seen,
        [
            ("claude", "not_installed"),
            ("codex", "not_found"),
            ("grok", "not_found")
        ]
    );
    core.shutdown();
}

#[test]
fn an_idle_core_holds_no_assertion() {
    let home = home();
    let (core, _) = start(&home);
    let awake = awake_of(&core);
    assert!(!awake.system_held);
    assert!(!awake.display_held);
    assert!(awake.leases.is_empty());
    let answer: serde_json::Value =
        serde_json::from_str(&dispatch_json(&core, "awake", "\"status\"")).unwrap();
    assert_eq!(answer["leases"], serde_json::json!([]));
    assert_eq!(answer["system_held"], false);
    core.shutdown();
}

#[test]
fn the_system_module_still_runs_beside_awake() {
    let home = home();
    let (core, _) = start(&home);
    eventually(|| {
        let modules = core.status().modules;
        modules
            .iter()
            .any(|module| module.id == "system" && module.state == "running")
            && modules
                .iter()
                .any(|module| module.id == "awake" && module.state == "running")
    });
    assert_eq!(dispatch_json(&core, "system", "\"ping\""), "\"pong\"");
    core.shutdown();
}

#[test]
fn a_lease_in_the_dev_profile_is_absent_from_the_release_profile() {
    let home = home();
    let (dev, _) = start_profile(&home, "dev");
    let (release, _) = start_profile(&home, "release");
    let dev_status = dev.status();
    let release_status = release.status();
    assert_ne!(dev_status.socket_path, release_status.socket_path);
    assert_ne!(dev_status.data_dir, release_status.data_dir);
    assert_ne!(dev_status.logs_dir, release_status.logs_dir);
    assert!(
        std::path::Path::new(&dev_status.data_dir)
            .join("navori.db")
            .exists()
    );
    assert!(
        std::path::Path::new(&release_status.data_dir)
            .join("navori.db")
            .exists()
    );
    assert!(
        dev_status.logs_dir.ends_with("Navori Dev"),
        "{}",
        dev_status.logs_dir
    );
    assert!(
        release_status.logs_dir.ends_with("Navori"),
        "{}",
        release_status.logs_dir
    );
    dispatch_json(&dev, "awake", r#"{"hold_for":{"seconds":3600}}"#);
    eventually(|| awake_of(&dev).system_held && awake_of(&dev).leases.len() == 1);
    let release_awake = awake_of(&release);
    assert!(
        release_awake.leases.is_empty(),
        "release saw the dev lease: {release_awake:?}"
    );
    assert!(!release_awake.system_held);
    dev.shutdown();
    release.shutdown();
}

#[test]
fn the_snapshot_and_the_status_carry_the_same_icon_state() {
    let home = home();
    let (core, _) = start(&home);
    let snapshot = core.status();
    assert_eq!(snapshot.tray.glyph, TrayGlyphView::Normal);
    assert!(!snapshot.tray.attention);
    assert_eq!(snapshot.tray.summary, "Navori");

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let rpc_status: CoreStatus = rt.block_on(async {
        let mut client = navori_ipc::Client::connect(std::path::Path::new(&snapshot.socket_path))
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
    assert_eq!(rpc_status.tray.glyph, navori_protocol::TrayGlyph::Normal);
    assert_eq!(rpc_status.tray.summary, snapshot.tray.summary);
    core.shutdown();
}

#[test]
fn a_bad_value_is_named_in_the_icon_summary() {
    let home = home();
    write_config(&home, "log_level = \"loud\"\n");
    let (core, _) = start(&home);
    let tray = core.status().tray;
    assert!(
        tray.summary.contains("config.toml has bad values"),
        "{}",
        tray.summary
    );
    assert!(tray.summary.contains("log_level"), "{}", tray.summary);
    core.shutdown();
}

#[test]
fn a_whole_file_fallback_says_so_in_the_icon_summary() {
    let home = home();
    write_config(&home, "log_level = \n");
    let (core, _) = start(&home);
    let tray = core.status().tray;
    assert!(
        tray.summary.contains("config.toml is on defaults"),
        "{}",
        tray.summary
    );
    core.shutdown();
}

fn ports_state(value: serde_json::Value) -> PortsState {
    serde_json::from_value(value).unwrap()
}

fn one_port(port: u16) -> serde_json::Value {
    serde_json::json!({
        "port": port,
        "addresses": ["127.0.0.1"],
        "pid": 100,
        "start_us": 1,
        "name": "node",
        "exe": null,
        "project": null,
        "worktree": null,
        "branch": null,
        "missing_path": null,
        "harness": null,
        "kind": "other",
        "container": null
    })
}

fn scanned_with_ports(ports: &[u16]) -> PortsState {
    let entries: Vec<_> = ports.iter().map(|port| one_port(*port)).collect();
    ports_state(serde_json::json!({
        "scanned_at_ms": 1_700_000_000_000_u64,
        "skipped": 0,
        "docker": "not_seen",
        "problem": null,
        "groups": [{
            "project": null,
            "worktrees": [{
                "worktree": null,
                "processes": [{
                    "pid": 100,
                    "start_us": 1,
                    "name": "node",
                    "exe": null,
                    "harness": null,
                    "entries": entries
                }]
            }]
        }]
    }))
}

#[test]
fn the_ports_row_follows_the_module_and_its_last_scan() {
    let failed = PortsState {
        scanned_at_ms: Some(1_700_000_000_000),
        problem: Some("reading the process list failed".to_owned()),
        ..PortsState::default()
    };
    let docker_unreadable = PortsState {
        scanned_at_ms: Some(1_700_000_000_000),
        docker: DockerStatus::Unreadable("Docker did not answer in time.".to_owned()),
        ..PortsState::default()
    };
    let table = [
        (
            "the module is down",
            ports_check(true, Some(&PortsState::default())),
            false,
            "The Ports module stopped.",
        ),
        (
            "the state is unreadable",
            ports_check(false, None),
            false,
            "Ports' state could not be read.",
        ),
        (
            "the last scan failed",
            ports_check(false, Some(&failed)),
            false,
            "reading the process list failed",
        ),
        (
            "nothing is scanned yet",
            ports_check(false, Some(&PortsState::default())),
            true,
            "not scanned yet",
        ),
        (
            "no port is listening",
            ports_check(false, Some(&scanned_with_ports(&[]))),
            true,
            "0 ports",
        ),
        (
            "one port is listening",
            ports_check(false, Some(&scanned_with_ports(&[3000]))),
            true,
            "1 port",
        ),
        (
            "three ports are listening",
            ports_check(false, Some(&scanned_with_ports(&[3000, 3001, 3002]))),
            true,
            "3 ports",
        ),
        (
            "docker could not be read",
            ports_check(false, Some(&docker_unreadable)),
            true,
            "0 ports",
        ),
    ];
    for (when, row, ok, detail) in table {
        assert_eq!(row.name, "ports", "{when}");
        assert_eq!(row.ok, ok, "{when}");
        assert_eq!(row.detail, detail, "{when}");
    }
}

#[test]
fn the_ports_row_passes_before_the_first_scan() {
    let home = home();
    let (core, _) = start(&home);
    let row = core
        .doctor()
        .into_iter()
        .find(|check| check.name == "ports")
        .unwrap();
    assert!(row.ok);
    assert_eq!(row.detail, "not scanned yet");
    core.shutdown();
}

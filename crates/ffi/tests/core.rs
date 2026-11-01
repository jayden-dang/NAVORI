//! NavoriCore driven from Rust exactly as Swift drives it: plain threads, no ambient runtime.
#![allow(clippy::unwrap_used)]

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use navori_ffi::{CoreConfig, CoreError, CoreSnapshot, NavoriCore, SnapshotListener};
use navori_protocol::CoreStatus;
use navori_protocol::rpc::method;

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
    assert!(s.config_error.is_none());
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
    let data = home.path().join("Library/Application Support/Navori Dev");
    std::fs::create_dir_all(&data).unwrap();
    std::fs::write(data.join("config.toml"), "log_level = 42\n").unwrap();
    let (core, _) = start(&home);
    let err = core
        .status()
        .config_error
        .expect("config error must be surfaced");
    assert!(err.contains("config.toml"), "{err}");
    core.shutdown();
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

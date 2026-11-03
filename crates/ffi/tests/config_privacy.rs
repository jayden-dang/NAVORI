//! One process, because file logging installs its subscriber once.
#![allow(clippy::unwrap_used)]

use std::os::unix::fs::MetadataExt;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use navori_ffi::{CoreConfig, CoreSnapshot, NavoriCore, SnapshotListener};

struct Ignore;
impl SnapshotListener for Ignore {
    fn on_snapshot(&self, _: CoreSnapshot) {}
}

fn home() -> tempfile::TempDir {
    tempfile::Builder::new()
        .prefix("nv")
        .tempdir_in("/tmp")
        .unwrap()
}

fn start(home: &tempfile::TempDir) -> Arc<NavoriCore> {
    NavoriCore::start(
        CoreConfig {
            profile: "dev".into(),
            home_dir: home.path().to_string_lossy().into_owned(),
            uid: std::fs::metadata(home.path()).unwrap().uid(),
            app_version: "0.1.0-test".into(),
            log_to_file: true,
        },
        Arc::new(Ignore),
    )
    .unwrap()
}

fn write_config(home: &tempfile::TempDir, body: &str) {
    let data = home.path().join("Library/Application Support/Navori Dev");
    std::fs::create_dir_all(&data).unwrap();
    std::fs::write(data.join("config.toml"), body).unwrap();
}

fn surface(core: &NavoriCore) -> String {
    let snapshot = core.status();
    let mut text = String::new();
    text.push_str(&snapshot.tray.summary);
    text.push('\n');
    if let Some(notice) = snapshot.config_notice {
        text.push_str(&notice.title);
        text.push('\n');
        text.push_str(&notice.message);
        text.push('\n');
    }
    for event in snapshot.recent_events {
        text.push_str(&event.summary);
        text.push('\n');
        text.push_str(&event.kind);
        text.push('\n');
    }
    for check in core.doctor() {
        text.push_str(&check.detail);
        text.push('\n');
    }
    text
}

fn dev_logs(home: &Path) -> String {
    let dir = home.join("Library/Logs/Navori Dev");
    let mut paths = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with("navori.") && name.ends_with(".log") {
                paths.push(entry.path());
            }
        }
    }
    paths.sort();
    let mut text = String::new();
    for path in paths {
        if let Ok(chunk) = std::fs::read_to_string(&path) {
            text.push_str(&chunk);
        }
    }
    text
}

fn wait_for_stops(home: &Path, stops: usize) -> String {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let text = dev_logs(home);
        if text.matches("core stopped").count() >= stops {
            return text;
        }
        if Instant::now() >= deadline {
            panic!("log never showed {stops} core stopped lines:\n{text}");
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn a_secret_in_a_bad_value_reaches_no_report() {
    let secret = concat!("sk-", "proj-", "BadCfgValue99AaBbCcDdEeFfGgHh");
    let home = home();
    write_config(
        &home,
        &format!("[journal]\nretention_days = \"{secret}\"\n"),
    );
    let core = start(&home);
    let first = surface(&core);
    assert!(
        first.contains("journal.retention_days"),
        "bad value must be named, got {first}"
    );
    assert!(!first.contains(secret), "{first}");
    core.shutdown();

    write_config(&home, &format!("{secret} = [\n"));
    let core = start(&home);
    let second = surface(&core);
    assert!(
        second.contains("is on defaults"),
        "broken file must be named, got {second}"
    );
    assert!(
        second.contains("Not valid TOML"),
        "broken file must say it is not TOML, got {second}"
    );
    assert!(!second.contains(secret), "{second}");
    core.shutdown();

    let log = wait_for_stops(home.path(), 2);
    assert!(log.contains("core stopped"), "{log}");
    assert!(
        log.contains("journal.retention_days"),
        "the log must carry the notice, got {log}"
    );
    assert!(
        log.contains("Not valid TOML"),
        "the log must carry the whole-file notice, got {log}"
    );
    assert!(!log.contains(secret), "{log}");
}

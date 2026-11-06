//! Issue #21: `core.status` must answer while another process holds the store's write lock
//! and a Core write is queued behind it.
#![allow(clippy::unwrap_used)]

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

use navori_ffi::{CoreConfig, CoreSnapshot, NavoriCore, SnapshotListener};
use navori_protocol::rpc::method;

struct Quiet;

impl SnapshotListener for Quiet {
    fn on_snapshot(&self, _snapshot: CoreSnapshot) {}
}

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

/// Another process holds `BEGIN IMMEDIATE` on the database until it is killed. Its stdin is
/// returned too: the transaction stays open only while stdin is open.
fn hold_write_lock(db: &std::path::Path) -> (Child, ChildStdin) {
    let mut child = Command::new("/usr/bin/sqlite3")
        .arg(db)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    writeln!(stdin, ".timeout 2000\nBEGIN IMMEDIATE;\nSELECT 'held';").unwrap();
    let mut line = String::new();
    BufReader::new(child.stdout.take().unwrap())
        .read_line(&mut line)
        .unwrap();
    assert_eq!(line.trim(), "held", "the lock holder did not confirm");
    (child, stdin)
}

fn call(socket: &str, method: &'static str, params: serde_json::Value) -> bool {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async {
        let mut client = navori_ipc::Client::connect(std::path::Path::new(socket))
            .await
            .unwrap();
        client.call(method, params).await.is_ok()
    })
}

#[test]
fn status_answers_while_the_store_is_locked_and_a_core_write_waits() {
    let home = home();
    let core = NavoriCore::start(config(&home), Arc::new(Quiet)).unwrap();
    let status = core.status();
    let socket = status.socket_path.clone();
    let db = std::path::Path::new(&status.data_dir).join("navori.db");

    let (mut holder, holder_stdin) = hold_write_lock(&db);

    // A Core write that has to wait behind the external lock.
    {
        let socket = socket.clone();
        std::thread::spawn(move || {
            call(
                &socket,
                method::DISPATCH,
                serde_json::json!({"module": "awake", "action": {"hold_for": {"seconds": 10}}}),
            )
        });
    }
    std::thread::sleep(Duration::from_millis(300));

    let (tx, rx) = mpsc::channel();
    let started = Instant::now();
    {
        let socket = socket.clone();
        std::thread::spawn(move || {
            let _ = tx.send(call(&socket, method::STATUS, serde_json::Value::Null));
        });
    }
    let answered = rx.recv_timeout(Duration::from_secs(1)).unwrap_or(false);
    let waited = started.elapsed();

    drop(holder_stdin);
    holder.kill().unwrap();
    let _ = holder.wait();
    core.shutdown();

    assert!(
        answered,
        "core.status did not answer within 1 s while the store was locked (waited {waited:?})"
    );
}

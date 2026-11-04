#![allow(clippy::unwrap_used)]

mod common;

use std::io::Write;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use common::{Gate, Rig};
use navori_macos::ProcessSource;
use navori_macos::fake::{FakeProcess, FakeProcesses};
use navori_mod_ports::{HarnessMemo, HarnessName, Scanner};
use serde_json::{Value, json};

const TOKEN_ENTRY: &str = concat!("CLAUDE_CODE_MESSAGING_TOKEN=", "tok_", "abcdefghijklmnop");
const TOKEN_NAME: &str = "CLAUDE_CODE_MESSAGING_TOKEN";
const TOKEN_TEXT: &str = concat!("tok_", "abcdefghijklmnop");

fn listener(pid: i32, exe: &str) -> FakeProcess {
    FakeProcess::new(pid, 1, 501, 10_000 + u64::try_from(pid).unwrap(), exe)
        .listen("127.0.0.1", 3000)
}

struct Captured(Arc<Mutex<Vec<u8>>>);

impl Write for Captured {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn gated() -> (Arc<FakeProcesses>, Arc<Gate>, Scanner) {
    let fake = Arc::new(FakeProcesses::new(501));
    let gate = Arc::new(Gate::new(Arc::clone(&fake) as Arc<dyn ProcessSource>));
    let scanner = Scanner::new(
        Arc::clone(&gate) as Arc<dyn ProcessSource>,
        std::env::temp_dir(),
    );
    (fake, gate, scanner)
}

fn harness(scan: &navori_mod_ports::Scan) -> Vec<Option<HarnessName>> {
    scan.entries.iter().map(|entry| entry.harness).collect()
}

#[tokio::test(start_paused = true)]
async fn a_listener_started_by_claude_code_shows_it_and_leaks_nothing_else() {
    let writer = Arc::new(Mutex::new(Vec::<u8>::new()));
    let captured = Arc::clone(&writer);
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::TRACE)
        .with_ansi(false)
        .with_writer(move || Captured(Arc::clone(&captured)))
        .init();

    let rig = Rig::new();
    rig.processes.add(
        listener(100, "/Users/u/bin/server").env(&[TOKEN_ENTRY, "CLAUDE_CODE_CHILD_SESSION=1"]),
    );
    rig.advance(Duration::from_secs(5)).await;
    rig.settle().await;
    let listed: Value = rig.dispatch(json!("list")).await.unwrap();

    assert_eq!(
        listed["groups"][0]["worktrees"][0]["processes"][0]["entries"][0]["harness"],
        json!("claude_code")
    );
    assert_eq!(
        listed["groups"][0]["worktrees"][0]["processes"][0]["harness"],
        json!("claude_code")
    );
    let state = rig.state().to_string();
    let log = String::from_utf8_lossy(&writer.lock().unwrap_or_else(PoisonError::into_inner))
        .into_owned();
    for text in [listed.to_string(), state, log] {
        assert!(!text.contains(TOKEN_TEXT));
        assert!(!text.contains(TOKEN_NAME));
    }
}

#[test]
fn a_scan_error_and_its_text_name_no_environment_entry() {
    let (fake, gate, scanner) = gated();
    fake.add(listener(100, "/Users/u/bin/server").env(&[TOKEN_ENTRY, "GROK_AGENT=1"]));
    gate.deny(true);

    let (scan, memo) = scanner.scan(HarnessMemo::new());
    let text = format!("{scan:?} {memo:?}");

    assert!(scan.is_err());
    assert!(!text.contains(TOKEN_TEXT));
    assert!(!text.contains(TOKEN_NAME));
}

#[test]
fn a_denied_environment_read_leaves_no_harness_and_the_next_scan_reads_again() {
    let (fake, gate, scanner) = gated();
    fake.add(listener(100, "/Users/u/bin/server").env(&["CODEX_THREAD_ID=abc"]));
    gate.deny_env_once();

    let (first, memo) = scanner.scan(HarnessMemo::new());
    let first = first.unwrap();
    assert_eq!(harness(&first), vec![None]);
    assert_eq!(first.skipped, 0);
    assert!(memo.is_empty());

    let (second, memo) = scanner.scan(memo);
    assert_eq!(harness(&second.unwrap()), vec![Some(HarnessName::Codex)]);
    assert_eq!(memo.len(), 1);
}

#[test]
fn a_completed_read_is_not_repeated_whatever_it_found() {
    let (fake, _gate, scanner) = gated();
    fake.add(listener(100, "/Users/u/bin/a").env(&["GROK_AGENT=1"]));
    fake.add(listener(101, "/Users/u/bin/b").env(&["PATH=/bin"]));

    let (first, memo) = scanner.scan(HarnessMemo::new());
    let (second, memo) = scanner.scan(memo);

    let env_reads: Vec<i32> = fake
        .reads()
        .into_iter()
        .filter(|(_, kind)| *kind == "env")
        .map(|(pid, _)| pid)
        .collect();
    assert_eq!(env_reads, vec![100, 101]);
    assert_eq!(
        harness(&first.unwrap()),
        vec![Some(HarnessName::Grok), None]
    );
    assert_eq!(
        harness(&second.unwrap()),
        vec![Some(HarnessName::Grok), None]
    );
    assert_eq!(memo.len(), 2);
}

#[test]
fn the_memo_keeps_only_the_listeners_of_the_latest_scan() {
    let (fake, _gate, scanner) = gated();
    fake.add(listener(100, "/Users/u/bin/a").env(&["GROK_AGENT=1"]));
    let (_, memo) = scanner.scan(HarnessMemo::new());
    assert_eq!(memo.len(), 1);

    fake.exit(100);
    let (_, memo) = scanner.scan(memo);
    assert!(memo.is_empty());
}

#[test]
fn a_reused_pid_does_not_inherit_the_harness_of_the_old_process() {
    let (fake, _gate, scanner) = gated();
    fake.add(listener(100, "/Users/u/bin/a").env(&["GROK_AGENT=1"]));
    let (_, memo) = scanner.scan(HarnessMemo::new());

    fake.exit(100);
    fake.add(
        FakeProcess::new(100, 1, 501, 99_000, "/Users/u/bin/a")
            .listen("127.0.0.1", 3000)
            .env(&["PATH=/bin"]),
    );
    let (scan, _) = scanner.scan(memo);

    assert_eq!(harness(&scan.unwrap()), vec![None]);
}

#[test]
fn docker_backend_ports_have_no_harness_and_their_environment_is_not_read() {
    let (fake, _gate, scanner) = gated();
    fake.add(
        listener(
            100,
            "/Applications/Docker.app/Contents/MacOS/com.docker.backend",
        )
        .env(&["CLAUDE_CODE_CHILD_SESSION=1"]),
    );

    let (scan, memo) = scanner.scan(HarnessMemo::new());

    assert_eq!(harness(&scan.unwrap()), vec![None]);
    assert!(memo.is_empty());
    assert!(fake.reads().iter().all(|(_, kind)| *kind != "env"));
}

#[test]
fn a_listener_under_a_claude_ancestor_gets_claude_code_without_an_environment_read() {
    let (fake, _gate, scanner) = gated();
    fake.add(FakeProcess::new(50, 1, 501, 1_000, "/usr/local/bin/claude"));
    fake.add(
        FakeProcess::new(100, 50, 501, 10_100, "/Users/u/bin/server").listen("127.0.0.1", 3000),
    );

    let (scan, _) = scanner.scan(HarnessMemo::new());

    assert_eq!(harness(&scan.unwrap()), vec![Some(HarnessName::ClaudeCode)]);
    assert!(fake.reads().iter().all(|(_, kind)| *kind != "env"));
}

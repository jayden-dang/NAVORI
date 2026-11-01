use std::io::Write;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use navori_macos::fake::{FakeExitsHandle, FakeProcess, FakeProcesses};
use navori_macos::{AssertionGuard, PowerError, ProcessSource, WatchError};
use navori_procs::{
    HarnessLookup, NotRunning, ProcId, ProcWatcher, ProcsError, Snapshot, Unavailable, activity,
    harness_of, sessions,
};

const EXE: &str = "/Users/u/bin/node";
const CWD: &str = "/Users/u/secret-project";
const SCRIPT: &str = "/x/@anthropic-ai/claude-code/cli.js";
const TOKEN: &str = concat!("--token=ghp_", "FAKEFAKEFAKEFAKEFAKEFAKEFAKEFAKEFAKE00");
const UID: u32 = 501;

const AGENT: i32 = 10;
const CHILD: i32 = 11;
const DENIED_CWD: i32 = 12;
const REPLACED: i32 = 13;
const EXITED: i32 = 14;
const DENIED_ARGV: i32 = 15;

const NEEDLES: [&str; 3] = ["ghp_", "secret-project", "/Users/u/bin"];

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

fn secret(pid: i32, ppid: i32, start_us: u64) -> FakeProcess {
    FakeProcess::new(pid, ppid, UID, start_us, EXE)
        .argv(&["node", SCRIPT, TOKEN])
        .cwd(CWD)
}

fn note(texts: &mut Vec<String>, err: &(impl std::fmt::Display + std::fmt::Debug)) {
    texts.push(format!("{err}"));
    texts.push(format!("{err:?}"));
}

fn absent(label: &str, text: &str) -> Result<(), String> {
    for needle in NEEDLES {
        if let Some(at) = text.find(needle) {
            let start = at.saturating_sub(48);
            let end = (at + needle.len() + 48).min(text.len());
            return Err(format!("{label} contains {needle}: {}", &text[start..end]));
        }
    }
    Ok(())
}

fn leaks(writer: &Arc<Mutex<Vec<u8>>>, texts: &[String]) -> Result<(), String> {
    let bytes = writer
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone();
    let log = String::from_utf8_lossy(&bytes);
    absent("log", &log)?;
    for text in texts {
        absent("error", text)?;
    }
    Ok(())
}

fn identity(snapshot: &Snapshot, pid: i32) -> Result<ProcId, String> {
    snapshot
        .get(pid)
        .map(|process| process.id)
        .ok_or_else(|| format!("pid {pid} is not in the snapshot"))
}

#[test]
fn argv_environment_and_paths_stay_out_of_errors_and_logs() -> Result<(), String> {
    let writer = Arc::new(Mutex::new(Vec::<u8>::new()));
    let captured = Arc::clone(&writer);
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::TRACE)
        .with_ansi(false)
        .with_writer(move || Captured(Arc::clone(&captured)))
        .init();

    let fake = Arc::new(FakeProcesses::new(UID));
    fake.add(secret(AGENT, 1, 10_000).cpu(Duration::from_millis(1)));
    fake.add(secret(CHILD, AGENT, 11_000).cpu(Duration::from_millis(2)));
    fake.add(FakeProcess::new(DENIED_CWD, 1, UID, 12_000, EXE).argv(&["node", SCRIPT, TOKEN]));
    fake.add(secret(REPLACED, 1, 13_000));
    fake.add(secret(EXITED, 1, 14_000));
    fake.add(FakeProcess::new(DENIED_ARGV, 1, UID, 15_000, EXE));

    let owned: Arc<FakeProcesses> = Arc::clone(&fake);
    let source: Arc<dyn ProcessSource> = owned;
    let mut texts = Vec::new();
    let snapshot = match Snapshot::take(Arc::clone(&source)) {
        Ok(snapshot) => snapshot,
        Err(err) => {
            note(&mut texts, &err);
            return leaks(&writer, &texts);
        }
    };
    let agent = identity(&snapshot, AGENT)?;
    let child = identity(&snapshot, CHILD)?;
    let denied_cwd = identity(&snapshot, DENIED_CWD)?;
    let replaced = identity(&snapshot, REPLACED)?;
    let exited = identity(&snapshot, EXITED)?;
    let denied_argv = identity(&snapshot, DENIED_ARGV)?;

    let found = sessions(&snapshot);
    let mut session_denied = false;
    for session in &found {
        if let Err(err) = &session.location {
            if *err == Unavailable::PermissionDenied {
                session_denied = true;
            }
            note(&mut texts, err);
        }
    }
    if !session_denied {
        leaks(&writer, &texts)?;
        return Err("no session reported a denied working directory".to_owned());
    }
    if !matches!(
        harness_of(&snapshot, agent.pid),
        HarnessLookup::Found { .. }
    ) {
        leaks(&writer, &texts)?;
        return Err("the agent process was not a harness".to_owned());
    }
    let _ = harness_of(&snapshot, child.pid);
    let _ = harness_of(&snapshot, denied_cwd.pid);

    match activity(&snapshot, agent) {
        Ok(children) => {
            if !children.iter().any(|row| row.process.pid == CHILD) {
                leaks(&writer, &texts)?;
                return Err("activity omitted the child".to_owned());
            }
        }
        Err(err) => {
            note(&mut texts, &err);
            leaks(&writer, &texts)?;
            return Err("activity failed while the process was running".to_owned());
        }
    }

    let denied_argv_err = match snapshot.argv(denied_argv) {
        Err(err) => err,
        Ok(_) => {
            leaks(&writer, &texts)?;
            return Err("argv of the denied process was readable".to_owned());
        }
    };
    note(&mut texts, &denied_argv_err);
    if denied_argv_err != Unavailable::PermissionDenied {
        leaks(&writer, &texts)?;
        return Err(format!("argv denial was {denied_argv_err:?}"));
    }

    let denied_cwd_err = match snapshot.cwd(denied_cwd) {
        Err(err) => err,
        Ok(_) => {
            leaks(&writer, &texts)?;
            return Err("working directory of the denied process was readable".to_owned());
        }
    };
    note(&mut texts, &denied_cwd_err);
    if denied_cwd_err != Unavailable::PermissionDenied {
        leaks(&writer, &texts)?;
        return Err(format!("directory denial was {denied_cwd_err:?}"));
    }

    fake.replace(REPLACED, 99_000, UID);
    let replaced_err = match snapshot.argv(replaced) {
        Err(err) => err,
        Ok(_) => {
            leaks(&writer, &texts)?;
            return Err("replaced process still returned arguments".to_owned());
        }
    };
    note(&mut texts, &replaced_err);
    if replaced_err != Unavailable::Exited {
        leaks(&writer, &texts)?;
        return Err(format!("replace was {replaced_err:?}"));
    }

    fake.exit(EXITED);
    let exited_err = match snapshot.cwd(exited) {
        Err(err) => err,
        Ok(_) => {
            leaks(&writer, &texts)?;
            return Err("exited process still returned a working directory".to_owned());
        }
    };
    note(&mut texts, &exited_err);
    if exited_err != Unavailable::Exited {
        leaks(&writer, &texts)?;
        return Err(format!("exit was {exited_err:?}"));
    }

    fake.replace_on_read(AGENT, "argv", 88_000, UID);
    let discarded = match snapshot.argv(agent) {
        Err(err) => err,
        Ok(_) => {
            leaks(&writer, &texts)?;
            return Err(
                "arguments were kept after the process was replaced during the read".to_owned(),
            );
        }
    };
    note(&mut texts, &discarded);
    if discarded != Unavailable::Exited {
        leaks(&writer, &texts)?;
        return Err(format!("replace during read was {discarded:?}"));
    }

    match activity(&snapshot, agent) {
        Err(err) => {
            note(&mut texts, &err);
            if err != NotRunning {
                leaks(&writer, &texts)?;
                return Err(format!("activity after replace was {err:?}"));
            }
        }
        Ok(_) => {
            leaks(&writer, &texts)?;
            return Err("activity succeeded after the process was replaced".to_owned());
        }
    }

    let exits = FakeExitsHandle::new();
    exits.fail_next_watch(AGENT, WatchError::Kqueue { errno: 9 });
    let mut watcher = match ProcWatcher::new(source, exits.factory()) {
        Ok(watcher) => watcher,
        Err(err) => {
            note(&mut texts, &err);
            return leaks(&writer, &texts);
        }
    };
    match watcher.watch(agent) {
        Err(err @ ProcsError::Watch { errno: 9 }) => note(&mut texts, &err),
        Err(err) => {
            note(&mut texts, &err);
            leaks(&writer, &texts)?;
            return Err(format!("kqueue failure was {err:?}"));
        }
        Ok(()) => {
            leaks(&writer, &texts)?;
            return Err("kqueue failure was not returned".to_owned());
        }
    }

    drop(AssertionGuard::new(|| {
        Err(PowerError::ReleaseFailed { code: 1 })
    }));

    let bytes = writer
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone();
    let log = String::from_utf8_lossy(&bytes);
    if !log.contains("power assertion release failed") {
        return Err(format!("tracing capture missed the release warning: {log}"));
    }
    absent("log", &log)?;
    for text in &texts {
        absent("error", text)?;
    }
    Ok(())
}

use std::io::{BufRead, BufReader, Read, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use anyhow::{Context, Result, bail};

#[derive(Debug)]
pub(crate) struct StoreHold {
    child: Child,
    stdin: Option<ChildStdin>,
}

impl StoreHold {
    pub(crate) fn open(db: &Path, confirm_within: Duration) -> Result<Self> {
        let mut child = Command::new("/usr/bin/sqlite3")
            .arg(db)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .with_context(|| format!("spawn sqlite3 to hold {}", db.display()))?;
        let stdin = child.stdin.take();
        let stdout = child.stdout.take();
        let stderr = child.stderr.take();
        let mut hold = Self { child, stdin };
        let stdout = stdout.context("sqlite3 stdout was not piped")?;
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if tx.send(line).is_err() {
                    break;
                }
            }
        });
        hold.send(".bail on\n.timeout 2000\nBEGIN IMMEDIATE;\nSELECT 'held';\n")
            .with_context(|| format!("feed sqlite3 for {}", db.display()))?;
        match rx.recv_timeout(confirm_within) {
            Ok(line) if line == "held" => Ok(hold),
            Ok(line) => bail!(
                "sqlite3 answered {line:?} instead of held for {}; the store may be broken",
                db.display()
            ),
            Err(_) => match hold.child.try_wait() {
                Ok(Some(status)) => {
                    let mut text = String::new();
                    if let Some(mut pipe) = stderr {
                        let _ = pipe.read_to_string(&mut text);
                    }
                    bail!(
                        "sqlite3 exited with {status} before taking the lock on {}: {}",
                        db.display(),
                        text.trim()
                    )
                }
                _ => bail!(
                    "the store lock on {} was not confirmed within {confirm_within:?}; check that the app is not writing the store",
                    db.display()
                ),
            },
        }
    }

    pub(crate) fn commit(mut self) -> Result<()> {
        self.send("COMMIT;\n")?;
        self.stdin = None;
        let status = self.child.wait().context("reap the sqlite3 holder")?;
        if !status.success() {
            bail!("sqlite3 holder exited with {status}; the commit may not have run");
        }
        Ok(())
    }

    fn send(&mut self, text: &str) -> Result<()> {
        let stdin = self.stdin.as_mut().context("sqlite3 stdin is closed")?;
        stdin.write_all(text.as_bytes())?;
        stdin.flush()?;
        Ok(())
    }
}

impl Drop for StoreHold {
    fn drop(&mut self) {
        self.stdin = None;
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use std::process::Command;
    use std::time::Duration;

    fn sqlite(db: &Path, cmds: &[&str]) -> std::process::Output {
        let mut c = Command::new("/usr/bin/sqlite3");
        c.arg(db);
        for cmd in cmds {
            c.args(["-cmd", cmd]);
        }
        c.output().unwrap()
    }

    fn make_db(dir: &Path) -> std::path::PathBuf {
        let db = dir.join("t.db");
        let out = sqlite(&db, &["PRAGMA journal_mode=WAL;", "CREATE TABLE t(x);"]);
        assert!(out.status.success());
        db
    }

    fn insert(db: &Path) -> std::process::Output {
        sqlite(db, &[".timeout 0", "INSERT INTO t VALUES (1);"])
    }

    #[test]
    fn a_store_hold_is_confirmed_and_blocks_another_writer() {
        let dir = tempfile::tempdir_in("/tmp").unwrap();
        let db = make_db(dir.path());
        let hold = StoreHold::open(&db, Duration::from_secs(3)).unwrap();
        let blocked = insert(&db);
        let text = String::from_utf8_lossy(&blocked.stderr).to_string();
        assert!(text.contains("database is locked"), "{text}");
        hold.commit().unwrap();
        assert!(insert(&db).status.success());
    }

    #[test]
    fn a_holder_that_exits_early_reports_its_stderr() {
        let dir = tempfile::tempdir_in("/tmp").unwrap();
        let db = dir.path().join("missing-dir/t.db");
        let err = StoreHold::open(&db, Duration::from_secs(3)).unwrap_err();
        let text = format!("{err:#}");
        assert!(text.contains("sqlite3 exited"), "{text}");
        assert!(text.contains("unable to open"), "{text}");
    }

    #[test]
    fn a_store_hold_that_cannot_take_the_lock_in_time_fails() {
        let dir = tempfile::tempdir_in("/tmp").unwrap();
        let db = make_db(dir.path());
        let first = StoreHold::open(&db, Duration::from_secs(3)).unwrap();
        let err = StoreHold::open(&db, Duration::from_secs(3)).unwrap_err();
        assert!(format!("{err:#}").contains("t.db"), "{err:#}");
        first.commit().unwrap();
    }
}

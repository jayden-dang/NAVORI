use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};

const POLL_STEP: Duration = Duration::from_millis(20);

#[derive(Debug)]
pub(crate) struct Cli {
    helper: PathBuf,
    home: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AwakeCommand {
    CommandLease(u32),
    RenewLease,
    ReleaseAll,
}

impl AwakeCommand {
    fn json(&self) -> String {
        match self {
            Self::CommandLease(pid) => format!(r#"{{"hold_command":{{"pid":{pid}}}}}"#),
            Self::RenewLease => r#"{"hold_for":{"seconds":10}}"#.to_string(),
            Self::ReleaseAll => r#""release_all""#.to_string(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Answer {
    Ok,
    Usage,
    AppDown,
    Failed(i32),
    TimedOut,
}

#[derive(Debug)]
pub(crate) struct Pending {
    child: Child,
    deadline: Instant,
}

impl Cli {
    pub(crate) fn new(app: &Path, home: &Path) -> Self {
        Self {
            helper: app.join("Contents/Helpers/navori"),
            home: home.to_path_buf(),
        }
    }

    fn command(&self) -> Command {
        let mut command = Command::new("/usr/bin/env");
        command
            .arg(format!("HOME={}", self.home.display()))
            .arg(&self.helper)
            .args(["--profile", "dev"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        command
    }

    fn spawn(&self, command: &mut Command) -> Result<Child> {
        command.spawn().with_context(|| {
            format!(
                "spawn {}; build the app so the helper exists",
                self.helper.display()
            )
        })
    }

    pub(crate) fn status(&self, timeout: Duration) -> Result<Answer> {
        let child = self.spawn(self.command().arg("status"))?;
        let mut pending = Pending {
            child,
            deadline: Instant::now() + timeout,
        };
        loop {
            if let Some(answer) = pending.poll(Instant::now())? {
                return Ok(answer);
            }
            std::thread::sleep(POLL_STEP);
        }
    }

    pub(crate) fn dispatch(&self, command: &AwakeCommand, deadline: Duration) -> Result<Pending> {
        let child = self.spawn(self.command().args(["dispatch", "awake", &command.json()]))?;
        Ok(Pending {
            child,
            deadline: Instant::now() + deadline,
        })
    }
}

impl Pending {
    pub(crate) fn poll(&mut self, now: Instant) -> Result<Option<Answer>> {
        if let Some(status) = self.child.try_wait().context("poll the navori helper")? {
            return Ok(Some(match status.code() {
                Some(0) => Answer::Ok,
                Some(2) => Answer::Usage,
                Some(3) => Answer::AppDown,
                Some(code) => Answer::Failed(code),
                None => Answer::Failed(-1),
            }));
        }
        if now >= self.deadline {
            self.child
                .kill()
                .context("kill the overdue navori helper")?;
            self.child.wait().context("reap the killed navori helper")?;
            return Ok(Some(Answer::TimedOut));
        }
        Ok(None)
    }
}

impl Drop for Pending {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::time::{Duration, Instant};

    struct Fixture {
        _dir: tempfile::TempDir,
        app: std::path::PathBuf,
        home: std::path::PathBuf,
    }

    impl Fixture {
        fn new(mode: &str) -> Self {
            let dir = tempfile::tempdir_in("/tmp").unwrap();
            let app = dir.path().join("Fake.app");
            let helpers = app.join("Contents/Helpers");
            fs::create_dir_all(&helpers).unwrap();
            let script = helpers.join("navori");
            fs::write(
                &script,
                "#!/bin/sh\nmode=$(cat \"$HOME/mode\")\necho $$ > \"$HOME/pid\"\ncase \"$mode\" in\n  sleep) exec sleep 5;;\n  one) exit 1;;\n  two) exit 2;;\n  three) exit 3;;\n  *) exit 0;;\nesac\n",
            )
            .unwrap();
            fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
            let home = dir.path().join("home");
            fs::create_dir_all(&home).unwrap();
            let fixture = Self {
                _dir: dir,
                app,
                home,
            };
            fixture.mode(mode);
            fixture
        }

        fn mode(&self, mode: &str) {
            fs::write(self.home.join("mode"), mode).unwrap();
        }

        fn cli(&self) -> Cli {
            Cli::new(&self.app, &self.home)
        }

        fn child_pid(&self) -> i32 {
            fs::read_to_string(self.home.join("pid"))
                .unwrap()
                .trim()
                .parse()
                .unwrap()
        }
    }

    fn alive(pid: i32) -> bool {
        std::process::Command::new("/bin/kill")
            .args(["-0", &pid.to_string()])
            .stderr(std::process::Stdio::null())
            .status()
            .unwrap()
            .success()
    }

    fn poll_until_answered(pending: &mut Pending) -> Answer {
        let start = Instant::now();
        loop {
            if let Some(answer) = pending.poll(Instant::now()).unwrap() {
                return answer;
            }
            assert!(start.elapsed() < Duration::from_secs(5), "never answered");
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    #[test]
    fn a_status_that_does_not_answer_in_two_seconds_times_out_and_the_child_is_killed() {
        let fx = Fixture::new("sleep");
        let start = Instant::now();
        let answer = fx.cli().status(Duration::from_secs(2)).unwrap();
        assert_eq!(answer, Answer::TimedOut);
        assert!(start.elapsed() < Duration::from_millis(3500));
        assert!(!alive(fx.child_pid()));
    }

    #[test]
    fn a_status_that_exits_zero_is_ok() {
        let fx = Fixture::new("zero");
        assert_eq!(fx.cli().status(Duration::from_secs(2)).unwrap(), Answer::Ok);
    }

    #[test]
    fn a_dispatch_returns_at_once_and_is_answered_later() {
        let fx = Fixture::new("zero");
        let start = Instant::now();
        let mut pending = fx
            .cli()
            .dispatch(&AwakeCommand::ReleaseAll, Duration::from_secs(10))
            .unwrap();
        assert!(start.elapsed() < Duration::from_millis(500));
        assert_eq!(poll_until_answered(&mut pending), Answer::Ok);
    }

    #[test]
    fn a_dispatch_still_running_at_its_deadline_is_killed_and_timed_out() {
        let fx = Fixture::new("sleep");
        let mut pending = fx
            .cli()
            .dispatch(&AwakeCommand::RenewLease, Duration::from_millis(300))
            .unwrap();
        std::thread::sleep(Duration::from_millis(500));
        let answer = pending.poll(Instant::now()).unwrap();
        assert_eq!(answer, Some(Answer::TimedOut));
        assert!(!alive(fx.child_pid()));
    }

    #[test]
    fn a_dispatch_that_is_dropped_unanswered_does_not_leave_its_child_running() {
        let fx = Fixture::new("sleep");
        let pending = fx
            .cli()
            .dispatch(&AwakeCommand::CommandLease(7), Duration::from_secs(10))
            .unwrap();
        std::thread::sleep(Duration::from_millis(300));
        let pid = fx.child_pid();
        assert!(alive(pid));
        drop(pending);
        assert!(!alive(pid));
    }

    #[test]
    fn exit_codes_one_two_and_three_are_reported_by_meaning() {
        for (mode, answer) in [
            ("one", Answer::Failed(1)),
            ("two", Answer::Usage),
            ("three", Answer::AppDown),
        ] {
            let fx = Fixture::new(mode);
            assert_eq!(fx.cli().status(Duration::from_secs(2)).unwrap(), answer);
            let mut pending = fx
                .cli()
                .dispatch(&AwakeCommand::ReleaseAll, Duration::from_secs(10))
                .unwrap();
            assert_eq!(poll_until_answered(&mut pending), answer);
        }
    }

    #[test]
    fn awake_commands_serialize_to_the_awake_wire_format() {
        assert_eq!(
            AwakeCommand::CommandLease(42).json(),
            r#"{"hold_command":{"pid":42}}"#
        );
        assert_eq!(
            AwakeCommand::RenewLease.json(),
            r#"{"hold_for":{"seconds":10}}"#
        );
        assert_eq!(AwakeCommand::ReleaseAll.json(), r#""release_all""#);
    }
}

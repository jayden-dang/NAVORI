use std::process::{Child, ExitStatus};

use anyhow::{Context, Result, bail};

use super::cli::Answer;

#[derive(Debug, Default)]
pub(crate) struct Children {
    list: Vec<Child>,
}

impl Children {
    pub(crate) fn push(&mut self, child: Child) {
        self.list.push(child);
    }

    pub(crate) fn reap(&mut self) -> Result<Vec<ExitStatus>> {
        let mut failed = Vec::new();
        let mut first_error = None;
        let mut still = Vec::new();
        for mut child in self.list.drain(..) {
            match child.try_wait() {
                Ok(None) => still.push(child),
                Ok(Some(status)) => {
                    if !status.success() {
                        failed.push(status);
                    }
                }
                Err(err) => {
                    first_error.get_or_insert(err);
                    still.push(child);
                }
            }
        }
        self.list = still;
        match first_error {
            Some(err) => Err(err).context("poll a load child"),
            None => Ok(failed),
        }
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.list.len()
    }
}

impl Drop for Children {
    fn drop(&mut self) {
        for child in &mut self.list {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

pub(crate) fn screen_locked(ioreg: &str) -> bool {
    let squeezed: String = ioreg.split_whitespace().collect();
    squeezed.contains(r#""CGSSessionScreenIsLocked"=Yes"#)
}

pub(crate) fn parse_loadavg(text: &str) -> Option<f64> {
    text.trim()
        .trim_start_matches('{')
        .split_whitespace()
        .next()?
        .parse()
        .ok()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Outcome {
    Fine,
    LoadError,
    LoadTimeout,
}

pub(crate) fn classify(answer: Answer, app_alive: bool) -> Result<Outcome> {
    match answer {
        Answer::Ok => Ok(Outcome::Fine),
        Answer::TimedOut => Ok(Outcome::LoadTimeout),
        Answer::Usage => bail!(
            "the navori helper exited 2 (usage error); the soak sent it a bad command, so fix the soak"
        ),
        Answer::AppDown if app_alive => bail!(
            "the navori helper says the app is not running while its process is alive; check the run's socket and logs"
        ),
        Answer::AppDown => Ok(Outcome::Fine),
        Answer::Failed(_) => Ok(Outcome::LoadError),
    }
}

#[cfg(test)]
mod tests {
    use std::process::{Command, Stdio};
    use std::time::Duration;

    use super::*;

    fn sleeper(secs: &str) -> Child {
        Command::new("/bin/sleep")
            .arg(secs)
            .stdin(Stdio::null())
            .spawn()
            .unwrap()
    }

    fn alive(pid: u32) -> bool {
        Command::new("/bin/kill")
            .args(["-0", &pid.to_string()])
            .stderr(Stdio::null())
            .status()
            .unwrap()
            .success()
    }

    #[test]
    fn reaping_drops_finished_children_and_keeps_running_ones() {
        let mut children = Children::default();
        let quick = sleeper("0");
        let slow = sleeper("30");
        let slow_pid = slow.id();
        children.push(quick);
        children.push(slow);
        std::thread::sleep(Duration::from_millis(300));
        children.reap().unwrap();
        assert_eq!(children.len(), 1);
        assert!(alive(slow_pid));
    }

    #[test]
    fn reaping_reports_a_child_that_exited_non_zero() {
        let mut children = Children::default();
        children.push(
            Command::new("/usr/bin/false")
                .stdin(Stdio::null())
                .spawn()
                .unwrap(),
        );
        children.push(sleeper("0"));
        std::thread::sleep(Duration::from_millis(300));
        let failed = children.reap().unwrap();
        assert_eq!(failed.len(), 1);
        assert_eq!(failed[0].code(), Some(1));
        assert_eq!(children.len(), 0);
    }

    #[test]
    fn dropping_the_guard_kills_every_child() {
        let mut children = Children::default();
        let child = sleeper("30");
        let pid = child.id();
        children.push(child);
        drop(children);
        assert!(!alive(pid));
    }

    #[test]
    fn a_locked_session_is_read_from_the_console_user_record() {
        let locked = r#"    | |   "IOConsoleUsers" = ({"kCGSSessionOnConsoleKey"=Yes,"CGSSessionScreenIsLocked"=Yes,"kCGSSessionUserNameKey"="jayden"})"#;
        let open = r#"    | |   "IOConsoleUsers" = ({"kCGSSessionOnConsoleKey"=Yes,"kCGSSessionUserNameKey"="jayden"})"#;
        let spaced = r#""CGSSessionScreenIsLocked" = Yes"#;
        assert!(screen_locked(locked));
        assert!(!screen_locked(open));
        assert!(screen_locked(spaced));
        assert!(!screen_locked(r#""CGSSessionScreenIsLocked"=No"#));
    }

    #[test]
    fn the_one_minute_load_average_is_the_first_number() {
        assert_eq!(parse_loadavg("{ 2.41 2.80 3.01 }\n"), Some(2.41));
        assert_eq!(parse_loadavg("garbage"), None);
    }

    #[test]
    fn answers_are_classified_and_tool_failures_stop_the_run() {
        assert_eq!(classify(Answer::Ok, true).unwrap(), Outcome::Fine);
        assert_eq!(
            classify(Answer::Failed(1), true).unwrap(),
            Outcome::LoadError
        );
        assert_eq!(
            classify(Answer::TimedOut, true).unwrap(),
            Outcome::LoadTimeout
        );
        assert_eq!(classify(Answer::AppDown, false).unwrap(), Outcome::Fine);
        let usage = classify(Answer::Usage, true).unwrap_err();
        assert!(format!("{usage:#}").contains("usage"), "{usage:#}");
        let gone = classify(Answer::AppDown, true).unwrap_err();
        assert!(format!("{gone:#}").contains("not running"), "{gone:#}");
    }
}

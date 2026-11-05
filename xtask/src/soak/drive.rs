use std::process::Child;

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

    pub(crate) fn reap(&mut self) -> Result<()> {
        let mut still = Vec::new();
        for mut child in self.list.drain(..) {
            if child.try_wait().context("poll a load child")?.is_none() {
                still.push(child);
            }
        }
        self.list = still;
        Ok(())
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

pub(crate) fn tally(
    answer: Answer,
    app_alive: bool,
    errors: &mut u32,
    timeouts: &mut u32,
) -> Result<()> {
    match answer {
        Answer::Ok => {}
        Answer::TimedOut => *timeouts += 1,
        Answer::Failed(2) => bail!(
            "the navori helper exited 2 (usage error); the soak sent it a bad command, so fix the soak"
        ),
        Answer::Failed(3) if app_alive => bail!(
            "the navori helper says the app is not running while its process is alive; check the run's socket and logs"
        ),
        Answer::Failed(3) => {}
        Answer::Failed(_) => *errors += 1,
    }
    Ok(())
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
    fn answers_are_tallied_and_tool_failures_stop_the_run() {
        let (mut errors, mut timeouts) = (0, 0);
        tally(Answer::Ok, true, &mut errors, &mut timeouts).unwrap();
        tally(Answer::Failed(1), true, &mut errors, &mut timeouts).unwrap();
        tally(Answer::TimedOut, true, &mut errors, &mut timeouts).unwrap();
        tally(Answer::Failed(3), false, &mut errors, &mut timeouts).unwrap();
        assert_eq!((errors, timeouts), (1, 1));
        let usage = tally(Answer::Failed(2), true, &mut errors, &mut timeouts).unwrap_err();
        assert!(format!("{usage:#}").contains("usage"), "{usage:#}");
        let gone = tally(Answer::Failed(3), true, &mut errors, &mut timeouts).unwrap_err();
        assert!(format!("{gone:#}").contains("not running"), "{gone:#}");
    }
}

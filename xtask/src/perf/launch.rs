use std::io::Read;
use std::path::Path;
use std::process::{Child, Command, ExitStatus, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail, ensure};

const COMMAND_TIMEOUT: Duration = Duration::from_secs(10);
const REAP_TIMEOUT: Duration = Duration::from_secs(2);
const POLL: Duration = Duration::from_millis(20);

#[derive(Debug)]
pub struct Launched {
    pub pid: u32,
    child: Child,
}

impl Launched {
    pub fn spawn(exe: &Path, args: &[&str], env: &[(&str, &str)]) -> Result<Self> {
        let mut command = Command::new(exe);
        command.args(args).envs(env.iter().copied());
        // Stay in the caller's process group so a terminal SIGINT reaches this
        // process. A new group or setsid would swallow Ctrl-C; Drop does not run then.
        let child = command
            .spawn()
            .with_context(|| format!("spawn {}", exe.display()))?;
        Ok(Self {
            pid: child.id(),
            child,
        })
    }

    pub fn signal(&self, sig: &str) -> Result<()> {
        signal_by_pid(self.pid, sig)
    }

    pub fn wait_exit(&mut self, timeout: Duration) -> Result<bool> {
        let deadline = Instant::now() + timeout;
        loop {
            if self
                .child
                .try_wait()
                .context("wait for launched process")?
                .is_some()
            {
                return Ok(true);
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Ok(false);
            }
            thread::sleep(remaining.min(POLL));
        }
    }
}

impl Drop for Launched {
    fn drop(&mut self) {
        let alive = matches!(self.child.try_wait(), Ok(None));
        if alive {
            let _ = signal_by_pid(self.pid, "KILL");
        }
        reap_child(&mut self.child);
    }
}

fn signal_by_pid(pid: u32, sig: &str) -> Result<()> {
    let flag = format!("-{sig}");
    let pid_text = pid.to_string();
    let rendered = format!("/bin/kill {flag} {pid_text}");
    let out = command_output("/bin/kill", &[&flag, &pid_text])?;
    ensure!(
        out.status.success(),
        "{rendered} failed: {}",
        String::from_utf8_lossy(&out.stderr).trim()
    );
    Ok(())
}

fn command_output(program: &str, args: &[&str]) -> Result<Output> {
    let rendered = format!("{program} {}", args.join(" "));
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("cannot run {rendered}"))?;
    let mut stdout = child
        .stdout
        .take()
        .with_context(|| format!("{rendered} has no stdout"))?;
    let mut stderr = child
        .stderr
        .take()
        .with_context(|| format!("{rendered} has no stderr"))?;
    let stdout_handle = thread::spawn(move || {
        let mut buf = Vec::new();
        stdout.read_to_end(&mut buf).map(|_| buf)
    });
    let stderr_handle = thread::spawn(move || {
        let mut buf = Vec::new();
        stderr.read_to_end(&mut buf).map(|_| buf)
    });
    let status = wait_child(&mut child, &rendered);
    let stdout = join_reader(stdout_handle, "stdout");
    let stderr = join_reader(stderr_handle, "stderr");
    Ok(Output {
        status: status?,
        stdout: stdout?,
        stderr: stderr?,
    })
}

fn wait_child(child: &mut Child, rendered: &str) -> Result<ExitStatus> {
    let started = Instant::now();
    loop {
        if let Some(status) = child
            .try_wait()
            .with_context(|| format!("wait for {rendered}"))?
        {
            return Ok(status);
        }
        if started.elapsed() >= COMMAND_TIMEOUT {
            let _ = child.kill();
            let reap_started = Instant::now();
            loop {
                match child.try_wait() {
                    Ok(Some(_)) => break,
                    Ok(None) if reap_started.elapsed() >= REAP_TIMEOUT => break,
                    Ok(None) => thread::sleep(POLL),
                    Err(err) => {
                        return Err(err).with_context(|| format!("reap {rendered}"));
                    }
                }
            }
            bail!("{rendered} timed out after 10s");
        }
        thread::sleep(POLL);
    }
}

fn reap_child(child: &mut Child) {
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) | Err(_) => return,
            Ok(None) if started.elapsed() >= REAP_TIMEOUT => return,
            Ok(None) => thread::sleep(POLL),
        }
    }
}

fn join_reader(
    handle: thread::JoinHandle<std::io::Result<Vec<u8>>>,
    which: &str,
) -> Result<Vec<u8>> {
    handle
        .join()
        .map_err(|_| anyhow::anyhow!("{which} reader panicked"))?
        .with_context(|| format!("read {which}"))
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::process::{Command, Stdio};
    use std::time::Duration;

    use super::Launched;

    #[test]
    fn wait_exit_times_out_while_the_process_runs() {
        let mut launched = Launched::spawn(Path::new("/bin/sleep"), &["30"], &[]).unwrap();
        let exited = launched.wait_exit(Duration::from_millis(100)).unwrap();
        assert!(!exited);
    }

    #[test]
    fn term_signal_ends_the_process() {
        let mut launched = Launched::spawn(Path::new("/bin/sleep"), &["30"], &[]).unwrap();
        launched.signal("TERM").unwrap();
        let exited = launched.wait_exit(Duration::from_secs(2)).unwrap();
        assert!(exited);
    }

    #[test]
    fn dropping_the_guard_kills_a_live_process() {
        let launched = Launched::spawn(Path::new("/bin/sleep"), &["30"], &[]).unwrap();
        let pid = launched.pid;
        drop(launched);
        let status = Command::new("/bin/kill")
            .args(["-0", &pid.to_string()])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .unwrap();
        assert!(
            !status.success(),
            "pid {pid} still alive after the guard was dropped"
        );
    }
}

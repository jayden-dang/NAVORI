use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail, ensure};
use serde_json::Value;

const COMMAND_TIMEOUT: Duration = Duration::from_secs(10);
const REAP_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Debug, PartialEq, Eq)]
pub struct Sample {
    pub t_ns: u64,
    pub footprint: u64,
    pub peak: u64,
    pub logical_writes: u64,
    pub cpu_ns: u64,
    pub idle_wakeups: u64,
    pub threads: u32,
}

#[derive(Debug)]
pub struct Probe {
    pub bin: PathBuf,
}

pub fn parse_sample(line: &str) -> Result<Sample> {
    let value: Value = serde_json::from_str(line).context("perf sample is not json")?;
    ensure!(value.is_object(), "perf sample is not an object");
    Ok(Sample {
        t_ns: required_u64(&value, "t_ns")?,
        footprint: required_u64(&value, "footprint")?,
        peak: required_u64(&value, "peak")?,
        logical_writes: required_u64(&value, "logical_writes")?,
        cpu_ns: required_u64(&value, "cpu_ns")?,
        idle_wakeups: required_u64(&value, "idle_wakeups")?,
        threads: required_u32(&value, "threads")?,
    })
}

fn required_u64(value: &Value, key: &str) -> Result<u64> {
    let Some(field) = value.get(key) else {
        bail!("perf sample missing {key}");
    };
    field
        .as_u64()
        .with_context(|| format!("perf sample {key} is not a u64"))
}

fn required_u32(value: &Value, key: &str) -> Result<u32> {
    let n = required_u64(value, key)?;
    u32::try_from(n).with_context(|| format!("perf sample {key} does not fit in u32"))
}

impl Probe {
    pub fn compile(root: &Path, out_dir: &Path) -> Result<Self> {
        let source = root.join("xtask/assets/perf-probe.swift");
        let bin = out_dir.join("perf-probe");
        let rendered = format!("swiftc -O {} -o {}", source.display(), bin.display());
        let mut command = Command::new("swiftc");
        command.arg("-O").arg(&source).arg("-o").arg(&bin);
        let out = command_output(&mut command, &rendered)?;
        ensure!(
            out.status.success(),
            "{rendered} failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
        Ok(Self { bin })
    }

    pub fn sample(&self, pid: u32) -> Result<Sample> {
        let rendered = format!("{} {pid}", self.bin.display());
        let mut command = Command::new(&self.bin);
        command.arg(pid.to_string());
        let out = command_output(&mut command, &rendered)?;
        ensure!(
            out.status.success(),
            "{rendered} failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
        let stdout = String::from_utf8(out.stdout)
            .with_context(|| format!("{rendered} stdout is not utf-8"))?;
        parse_sample(stdout.trim()).with_context(|| format!("{rendered} returned {stdout:?}"))
    }
}

fn command_output(command: &mut Command, rendered: &str) -> Result<Output> {
    let mut child = command
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
    let status = wait_child(&mut child, rendered);
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
            if let Err(err) = child.kill()
                && !err.to_string().contains("No such process")
                && !err.to_string().contains("can't kill an exited process")
            {
                eprintln!(
                    "{rendered} timed out and kill failed for pid {}: {err}",
                    child.id()
                );
            }
            let reap_started = Instant::now();
            loop {
                match child.try_wait() {
                    Ok(Some(_)) => break,
                    Ok(None) if reap_started.elapsed() >= REAP_TIMEOUT => break,
                    Ok(None) => thread::sleep(Duration::from_millis(20)),
                    Err(err) => {
                        return Err(err).with_context(|| format!("reap {rendered}"));
                    }
                }
            }
            bail!("{rendered} timed out after 10s");
        }
        thread::sleep(Duration::from_millis(20));
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
    use super::parse_sample;

    #[test]
    fn parses_a_helper_line() {
        let line = r#"{"t_ns":1000,"footprint":2000,"peak":3000,"logical_writes":4000,"cpu_ns":5000,"idle_wakeups":6000,"threads":7}"#;
        assert_eq!(
            parse_sample(line).unwrap(),
            super::Sample {
                t_ns: 1000,
                footprint: 2000,
                peak: 3000,
                logical_writes: 4000,
                cpu_ns: 5000,
                idle_wakeups: 6000,
                threads: 7,
            }
        );
    }

    #[test]
    fn rejects_a_line_missing_a_field() {
        let line = r#"{"t_ns":1000,"footprint":2000,"logical_writes":4000,"cpu_ns":5000,"idle_wakeups":6000,"threads":7}"#;
        assert!(parse_sample(line).is_err());
    }
}

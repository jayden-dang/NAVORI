use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail, ensure};
use serde_json::Value;

use super::judge::Reading;

const COMMAND_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug)]
pub(crate) struct SoakProbe {
    pub bin: PathBuf,
}

pub(crate) fn parse_reading(line: &str, at: Duration) -> Result<Reading> {
    let value: Value = serde_json::from_str(line).context("soak reading is not json")?;
    ensure!(value.is_object(), "soak reading is not an object");
    Ok(Reading {
        at,
        footprint_bytes: required_u64(&value, "footprint")?,
        threads: required_u32(&value, "threads")?,
        fds: required_u32(&value, "fds")?,
    })
}

fn required_u64(value: &Value, key: &str) -> Result<u64> {
    let Some(field) = value.get(key) else {
        bail!("soak reading missing {key}");
    };
    field
        .as_u64()
        .with_context(|| format!("soak reading {key} is not a u64"))
}

fn required_u32(value: &Value, key: &str) -> Result<u32> {
    let n = required_u64(value, key)?;
    u32::try_from(n).with_context(|| format!("soak reading {key} does not fit in u32"))
}

impl SoakProbe {
    pub(crate) fn compile(root: &Path, out_dir: &Path) -> Result<Self> {
        std::fs::create_dir_all(out_dir)
            .with_context(|| format!("create {}", out_dir.display()))?;
        let source = root.join("xtask/assets/soak-probe.swift");
        let bin = out_dir.join("soak-probe");
        let rendered = format!("swiftc -O {} -o {}", source.display(), bin.display());
        let mut command = Command::new("swiftc");
        command.arg("-O").arg(&source).arg("-o").arg(&bin);
        let out = command_output(&mut command, &rendered, Duration::from_secs(300))?;
        ensure!(
            out.status.success(),
            "{rendered} failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
        Ok(Self { bin })
    }

    pub(crate) fn read(&self, pid: u32, at: Duration) -> Result<Reading> {
        let rendered = format!("{} {pid}", self.bin.display());
        let mut command = Command::new(&self.bin);
        command.arg(pid.to_string());
        let out = command_output(&mut command, &rendered, COMMAND_TIMEOUT)?;
        ensure!(
            out.status.success(),
            "{rendered} failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
        let stdout = String::from_utf8(out.stdout)
            .with_context(|| format!("{rendered} stdout is not utf-8"))?;
        parse_reading(stdout.trim(), at).with_context(|| format!("{rendered} returned {stdout:?}"))
    }
}

pub(super) fn command_output(
    command: &mut Command,
    rendered: &str,
    timeout: Duration,
) -> Result<Output> {
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("cannot run {rendered}"))?;
    let mut stdout = child.stdout.take().context("no stdout")?;
    let mut stderr = child.stderr.take().context("no stderr")?;
    let out_reader = thread::spawn(move || {
        let mut buf = Vec::new();
        stdout.read_to_end(&mut buf).map(|_| buf)
    });
    let err_reader = thread::spawn(move || {
        let mut buf = Vec::new();
        stderr.read_to_end(&mut buf).map(|_| buf)
    });
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child
            .try_wait()
            .with_context(|| format!("wait for {rendered}"))?
        {
            break status;
        }
        if started.elapsed() >= timeout {
            child.kill().ok();
            child.wait().ok();
            bail!("{rendered} timed out after {}s", timeout.as_secs());
        }
        thread::sleep(Duration::from_millis(20));
    };
    let stdout = out_reader
        .join()
        .map_err(|_| anyhow::anyhow!("stdout reader panicked"))?
        .context("read stdout")?;
    let stderr = err_reader
        .join()
        .map_err(|_| anyhow::anyhow!("stderr reader panicked"))?
        .context("read stderr")?;
    Ok(Output {
        status,
        stdout,
        stderr,
    })
}

#[cfg(test)]
mod tests {
    use std::process::{Command, Stdio};
    use std::time::Duration;

    use super::{SoakProbe, parse_reading};
    use crate::soak::judge::{RunFacts, judge};

    #[test]
    fn a_probe_line_parses_into_a_reading() {
        let reading = parse_reading(
            r#"{"footprint":31457280,"threads":9,"fds":40}"#,
            Duration::from_secs(5),
        )
        .unwrap();
        assert_eq!(reading.at, Duration::from_secs(5));
        assert_eq!(reading.footprint_bytes, 31_457_280);
        assert_eq!(reading.threads, 9);
        assert_eq!(reading.fds, 40);
    }

    #[test]
    fn a_probe_line_missing_fds_is_an_error_naming_the_field() {
        let err = parse_reading(r#"{"footprint":1,"threads":9}"#, Duration::ZERO).unwrap_err();
        assert!(err.to_string().contains("fds"), "{err}");
    }

    #[test]
    fn a_process_that_opens_a_file_every_tick_is_judged_a_leak() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        let dir = tempfile::tempdir().unwrap();
        let probe = SoakProbe::compile(&root, &dir.path().join("out")).unwrap();
        let mut child = Command::new("/usr/bin/perl")
            .args([
                "-e",
                r#"my @f; for (1..40) { open(my $h, "<", "/dev/null"); push @f, $h; sleep 1 } sleep 5"#,
            ])
            .stdin(Stdio::null())
            .spawn()
            .unwrap();
        std::thread::sleep(Duration::from_secs(2));
        let mut readings = Vec::new();
        for i in 0..12u64 {
            readings.push(probe.read(child.id(), Duration::from_secs(i)).unwrap());
            std::thread::sleep(Duration::from_secs(1));
        }
        child.kill().unwrap();
        child.wait().unwrap();
        assert!(readings.last().unwrap().fds > readings.first().unwrap().fds);
        let report = judge(&RunFacts {
            settle: Duration::ZERO,
            readings,
            hangs: Vec::new(),
            exited_at: None,
            elapsed: Duration::from_secs(12),
            load_errors: 0,
            load_timeouts: 0,
        });
        let lines = report.lines();
        assert!(
            lines
                .iter()
                .any(|l| l.starts_with("soak fds") && l.ends_with("LEAK")),
            "{lines:?}"
        );
    }
}

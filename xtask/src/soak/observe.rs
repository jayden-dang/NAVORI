use std::io::Write;
use std::path::Path;
use std::thread;
use std::time::Duration;

use anyhow::{Context, Result};

use super::judge::{MIB, Reading};
use super::probe::{SoakProbe, tool_stdout_bytes};

const INSTALLED_EXECUTABLE: &str = "/Applications/Navori.app/Contents/MacOS/Navori";

pub(crate) fn find_installed_pid(ps_output: &str) -> Option<u32> {
    ps_output.lines().find_map(|line| {
        let (pid, comm) = line.trim_start().split_once(' ')?;
        (comm.trim() == INSTALLED_EXECUTABLE)
            .then(|| pid.parse().ok())
            .flatten()
    })
}

#[derive(Debug)]
struct NotRunning;

impl std::fmt::Display for NotRunning {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Navori is not running")
    }
}

impl std::error::Error for NotRunning {}

fn is_not_running(err: &anyhow::Error) -> bool {
    err.downcast_ref::<NotRunning>().is_some()
}

fn require_pid(ps_output: &str) -> Result<u32> {
    find_installed_pid(ps_output).ok_or_else(|| NotRunning.into())
}

fn format_line(clock: &str, pid: u32, reading: &Reading) -> String {
    format!(
        "observe {clock} pid {pid} footprint {:.1} threads {} fds {}",
        reading.footprint_bytes as f64 / MIB,
        reading.threads,
        reading.fds
    )
}

fn tool_stdout(program: &str, args: &[&str]) -> Result<String> {
    let rendered = format!("{program} {}", args.join(" "));
    Ok(
        String::from_utf8(tool_stdout_bytes(program, args, &rendered)?)
            .with_context(|| format!("{rendered} output is not utf-8"))?
            .trim_end()
            .to_string(),
    )
}

fn installed_pid() -> Result<u32> {
    require_pid(&tool_stdout("/bin/ps", &["-axo", "pid=,comm="])?)
}

fn observe_once(probe: &SoakProbe) -> Result<()> {
    let pid = installed_pid()?;
    let reading = probe.read(pid, Duration::ZERO)?;
    let clock = tool_stdout("/bin/date", &["+%H:%M:%S"])?;
    let mut out = std::io::stdout().lock();
    writeln!(out, "{}", format_line(&clock, pid, &reading))?;
    out.flush()?;
    Ok(())
}

pub(crate) fn observe(root: &Path, every: Option<Duration>) -> Result<()> {
    let probe = SoakProbe::compile(root, &root.join(super::OUT_DIR))?;
    let Some(interval) = every else {
        return observe_once(&probe);
    };
    loop {
        if let Err(err) = observe_once(&probe) {
            if !is_not_running(&err) {
                return Err(err);
            }
            println!("{err}");
        }
        thread::sleep(interval);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_installed_app_is_found_by_its_exact_executable_path() {
        let ps = "    1 /sbin/launchd\n  812 /Applications/Navori.app/Contents/MacOS/Navori\n  900 /bin/zsh\n";
        assert_eq!(find_installed_pid(ps), Some(812));
    }

    #[test]
    fn the_dev_app_is_never_taken_for_the_installed_app() {
        let ps = "  77 /tmp/x/build/dev/Navori Dev.app/Contents/MacOS/Navori\n  78 /Applications/Navori Dev.app/Contents/MacOS/Navori\n";
        assert_eq!(find_installed_pid(ps), None);
    }

    #[test]
    fn no_navori_process_is_none() {
        assert_eq!(find_installed_pid(""), None);
        assert_eq!(find_installed_pid("  1 /sbin/launchd\n"), None);
    }

    #[test]
    fn a_missing_process_reports_that_navori_is_not_running() {
        let err = require_pid("  1 /sbin/launchd\n").unwrap_err();
        assert_eq!(err.to_string(), "Navori is not running");
        assert!(is_not_running(&err));
    }

    #[test]
    fn a_line_shows_time_pid_footprint_threads_and_fds() {
        let reading = Reading {
            at: Duration::ZERO,
            footprint_bytes: 150 * 1024 * 1024 + 512 * 1024,
            threads: 14,
            fds: 61,
        };
        assert_eq!(
            format_line("09:30:05", 812, &reading),
            "observe 09:30:05 pid 812 footprint 150.5 threads 14 fds 61"
        );
    }
}

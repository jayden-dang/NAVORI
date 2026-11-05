use std::io::Write;
use std::path::Path;
use std::process::Command;
use std::thread;
use std::time::Duration;

use anyhow::{Context, Result, bail, ensure};

use super::judge::Reading;
use super::probe::{SoakProbe, command_output};

const INSTALLED_EXECUTABLE: &str = "/Applications/Navori.app/Contents/MacOS/Navori";
const TOOL_TIMEOUT: Duration = Duration::from_secs(10);
const MIB: f64 = 1024.0 * 1024.0;

pub(crate) fn find_installed_pid(ps_output: &str) -> Option<u32> {
    ps_output.lines().find_map(|line| {
        let (pid, comm) = line.trim_start().split_once(' ')?;
        (comm.trim() == INSTALLED_EXECUTABLE)
            .then(|| pid.parse().ok())
            .flatten()
    })
}

fn require_pid(ps_output: &str) -> Result<u32> {
    match find_installed_pid(ps_output) {
        Some(pid) => Ok(pid),
        None => bail!("Navori is not running"),
    }
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
    let mut command = Command::new(program);
    command.args(args);
    let out = command_output(&mut command, &rendered, TOOL_TIMEOUT)?;
    ensure!(out.status.success(), "{rendered} failed");
    Ok(String::from_utf8(out.stdout)
        .with_context(|| format!("{rendered} output is not utf-8"))?
        .trim_end()
        .to_string())
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
    let probe = SoakProbe::compile(root, &root.join("build/soak"))?;
    observe_once(&probe)?;
    if let Some(interval) = every {
        loop {
            thread::sleep(interval);
            observe_once(&probe)?;
        }
    }
    Ok(())
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

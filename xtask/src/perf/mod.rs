mod baseline;
mod home;
mod judge;
mod launch;
mod logstream;
mod probe;

use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail, ensure};

use baseline::{Baseline, Machine, finish};
use home::{PerfHome, check_socket_len, current_uid};
use judge::{Measurements, judge, median};
use launch::Launched;
use logstream::{LogStream, Mark};
use probe::{Probe, Sample};

const MARK_TIMEOUT: Duration = Duration::from_secs(10);
const SETTLE: Duration = Duration::from_millis(250);
const MEASURED_EXIT: Duration = Duration::from_secs(5);
const WARMUP_EXIT: Duration = Duration::from_secs(15);
const IDLE_SETTLE: Duration = Duration::from_secs(10);
const IDLE_WINDOW: Duration = Duration::from_secs(12);
const COMMAND_TIMEOUT: Duration = Duration::from_secs(10);

pub fn run(root: &Path, record: bool) -> Result<bool> {
    let (app, probe, build_elapsed) = build_bundle(root)?;
    emit(&format!("build {}", format_elapsed(build_elapsed)))?;

    let exe = app.join("Contents/MacOS/Navori");
    let home = prepare_home(&exe)?;
    let mut log = LogStream::start()?;
    warm_up(&home, &exe, &mut log)?;

    let phase_a_started = Instant::now();
    let mut phase_a = Vec::with_capacity(5);
    for n in 1..=5u32 {
        phase_a.push(measure_phase_a(&home, &exe, &probe, &mut log, n)?);
    }
    let phase_a_elapsed = phase_a_started.elapsed();

    let phase_b_started = Instant::now();
    let phase_b = measure_phase_b(&home, &exe, &probe, &mut log)?;
    let phase_b_elapsed = phase_b_started.elapsed();

    let app_size_mb = mib(bundle_bytes(&app)?);
    let measured = measurements(&phase_a, &phase_b, app_size_mb)?;
    let machine = Machine::current()?;
    let baseline_path = root.join("perf-baseline.json");
    let loaded = Baseline::load(&baseline_path)?;
    let verdict = judge(&measured, loaded.as_ref(), &machine);
    report(&verdict, phase_a_elapsed, phase_b_elapsed)?;

    if record && verdict.passed {
        let baseline = Baseline::from_run(&measured, machine, git_revision(root)?, record_date()?);
        finish(&baseline_path, &verdict, record, &baseline)?;
    }
    Ok(verdict.passed)
}

fn build_bundle(root: &Path) -> Result<(PathBuf, Probe, Duration)> {
    let started = Instant::now();
    let out_dir = root.join("build/perf");
    let app = super::bundle(root, super::Profile::Dev, super::Flavor::Release, &out_dir)?;
    let probe = Probe::compile(root, &out_dir)?;
    Ok((app, probe, started.elapsed()))
}

fn prepare_home(exe: &Path) -> Result<PerfHome> {
    let uid = current_uid()?;
    let sweep = PerfHome {
        root: Path::new("/tmp").join(format!("navori-perf-{uid}")),
    };
    // prepare replaces the tree and would delete the pid file the sweep reads.
    let swept = sweep.sweep_stale(exe)?;
    for pid in swept {
        emit(&format!("swept stale pid {pid}"))?;
    }
    let home = PerfHome::prepare(Path::new("/tmp"), uid)?;
    check_socket_len(&home.root.join("run-9"))?;
    Ok(home)
}

fn warm_up(home: &PerfHome, exe: &Path, log: &mut LogStream) -> Result<()> {
    let template = home.template();
    let mut launched = spawn_perf(home, exe, &template)?;
    let pid = launched.pid;
    let drawn = await_mark(log, pid, is_status_item, "statusItemDrawn")?;
    let Mark::StatusItemDrawn { .. } = drawn else {
        bail!("warm-up pid {pid} status wait returned a menu mark");
    };
    let exited = request_exit(&mut launched, WARMUP_EXIT)?;
    ensure!(exited, "warm-up pid {pid} did not exit within 15s");
    absent_marker(&session_marker(&template))
}

fn measure_phase_a(
    home: &PerfHome,
    exe: &Path,
    probe: &Probe,
    log: &mut LogStream,
    n: u32,
) -> Result<PhaseAProcess> {
    let run_dir = home.fresh_run(n)?;
    let mut launched = spawn_perf(home, exe, &run_dir)?;
    let pid = launched.pid;
    let drawn = await_mark(log, pid, is_status_item, "statusItemDrawn")?;
    let Mark::StatusItemDrawn { ms: cold_start_ms } = drawn else {
        bail!("pid {pid} status wait returned a menu mark");
    };
    let first = open_menu(
        probe,
        &launched,
        log,
        is_first_open,
        "menuOpened first=true",
    )?;
    let mut menu_reopen_ms = [0.0; 5];
    let mut menu_open_cpu_ms = Vec::with_capacity(6);
    menu_open_cpu_ms.push(first.cpu_ms);
    for slot in &mut menu_reopen_ms {
        let reopen = open_menu(probe, &launched, log, is_reopen, "menuOpened first=false")?;
        *slot = reopen.ms;
        menu_open_cpu_ms.push(reopen.cpu_ms);
    }
    let mut submenu_opens = [0.0; 5];
    for slot in &mut submenu_opens {
        *slot = open_submenu(&launched, log)?;
    }
    let peak = sample_pid(probe, pid)?;
    finish_measured(&mut launched)?;
    Ok(PhaseAProcess {
        cold_start_ms,
        menu_open_after_launch_ms: first.ms,
        footprint_menu_open_mb: mib(first.footprint_bytes),
        modules: first.modules,
        menu_reopen_ms,
        menu_open_cpu_ms,
        submenu_open_ms: median(&submenu_opens).context("submenu_open_ms has no samples")?,
        peak_bytes: peak.peak,
    })
}

fn measure_phase_b(
    home: &PerfHome,
    exe: &Path,
    probe: &Probe,
    log: &mut LogStream,
) -> Result<PhaseBProcess> {
    let run_dir = home.fresh_run(6)?;
    let mut launched = spawn_perf(home, exe, &run_dir)?;
    let pid = launched.pid;
    let drawn = await_mark(log, pid, is_status_item, "statusItemDrawn")?;
    let Mark::StatusItemDrawn { .. } = drawn else {
        bail!("pid {pid} status wait returned a menu mark");
    };
    thread::sleep(IDLE_SETTLE);
    let mut before = sample_pid(probe, pid)?;
    let mut windows = Vec::with_capacity(5);
    for _ in 0..5 {
        thread::sleep(IDLE_WINDOW);
        let after = sample_pid(probe, pid)?;
        windows.push(idle_window(&before, &after)?);
        before = after;
    }
    let peak_bytes = before.peak;
    finish_measured(&mut launched)?;
    Ok(PhaseBProcess {
        windows,
        peak_bytes,
    })
}

fn measurements(
    phase_a: &[PhaseAProcess],
    phase_b: &PhaseBProcess,
    app_size_mb: f64,
) -> Result<Measurements> {
    let cold_start: Vec<f64> = phase_a.iter().map(|run| run.cold_start_ms).collect();
    let menu_open: Vec<f64> = phase_a
        .iter()
        .map(|run| run.menu_open_after_launch_ms)
        .collect();
    let footprint: Vec<f64> = phase_a
        .iter()
        .map(|run| run.footprint_menu_open_mb)
        .collect();
    let mut reopen = Vec::with_capacity(phase_a.len());
    let mut submenu = Vec::with_capacity(phase_a.len());
    let mut cpu = Vec::new();
    for run in phase_a {
        reopen
            .push(median(run.menu_reopen_ms.as_slice()).context("menu_reopen_ms has no samples")?);
        submenu.push(run.submenu_open_ms);
        cpu.extend(run.menu_open_cpu_ms.iter().copied());
    }
    let mut ranked: Vec<&PhaseAProcess> = phase_a.iter().collect();
    ranked.sort_by(|left, right| {
        left.footprint_menu_open_mb
            .total_cmp(&right.footprint_menu_open_mb)
    });
    let modules = ranked
        .get(ranked.len() / 2)
        .context("phase A produced no process")?
        .modules;

    let idle_cpu: Vec<f64> = phase_b
        .windows
        .iter()
        .map(|window| window.cpu_pct)
        .collect();
    let idle_wakeups: Vec<f64> = phase_b
        .windows
        .iter()
        .map(|window| window.wakeups_per_s)
        .collect();
    let idle_footprint: Vec<f64> = phase_b
        .windows
        .iter()
        .map(|window| window.footprint_mb)
        .collect();
    let idle_writes: Vec<f64> = phase_b
        .windows
        .iter()
        .map(|window| window.disk_writes_kb)
        .collect();
    let threads: Vec<f64> = phase_b
        .windows
        .iter()
        .map(|window| window.threads)
        .collect();

    let mut peak_bytes = phase_b.peak_bytes;
    for run in phase_a {
        peak_bytes = peak_bytes.max(run.peak_bytes);
    }

    let mut out = Measurements::new();
    insert_median(&mut out, "cold_start_ms", &cold_start)?;
    insert_median(&mut out, "menu_open_after_launch_ms", &menu_open)?;
    insert_median(&mut out, "menu_reopen_ms", &reopen)?;
    insert_median(&mut out, "submenu_open_ms", &submenu)?;
    insert_median(&mut out, "footprint_menu_open_mb", &footprint)?;
    insert_median(&mut out, "menu_open_cpu_ms", &cpu)?;
    insert_median(&mut out, "idle_cpu_pct", &idle_cpu)?;
    insert_median(&mut out, "idle_wakeups_per_s", &idle_wakeups)?;
    insert_median(&mut out, "idle_footprint_mb", &idle_footprint)?;
    insert_median(&mut out, "idle_disk_writes_kb", &idle_writes)?;
    insert_median(&mut out, "thread_count", &threads)?;
    out.insert("modules_running".to_owned(), f64::from(modules));
    out.insert("peak_footprint_mb".to_owned(), mib(peak_bytes));
    out.insert("app_size_mb".to_owned(), app_size_mb);
    Ok(out)
}

fn insert_median(out: &mut Measurements, name: &str, values: &[f64]) -> Result<()> {
    let value = median(values).with_context(|| format!("{name} has no samples"))?;
    out.insert(name.to_owned(), value);
    Ok(())
}

struct PhaseAProcess {
    cold_start_ms: f64,
    menu_open_after_launch_ms: f64,
    footprint_menu_open_mb: f64,
    modules: u32,
    menu_reopen_ms: [f64; 5],
    menu_open_cpu_ms: Vec<f64>,
    submenu_open_ms: f64,
    peak_bytes: u64,
}

struct PhaseBWindow {
    cpu_pct: f64,
    wakeups_per_s: f64,
    footprint_mb: f64,
    disk_writes_kb: f64,
    threads: f64,
}

struct PhaseBProcess {
    windows: Vec<PhaseBWindow>,
    peak_bytes: u64,
}

struct MenuOpen {
    ms: f64,
    modules: u32,
    cpu_ms: f64,
    footprint_bytes: u64,
}

fn spawn_perf(home: &PerfHome, exe: &Path, perf_home: &Path) -> Result<Launched> {
    let perf_home = path_str(perf_home)?;
    let launched = Launched::spawn(
        exe,
        &[],
        &[("NAVORI_PERF", "1"), ("NAVORI_PERF_HOME", perf_home)],
    )?;
    home.record_pid(launched.pid)?;
    Ok(launched)
}

fn open_submenu(launched: &Launched, log: &mut LogStream) -> Result<f64> {
    let pid = launched.pid;
    launched.signal("USR2")?;
    let mark = await_mark(log, pid, is_submenu_open, "submenuOpened")?;
    let Mark::SubmenuOpened { ms } = mark else {
        bail!("pid {pid} submenuOpened returned a different mark");
    };
    Ok(ms)
}

fn open_menu(
    probe: &Probe,
    launched: &Launched,
    log: &mut LogStream,
    want: fn(&Mark) -> bool,
    what: &str,
) -> Result<MenuOpen> {
    let pid = launched.pid;
    let before = sample_pid(probe, pid)?;
    launched.signal("USR1")?;
    let mark = await_mark(log, pid, want, what)?;
    thread::sleep(SETTLE);
    let after = sample_pid(probe, pid)?;
    let Mark::MenuOpened { ms, modules, .. } = mark else {
        bail!("pid {pid} {what} returned a status mark");
    };
    Ok(MenuOpen {
        ms,
        modules,
        cpu_ms: cpu_delta_ms(before.cpu_ns, after.cpu_ns)?,
        footprint_bytes: after.footprint,
    })
}

fn idle_window(before: &Sample, after: &Sample) -> Result<PhaseBWindow> {
    let t_ns = after
        .t_ns
        .checked_sub(before.t_ns)
        .context("idle window clock went backwards")?;
    if t_ns == 0 {
        bail!("idle window clock did not advance");
    }
    let cpu_ns = after
        .cpu_ns
        .checked_sub(before.cpu_ns)
        .context("cpu time went backwards during an idle window")?;
    let wakeups = after
        .idle_wakeups
        .checked_sub(before.idle_wakeups)
        .context("idle wakeups went backwards")?;
    let writes = after
        .logical_writes
        .checked_sub(before.logical_writes)
        .context("logical writes went backwards")?;
    let seconds = t_ns as f64 / 1e9;
    Ok(PhaseBWindow {
        cpu_pct: (cpu_ns as f64 / t_ns as f64) * 100.0,
        wakeups_per_s: wakeups as f64 / seconds,
        footprint_mb: mib(after.footprint),
        disk_writes_kb: writes as f64 / 1024.0,
        threads: f64::from(after.threads),
    })
}

fn await_mark(log: &mut LogStream, pid: u32, want: fn(&Mark) -> bool, what: &str) -> Result<Mark> {
    match log.wait_for(pid, want, MARK_TIMEOUT)? {
        Some(mark) => Ok(mark),
        None => bail!("pid {pid} produced no {what} within 10s"),
    }
}

fn finish_measured(launched: &mut Launched) -> Result<()> {
    if request_exit(launched, MEASURED_EXIT)? {
        return Ok(());
    }
    // PERF-3.4: a measured process that ignores SIGTERM is killed. The samples
    // already taken stay. Warm-up does not use this path.
    emit(&format!(
        "pid {} ignored SIGTERM for 5s; sending SIGKILL",
        launched.pid
    ))?;
    match launched.signal("KILL") {
        Ok(()) => Ok(()),
        Err(err) if format!("{err:#}").contains("No such process") => Ok(()),
        Err(err) => Err(err),
    }
}

fn request_exit(launched: &mut Launched, timeout: Duration) -> Result<bool> {
    launched.signal("TERM")?;
    launched.wait_exit(timeout)
}

fn sample_pid(probe: &Probe, pid: u32) -> Result<Sample> {
    probe
        .sample(pid)
        .with_context(|| format!("sample pid {pid}"))
}

fn cpu_delta_ms(before: u64, after: u64) -> Result<f64> {
    let delta = after
        .checked_sub(before)
        .with_context(|| format!("cpu time went backwards: {before} ns then {after} ns"))?;
    Ok(delta as f64 / 1e6)
}

fn is_status_item(mark: &Mark) -> bool {
    matches!(mark, Mark::StatusItemDrawn { .. })
}

fn is_first_open(mark: &Mark) -> bool {
    matches!(mark, Mark::MenuOpened { first: true, .. })
}

fn is_reopen(mark: &Mark) -> bool {
    matches!(mark, Mark::MenuOpened { first: false, .. })
}

fn is_submenu_open(mark: &Mark) -> bool {
    matches!(mark, Mark::SubmenuOpened { .. })
}

fn session_marker(perf_home: &Path) -> PathBuf {
    perf_home
        .join("Library")
        .join("Application Support")
        .join("Navori Dev")
        .join("session.marker")
}

fn absent_marker(marker: &Path) -> Result<()> {
    match fs::symlink_metadata(marker) {
        Ok(_) => bail!("warm-up left {}", marker.display()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err).with_context(|| format!("stat {}", marker.display())),
    }
}

fn bundle_bytes(path: &Path) -> Result<u64> {
    let meta = fs::metadata(path).with_context(|| format!("stat {}", path.display()))?;
    if !meta.is_dir() {
        return Ok(meta.len());
    }
    let mut total = 0u64;
    for entry in fs::read_dir(path).with_context(|| format!("read {}", path.display()))? {
        let entry = entry.with_context(|| format!("read {}", path.display()))?;
        total += bundle_bytes(&entry.path())?;
    }
    Ok(total)
}

fn mib(bytes: u64) -> f64 {
    bytes as f64 / 1_048_576.0
}

fn report(verdict: &judge::Verdict, phase_a: Duration, phase_b: Duration) -> Result<()> {
    for line in &verdict.lines {
        emit(line)?;
    }
    for warning in &verdict.warnings {
        emit(warning)?;
    }
    emit(&format!("phase A {}", format_elapsed(phase_a)))?;
    emit(&format!("phase B {}", format_elapsed(phase_b)))?;
    Ok(())
}

fn format_elapsed(elapsed: Duration) -> String {
    format!("{:.3}s", elapsed.as_secs_f64())
}

fn emit(line: &str) -> Result<()> {
    let mut out = std::io::stdout().lock();
    writeln!(out, "{line}").context("write the perf report")?;
    out.flush().context("flush the perf report")?;
    Ok(())
}

fn git_revision(root: &Path) -> Result<String> {
    let revision = command_stdout("git", &["rev-parse", "HEAD"], Some(root))?;
    ensure!(!revision.is_empty(), "git rev-parse HEAD returned nothing");
    ensure!(
        !revision.contains('\n'),
        "git rev-parse HEAD returned more than one line"
    );
    Ok(revision)
}

fn record_date() -> Result<String> {
    let date = command_stdout("/bin/date", &["+%Y-%m-%d"], None)?;
    let bytes = date.as_bytes();
    ensure!(
        bytes.len() == 10 && bytes.get(4) == Some(&b'-') && bytes.get(7) == Some(&b'-'),
        "record date {date} is not YYYY-MM-DD"
    );
    Ok(date)
}

fn path_str(path: &Path) -> Result<&str> {
    path.to_str()
        .with_context(|| format!("path is not utf-8: {}", path.display()))
}

fn command_stdout(program: &str, args: &[&str], dir: Option<&Path>) -> Result<String> {
    let rendered = format!("{program} {}", args.join(" "));
    let mut command = Command::new(program);
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(dir) = dir {
        command.current_dir(dir);
    }
    let mut child = command
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
    let status = wait_bounded(&mut child, &rendered);
    let stdout = join_pipe(stdout_handle, "stdout");
    let stderr = join_pipe(stderr_handle, "stderr");
    let status = status?;
    let stdout = stdout?;
    let stderr = stderr?;
    ensure!(
        status.success(),
        "{rendered} failed: {}",
        String::from_utf8_lossy(&stderr).trim()
    );
    String::from_utf8(stdout)
        .with_context(|| format!("{rendered} returned non-utf8"))
        .map(|text| text.trim().to_owned())
}

fn wait_bounded(child: &mut std::process::Child, rendered: &str) -> Result<ExitStatus> {
    let started = Instant::now();
    loop {
        if let Some(status) = child
            .try_wait()
            .with_context(|| format!("wait for {rendered}"))?
        {
            return Ok(status);
        }
        if started.elapsed() >= COMMAND_TIMEOUT {
            if let Err(err) = child.kill() {
                let _ = writeln!(
                    std::io::stderr().lock(),
                    "{rendered} timed out and kill failed for pid {}: {err}",
                    child.id()
                );
            }
            let reap_started = Instant::now();
            loop {
                match child.try_wait() {
                    Ok(Some(_)) => break,
                    Ok(None) if reap_started.elapsed() >= Duration::from_secs(2) => {
                        bail!(
                            "{rendered} timed out after 10s and pid {} is still running",
                            child.id()
                        );
                    }
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

fn join_pipe(handle: thread::JoinHandle<std::io::Result<Vec<u8>>>, which: &str) -> Result<Vec<u8>> {
    handle
        .join()
        .map_err(|_| anyhow::anyhow!("{which} reader panicked"))?
        .with_context(|| format!("read {which}"))
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::time::Duration;

    use super::probe::Sample;
    use super::{
        PhaseAProcess, PhaseBProcess, PhaseBWindow, finish_measured, idle_window, measurements,
    };
    use crate::perf::launch::Launched;

    fn process(reopen: [f64; 5], submenu_open_ms: f64) -> PhaseAProcess {
        PhaseAProcess {
            cold_start_ms: 40.0,
            menu_open_after_launch_ms: 20.0,
            footprint_menu_open_mb: 10.0,
            modules: 3,
            menu_reopen_ms: reopen,
            menu_open_cpu_ms: vec![1.0, 3.0],
            peak_bytes: 3 * 1_048_576,
            submenu_open_ms,
        }
    }

    fn phase_b() -> PhaseBProcess {
        PhaseBProcess {
            windows: vec![PhaseBWindow {
                cpu_pct: 0.0,
                wakeups_per_s: 0.0,
                footprint_mb: 8.0,
                disk_writes_kb: 1.0,
                threads: 4.0,
            }],
            peak_bytes: 1_048_576,
        }
    }

    #[test]
    fn reopen_median_is_the_median_of_per_process_medians() {
        let phase_a = [
            process([0.0, 0.0, 0.0, 100.0, 100.0], 4.0),
            process([10.0, 10.0, 10.0, 10.0, 10.0], 12.0),
        ];
        let measured = measurements(&phase_a, &phase_b(), 12.5).unwrap();
        assert_eq!(measured["menu_reopen_ms"], 5.0);
        assert_eq!(measured["cold_start_ms"], 40.0);
        assert_eq!(measured["menu_open_cpu_ms"], 2.0);
        assert_eq!(measured["idle_footprint_mb"], 8.0);
        assert_eq!(measured["peak_footprint_mb"], 3.0);
        assert_eq!(measured["app_size_mb"], 12.5);
        assert_eq!(measured["modules_running"], 3.0);
    }

    #[test]
    fn the_submenu_metric_is_the_median_of_each_process_median() {
        let phase_a = [
            process([0.0, 0.0, 0.0, 100.0, 100.0], 4.0),
            process([10.0, 10.0, 10.0, 10.0, 10.0], 12.0),
        ];
        let measured = measurements(&phase_a, &phase_b(), 12.5).unwrap();
        assert_eq!(measured["submenu_open_ms"], 8.0);
    }

    #[test]
    fn idle_window_uses_nanoseconds_and_kibibytes() {
        let before = Sample {
            t_ns: 1_000_000_000,
            footprint: 0,
            peak: 0,
            logical_writes: 100,
            cpu_ns: 0,
            idle_wakeups: 10,
            threads: 1,
        };
        let after = Sample {
            t_ns: 3_000_000_000,
            footprint: 2 * 1_048_576,
            peak: 9 * 1_048_576,
            logical_writes: 100 + 2048,
            cpu_ns: 500_000_000,
            idle_wakeups: 14,
            threads: 7,
        };
        let window = idle_window(&before, &after).unwrap();
        assert_eq!(window.cpu_pct, 25.0);
        assert_eq!(window.wakeups_per_s, 2.0);
        assert_eq!(window.disk_writes_kb, 2.0);
        assert_eq!(window.footprint_mb, 2.0);
        assert_eq!(window.threads, 7.0);
    }

    #[test]
    fn a_measured_process_that_ignores_sigterm_is_killed() {
        let mut launched = Launched::spawn(
            Path::new("/usr/bin/perl"),
            &["-e", "$SIG{TERM} = 'IGNORE'; sleep 30;"],
            &[],
        )
        .unwrap();
        // The handler is not installed until perl starts. A signal before that
        // uses the default action and kills the process.
        std::thread::sleep(Duration::from_millis(200));
        launched.signal("TERM").unwrap();
        let ignored = launched.wait_exit(Duration::from_millis(200)).unwrap();
        assert!(
            !ignored,
            "SIGTERM ended the process; the ignore did not hold"
        );
        finish_measured(&mut launched).unwrap();
        let gone = launched.wait_exit(Duration::from_secs(2)).unwrap();
        assert!(gone, "pid {} still running", launched.pid);
    }
}

pub(crate) mod cli;
mod drive;
pub(crate) mod hold;
pub(crate) mod home;
pub(crate) mod judge;
mod listen;
pub(crate) mod plan;
pub(crate) mod probe;

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail, ensure};

use crate::perf::launch::Launched;
use crate::perf::logstream::{LogStream, Mark};
use cli::{Answer, Cli, Envelope, Pending};
use drive::{Children, parse_loadavg, screen_locked, tally};
use hold::StoreHold;
use home::SoakHome;
use judge::{Hang, Reading, RunFacts, Side, clock, judge};
use plan::{Event, Tier, plan};
use probe::{SoakProbe, command_output};

pub(crate) use listen::listen;

const DISPATCH_DEADLINE: Duration = Duration::from_secs(10);
const CORE_TIMEOUT: Duration = Duration::from_secs(2);
const MENU_TIMEOUT: Duration = Duration::from_secs(2);
const HOLD_CONFIRM: Duration = Duration::from_secs(3);
const DRAWN_TIMEOUT: Duration = Duration::from_secs(10);
const WARMUP_EXIT: Duration = Duration::from_secs(15);
const STOP_GRACE: Duration = Duration::from_secs(5);
const SLICE: Duration = Duration::from_millis(250);
const TOOL_TIMEOUT: Duration = Duration::from_secs(10);

pub(crate) fn run(root: &Path, tier: Tier) -> Result<bool> {
    let started = Instant::now();
    let out = root.join("build/soak");
    let app = crate::bundle(root, crate::Profile::Dev, crate::Flavor::Release, &out)?;
    drive(root, &app, &tier, started)
}

#[cfg_attr(test, expect(dead_code))]
pub(crate) fn gate(root: &Path, app: &Path) -> Result<bool> {
    ensure!(
        app.exists(),
        "{} is missing; run cargo xtask perf first so the soak has a bundle",
        app.display()
    );
    drive(root, app, &Tier::Short, Instant::now())
}

fn progress(at: Duration, label: &str) {
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "soak +{} {label}", clock(at));
    let _ = out.flush();
}

fn say(line: &str) {
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "{line}");
    let _ = out.flush();
}

fn label(event: &Event) -> &'static str {
    match event {
        Event::CommandLease => "command lease",
        Event::RenewLease => "renew lease",
        Event::ReleaseAll => "release all",
        Event::OpenListener { .. } => "open listener",
        Event::HoldStore { .. } => "hold store",
        Event::ProbeCore => "probe core",
        Event::Sample => "sample",
        Event::ProbeMenu => "probe menu",
    }
}

fn path_str(path: &Path) -> Result<&str> {
    path.to_str()
        .with_context(|| format!("path is not utf-8: {}", path.display()))
}

fn load_average() -> Result<f64> {
    let mut command = Command::new("/usr/sbin/sysctl");
    command.args(["-n", "vm.loadavg"]);
    let out = command_output(&mut command, "sysctl -n vm.loadavg", TOOL_TIMEOUT)?;
    ensure!(out.status.success(), "sysctl -n vm.loadavg failed");
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    parse_loadavg(&text).with_context(|| format!("sysctl -n vm.loadavg returned {text:?}"))
}

fn session_locked() -> Result<bool> {
    let mut command = Command::new("/usr/sbin/ioreg");
    command.args(["-n", "Root", "-d1"]);
    let out = command_output(&mut command, "ioreg -n Root -d1", TOOL_TIMEOUT)?;
    ensure!(out.status.success(), "ioreg -n Root -d1 failed");
    Ok(screen_locked(&String::from_utf8_lossy(&out.stdout)))
}

fn spawn_perf(exe: &Path, perf_home: &Path) -> Result<Launched> {
    Launched::spawn(
        exe,
        &[],
        &[
            ("NAVORI_PERF", "1"),
            ("NAVORI_PERF_HOME", path_str(perf_home)?),
        ],
    )
}

fn await_drawn(log: &mut LogStream, pid: u32) -> Result<()> {
    let mark = log.wait_for(
        pid,
        |m| matches!(m, Mark::StatusItemDrawn { .. }),
        DRAWN_TIMEOUT,
    )?;
    ensure!(
        mark.is_some(),
        "pid {pid} drew no status item within 10s; run cargo xtask perf to check the bundle"
    );
    Ok(())
}

fn warm_up(home: &SoakHome, exe: &Path, log: &mut LogStream) -> Result<()> {
    let mut launched = spawn_perf(exe, &home.template())?;
    let pid = launched.pid;
    home.record_pid(pid)?;
    await_drawn(log, pid)?;
    launched.signal("TERM")?;
    ensure!(
        launched.wait_exit(WARMUP_EXIT)?,
        "warm-up pid {pid} did not exit within 15s"
    );
    Ok(())
}

fn drive(root: &Path, app: &Path, tier: &Tier, started: Instant) -> Result<bool> {
    let out = root.join("build/soak");
    let probe = SoakProbe::compile(root, &out)?;
    let mut children = Children::default();
    children.push(
        Command::new("/usr/bin/caffeinate")
            .args(["-d", "-i", "-w", &std::process::id().to_string()])
            .stdin(Stdio::null())
            .spawn()
            .context("spawn /usr/bin/caffeinate")?,
    );
    let exe = app.join("Contents/MacOS/Navori");
    let own_exe = out.join("Navori Dev.app/Contents/MacOS/Navori");
    let perf_exe = root.join("build/perf/Navori Dev.app/Contents/MacOS/Navori");
    let home = SoakHome::prepare(&[&own_exe, &perf_exe])?;
    let mut log = LogStream::start()?;
    warm_up(&home, &exe, &mut log)?;
    let run_dir = home.fresh_run()?;
    say("docker=not linked");
    let app_proc = spawn_perf(&exe, &run_dir)?;
    let t0 = Instant::now();
    let pid = app_proc.pid;
    home.record_pid(pid)?;
    await_drawn(&mut log, pid)?;
    drop(log);

    let plan = plan(tier);
    let mut driver = Driver {
        cli: Cli::new(app, &run_dir),
        probe: &probe,
        app: app_proc,
        pid,
        t0,
        db: run_dir.join("Library/Application Support/Navori Dev/navori.db"),
        children,
        pending: Vec::new(),
        errors: 0,
        timeouts: 0,
        readings: Vec::new(),
        hangs: Vec::new(),
        exited_at: None,
    };
    for step in &plan.steps {
        driver.wait_until(t0 + step.at)?;
        if driver.exited_at.is_some() {
            break;
        }
        progress(t0.elapsed(), label(&step.event));
        driver.step(&step.event)?;
    }
    driver.stop()?;
    let facts = RunFacts {
        settle: plan.settle,
        readings: driver.readings.clone(),
        hangs: driver.hangs.clone(),
        exited_at: driver.exited_at,
        elapsed: started.elapsed(),
        load_errors: driver.errors,
        load_timeouts: driver.timeouts,
    };
    drop(driver);
    let report = judge(&facts);
    for line in report.lines() {
        say(&line);
    }
    if !report.passed() {
        say(&format!("soak evidence {} pid {pid}", path_str(&run_dir)?));
    }
    Ok(report.exit_ok(tier))
}

#[derive(Debug)]
struct Driver<'a> {
    cli: Cli,
    probe: &'a SoakProbe,
    app: Launched,
    pid: u32,
    t0: Instant,
    db: PathBuf,
    children: Children,
    pending: Vec<Pending>,
    errors: u32,
    timeouts: u32,
    readings: Vec<Reading>,
    hangs: Vec<Hang>,
    exited_at: Option<Duration>,
}

impl Driver<'_> {
    fn tick(&mut self) -> Result<()> {
        self.children.reap()?;
        if self.exited_at.is_none() && self.app.wait_exit(Duration::ZERO)? {
            let at = self.t0.elapsed();
            progress(at, "app exited");
            self.exited_at = Some(at);
        }
        let alive = self.exited_at.is_none();
        let mut i = 0;
        while i < self.pending.len() {
            match self.pending[i].poll(Instant::now())? {
                Some(answer) => {
                    tally(answer, alive, &mut self.errors, &mut self.timeouts)?;
                    self.pending.swap_remove(i);
                }
                None => i += 1,
            }
        }
        Ok(())
    }

    fn wait_until(&mut self, target: Instant) -> Result<()> {
        loop {
            self.tick()?;
            let remaining = target.saturating_duration_since(Instant::now());
            if remaining.is_zero() || self.exited_at.is_some() {
                return Ok(());
            }
            std::thread::sleep(remaining.min(SLICE));
        }
    }

    fn dispatch(&mut self, envelope: &Envelope) -> Result<()> {
        let pending = self.cli.dispatch(envelope, DISPATCH_DEADLINE)?;
        self.pending.push(pending);
        Ok(())
    }

    fn step(&mut self, event: &Event) -> Result<()> {
        match event {
            Event::CommandLease => {
                let child = Command::new("/bin/sleep")
                    .arg("3")
                    .stdin(Stdio::null())
                    .spawn()
                    .context("spawn /bin/sleep for a command lease")?;
                let child_pid = child.id();
                self.children.push(child);
                self.dispatch(&Envelope::CommandLease(child_pid))
            }
            Event::RenewLease => self.dispatch(&Envelope::RenewLease),
            Event::ReleaseAll => self.dispatch(&Envelope::ReleaseAll),
            Event::OpenListener { hold } => {
                let exe = std::env::current_exe().context("locate the xtask binary")?;
                let child = Command::new(&exe)
                    .args(["soak-listen", "--secs", &hold.as_secs().to_string()])
                    .stdin(Stdio::null())
                    .spawn()
                    .with_context(|| format!("spawn {} soak-listen", exe.display()))?;
                self.children.push(child);
                Ok(())
            }
            Event::HoldStore {
                renew_after,
                probe_after,
                commit_after,
            } => {
                let hold = StoreHold::open(&self.db, HOLD_CONFIRM)?;
                let held = Instant::now();
                self.wait_until(held + *renew_after)?;
                self.dispatch(&Envelope::RenewLease)?;
                self.wait_until(held + *probe_after)?;
                self.probe_core()?;
                self.wait_until(held + *commit_after)?;
                hold.commit()
            }
            Event::ProbeCore => self.probe_core(),
            Event::Sample => self.sample(),
            Event::ProbeMenu => self.probe_menu(),
        }
    }

    fn probe_core(&mut self) -> Result<()> {
        let at = self.t0.elapsed();
        let started = Instant::now();
        let answer = self.cli.status(CORE_TIMEOUT)?;
        let alive = self.exited_at.is_none();
        tally(answer, alive, &mut 0, &mut 0)?;
        if answer == Answer::TimedOut {
            self.hangs.push(Hang {
                side: Side::Core,
                at,
                waited: started.elapsed(),
                load_avg: load_average()?,
            });
        }
        Ok(())
    }

    fn sample(&mut self) -> Result<()> {
        let at = self.t0.elapsed();
        match self.probe.read(self.pid, at) {
            Ok(reading) => {
                self.readings.push(reading);
                Ok(())
            }
            Err(err) => {
                if self.app.wait_exit(Duration::ZERO)? {
                    self.exited_at = Some(self.t0.elapsed());
                    Ok(())
                } else {
                    Err(err)
                }
            }
        }
    }

    fn probe_menu(&mut self) -> Result<()> {
        if session_locked()? {
            bail!("the screen is locked; unlock it and rerun");
        }
        let at = self.t0.elapsed();
        let mut log = LogStream::start()?;
        let started = Instant::now();
        self.app.signal("USR1")?;
        let mark = log.wait_for(
            self.pid,
            |m| matches!(m, Mark::MenuOpened { .. }),
            MENU_TIMEOUT,
        )?;
        if mark.is_none() {
            self.hangs.push(Hang {
                side: Side::Menu,
                at,
                waited: started.elapsed(),
                load_avg: load_average()?,
            });
        }
        Ok(())
    }

    fn stop(&mut self) -> Result<()> {
        if self.exited_at.is_some() {
            return Ok(());
        }
        self.app.signal("TERM")?;
        if !self.app.wait_exit(STOP_GRACE)? {
            self.app.signal("KILL")?;
            self.app.wait_exit(STOP_GRACE)?;
        }
        Ok(())
    }
}

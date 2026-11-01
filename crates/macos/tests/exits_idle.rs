//! Idle cost of one blocked exit watcher. Ignored in the normal suite; one test in this process.

use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use navori_macos::{ExitSink, ExitSource, KqueueExits};

struct Sample {
    idle_wakeups: u64,
    cpu_ticks: u64,
}

fn sample() -> Sample {
    let mut info = unsafe {
        // SAFETY: rusage_info_v2 is a C struct of integers and a byte array; zero is a valid value.
        std::mem::zeroed::<libc::rusage_info_v2>()
    };
    let rc = unsafe {
        // SAFETY: info is writable storage for RUSAGE_INFO_V2. The cast matches the C buffer pointer.
        libc::proc_pid_rusage(
            std::process::id() as i32,
            libc::RUSAGE_INFO_V2,
            (&raw mut info).cast(),
        )
    };
    assert_eq!(rc, 0, "proc_pid_rusage failed");
    Sample {
        idle_wakeups: info.ri_pkg_idle_wkups,
        cpu_ticks: info.ri_user_time.saturating_add(info.ri_system_time),
    }
}

#[allow(deprecated)] // libc 0.2.191 deprecates mach_timebase_info; the test converts ticks with it.
fn cpu_nanos(ticks: u64) -> u64 {
    let mut info = libc::mach_timebase_info { numer: 0, denom: 0 };
    let rc = unsafe {
        // SAFETY: info is a live local. mach_timebase_info writes the timebase and does not retain it.
        libc::mach_timebase_info(&raw mut info)
    };
    assert_eq!(rc, 0, "mach_timebase_info failed");
    assert_ne!(info.denom, 0);
    let nanos = u128::from(ticks) * u128::from(info.numer) / u128::from(info.denom);
    u64::try_from(nanos).unwrap_or(u64::MAX)
}

#[test]
#[ignore]
fn an_idle_watcher_causes_no_wakeups_and_no_cpu_time() {
    let mut child = Command::new("sleep")
        .arg("60")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap_or_else(|err| panic!("spawn sleep: {err}"));
    let pid = child.id() as i32;
    let (tx, rx) = mpsc::channel();
    let sink: ExitSink = Box::new(move |pid, token| {
        let _ = tx.send((pid, token));
    });
    let mut exits = KqueueExits::start(sink).unwrap_or_else(|err| panic!("start watcher: {err}"));
    exits
        .watch(pid, 1)
        .unwrap_or_else(|err| panic!("watch {pid}: {err}"));

    let before = sample();
    std::thread::sleep(Duration::from_secs(10));
    let after = sample();
    let wakeups = after.idle_wakeups.saturating_sub(before.idle_wakeups);
    let cpu_ns = cpu_nanos(after.cpu_ticks.saturating_sub(before.cpu_ticks));
    let sink_called = rx.try_recv().is_ok();
    let _ = child.kill();
    let _ = child.wait();
    drop(exits);

    println!("idle wakeups delta {wakeups}, cpu ns {cpu_ns}");
    assert!(!sink_called, "the idle watcher delivered an exit");
    assert_eq!(wakeups, 0, "package idle wakeups increased");
    assert!(cpu_ns < 2_000_000, "process CPU time rose by {cpu_ns} ns");
}

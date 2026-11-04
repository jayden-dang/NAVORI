//! Real process list and per-pid reads against this Mac.

#![allow(clippy::unwrap_used)] // helpers outside #[test] fns

use std::mem::MaybeUninit;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::time::Duration;

use navori_macos::{MacProcesses, Marker, ProcessSource, ReadError, StartTime};

/// A child in its own process group, killed with that group on drop.
struct ChildTree {
    child: Child,
}

impl ChildTree {
    fn spawn(program: &str, args: &[&str], dir: Option<&Path>) -> Self {
        let mut cmd = Command::new(program);
        cmd.args(args);
        if let Some(dir) = dir {
            cmd.current_dir(dir);
        }
        Self::spawn_command(cmd)
    }

    fn spawn_command(mut cmd: Command) -> Self {
        cmd.process_group(0);
        Self {
            child: cmd.spawn().unwrap(),
        }
    }

    fn id(&self) -> i32 {
        self.child.id() as i32
    }
}

impl Drop for ChildTree {
    fn drop(&mut self) {
        let pid = self.child.id() as i32;
        unsafe {
            // SAFETY: this child was started with process_group(0), so its pid is the
            // process-group id. A negative pid signals only that group.
            libc::kill(-pid, libc::SIGKILL);
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn source() -> MacProcesses {
    MacProcesses::new().unwrap()
}

fn wait_for_child(procs: &MacProcesses, parent: i32) -> i32 {
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    loop {
        if let Some(entry) = procs
            .list()
            .unwrap()
            .into_iter()
            .find(|entry| entry.ppid == parent)
        {
            return entry.pid;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "busy child did not appear"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn kernel_bsd(pid: i32) -> Option<libc::proc_bsdinfo> {
    let mut info = MaybeUninit::<libc::proc_bsdinfo>::zeroed();
    let size = std::mem::size_of::<libc::proc_bsdinfo>() as i32;
    let wrote = unsafe {
        // SAFETY: info is writable storage for one proc_bsdinfo, and size is that many bytes.
        libc::proc_pidinfo(
            pid,
            libc::PROC_PIDTBSDINFO,
            0,
            info.as_mut_ptr().cast(),
            size,
        )
    };
    if wrote == size {
        Some(unsafe {
            // SAFETY: proc_pidinfo wrote a full proc_bsdinfo, matching the size we passed.
            info.assume_init()
        })
    } else {
        None
    }
}

#[test]
fn listing_reports_a_spawned_child_with_its_parent_and_owner() {
    let child = ChildTree::spawn("sleep", &["30"], None);
    let procs = source();
    let listed = procs.list().unwrap();
    assert!(
        listed.iter().any(|entry| entry.pid == child.id()),
        "spawned sleep is missing from the process list"
    );
    let entry = procs.entry(child.id()).unwrap();
    assert_eq!(entry.ppid, std::process::id() as i32);
    assert_eq!(entry.ruid, procs.current_uid());
    assert!(!entry.zombie);
}

#[test]
fn listing_matches_the_kernel_view_for_every_owned_process_and_launchd() {
    let procs = source();
    let listed = procs.list().unwrap();
    let launchd = listed.iter().find(|entry| entry.pid == 1).unwrap();
    assert_eq!(launchd.ppid, 0);
    assert_eq!(launchd.ruid, 0);

    let me = std::process::id() as i32;
    let mut checked_self = false;
    for entry in &listed {
        let Some(info) = kernel_bsd(entry.pid) else {
            continue;
        };
        let start = info.pbi_start_tvsec * 1_000_000 + info.pbi_start_tvusec;
        assert_eq!(entry.ppid as u32, info.pbi_ppid, "pid {}", entry.pid);
        assert_eq!(entry.ruid, info.pbi_ruid, "pid {}", entry.pid);
        assert_eq!(entry.start, StartTime(start), "pid {}", entry.pid);
        if entry.pid == me {
            checked_self = true;
        }
    }
    assert!(checked_self, "the test process was not cross-checked");
}

#[test]
fn a_spawned_child_has_its_executable_and_working_directory() {
    let dir = tempfile::tempdir().unwrap();
    let expected = dir.path().canonicalize().unwrap();
    let child = ChildTree::spawn("sleep", &["30"], Some(dir.path()));
    let procs = source();
    assert_eq!(procs.exe(child.id()).unwrap(), PathBuf::from("/bin/sleep"));
    assert_eq!(procs.cwd(child.id()).unwrap(), expected);
}

#[test]
#[ignore = "reads a real process's removed working directory"]
fn a_removed_working_directory_is_still_read_as_its_old_path() {
    let dir = tempfile::tempdir().unwrap();
    let expected = dir.path().canonicalize().unwrap();
    let child = ChildTree::spawn("sleep", &["5"], Some(&expected));
    let procs = source();
    assert_eq!(procs.cwd(child.id()).unwrap(), expected);
    std::fs::remove_dir(&expected).unwrap();
    assert_eq!(procs.cwd(child.id()).unwrap(), expected);
}

#[test]
fn cpu_time_of_a_busy_child_is_close_to_its_wall_time() {
    let shell = ChildTree::spawn("sh", &["-c", "yes > /dev/null"], None);
    let procs = source();
    // sh only waits. The CPU time belongs to its yes child; the shell's own
    // rusage stays a few milliseconds, and child rusage stays 0 until wait.
    let busy = wait_for_child(&procs, shell.id());
    assert_eq!(procs.exe(busy).unwrap(), PathBuf::from("/usr/bin/yes"));
    let warm = std::time::Instant::now() + Duration::from_secs(2);
    while procs.cpu_time(busy).unwrap() < Duration::from_millis(1) {
        assert!(
            std::time::Instant::now() < warm,
            "yes did not start running"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    let started = std::time::Instant::now();
    let baseline = procs.cpu_time(busy).unwrap();
    std::thread::sleep(Duration::from_secs(1));
    let cpu = procs.cpu_time(busy).unwrap().saturating_sub(baseline);
    assert!(
        started.elapsed() < Duration::from_millis(1500),
        "the one-second sample ran long"
    );
    assert!(
        (Duration::from_millis(500)..=Duration::from_millis(1500)).contains(&cpu),
        "cpu_time {cpu:?} is outside 0.5–1.5 s"
    );
}

#[test]
fn reading_a_pid_that_does_not_exist_reports_gone() {
    let procs = source();
    let missing = 999_999;
    assert_eq!(procs.entry(missing).unwrap_err(), ReadError::Gone);
    assert_eq!(procs.exe(missing).unwrap_err(), ReadError::Gone);
    assert_eq!(procs.cwd(missing).unwrap_err(), ReadError::Gone);
}

#[test]
fn reading_the_working_directory_of_launchd_is_denied() {
    let procs = source();
    assert_eq!(procs.cwd(1).unwrap_err(), ReadError::Denied);
}

#[test]
fn reading_arguments_of_a_missing_process_reports_gone() {
    let procs = source();
    assert_eq!(procs.argv(999_999).unwrap_err(), ReadError::Gone);
}

#[test]
fn reading_arguments_of_launchd_is_denied() {
    let procs = source();
    assert_eq!(procs.argv(1).unwrap_err(), ReadError::Denied);
}

#[test]
fn a_spawned_child_reports_its_arguments() {
    let child = ChildTree::spawn("sleep", &["30"], None);
    let procs = source();
    assert_eq!(procs.argv(child.id()).unwrap(), vec!["sleep", "30"]);
}

#[test]
fn the_test_process_own_arguments_exclude_its_environment() {
    let procs = source();
    assert_eq!(
        procs.argv(std::process::id() as i32).unwrap(),
        std::env::args().collect::<Vec<_>>()
    );
}

const MARKER_NAMES: [&str; 3] = ["CLAUDE_CODE_CHILD_SESSION", "CODEX_THREAD_ID", "GROK_AGENT"];

/// The kernel withholds the environment of a platform binary, so the child is an ad-hoc signed copy.
fn unrestricted_copy(dir: &Path, tool: &str) -> PathBuf {
    let copy = dir.join(tool);
    std::fs::copy(Path::new("/bin").join(tool), &copy).unwrap();
    let signed = Command::new("/usr/bin/codesign")
        .args(["-s", "-", "--force"])
        .arg(&copy)
        .output()
        .unwrap();
    assert!(signed.status.success(), "codesign refused the copy");
    copy
}

#[test]
#[ignore = "reads the environment of a real child"]
fn an_environment_marker_is_found_in_a_real_process() {
    let dir = tempfile::tempdir().unwrap();
    let sleep = unrestricted_copy(dir.path(), "sleep");
    let mut cmd = Command::new(sleep);
    cmd.env_clear().env("GROK_AGENT", "1").args(["5"]);
    let child = ChildTree::spawn_command(cmd);
    let procs = source();
    assert_eq!(
        procs.env_marker(child.id(), &MARKER_NAMES).unwrap(),
        Marker::One(2)
    );
}

#[test]
#[ignore = "reads the environment of a real child"]
fn a_marker_given_only_as_an_argument_is_not_found() {
    let dir = tempfile::tempdir().unwrap();
    let sh = unrestricted_copy(dir.path(), "sh");
    let mut cmd = Command::new(sh);
    cmd.env_clear()
        .args(["-c", "sleep 5; :", "sh", "GROK_AGENT=1"]);
    let child = ChildTree::spawn_command(cmd);
    let procs = source();
    assert_eq!(
        procs.env_marker(child.id(), &MARKER_NAMES).unwrap(),
        Marker::None
    );
}

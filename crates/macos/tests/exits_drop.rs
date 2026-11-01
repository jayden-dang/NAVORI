//! File descriptors and threads owned by one watcher. One test, so nothing else shares this process.

use std::sync::mpsc;
use std::time::{Duration, Instant};

use navori_macos::{ExitSink, KqueueExits};

fn fd_count() -> usize {
    std::fs::read_dir("/dev/fd")
        .unwrap_or_else(|err| panic!("read /dev/fd: {err}"))
        .count()
}

fn thread_count() -> i32 {
    let mut info = unsafe {
        // SAFETY: proc_taskinfo is a C struct of integers; zero is a valid value.
        std::mem::zeroed::<libc::proc_taskinfo>()
    };
    let size = std::mem::size_of::<libc::proc_taskinfo>() as i32;
    let wrote = unsafe {
        // SAFETY: info is writable for `size` bytes. PROC_PIDTASKINFO fills one proc_taskinfo
        // for this process and does not retain the buffer.
        libc::proc_pidinfo(
            std::process::id() as i32,
            libc::PROC_PIDTASKINFO,
            0,
            (&raw mut info).cast(),
            size,
        )
    };
    assert_eq!(wrote, size, "proc_pidinfo returned {wrote}");
    info.pti_threadnum
}

fn wait_until(mut pred: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < deadline {
        if pred() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    pred()
}

#[test]
fn dropping_the_watcher_closes_its_kqueue_and_ends_its_thread() {
    let fds_before = fd_count();
    let threads_before = thread_count();
    let (tx, _rx) = mpsc::channel();
    let sink: ExitSink = Box::new(move |pid, token| {
        let _ = tx.send((pid, token));
    });
    let exits = KqueueExits::start(sink).unwrap_or_else(|err| panic!("start watcher: {err}"));
    assert!(
        wait_until(|| thread_count() == threads_before + 1),
        "watcher thread did not appear (before {threads_before}, now {})",
        thread_count()
    );
    drop(exits);
    assert!(
        wait_until(|| thread_count() == threads_before),
        "watcher thread still alive (before {threads_before}, now {})",
        thread_count()
    );
    assert_eq!(fd_count(), fds_before);
    assert_eq!(thread_count(), threads_before);
}

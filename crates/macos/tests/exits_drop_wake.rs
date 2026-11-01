//! Drop when the wake event is already gone. One test, so nothing else shares this process.

use std::collections::HashSet;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use navori_macos::{ExitSink, KqueueExits};

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

fn fd_set() -> HashSet<i32> {
    let names: Vec<i32> = std::fs::read_dir("/dev/fd")
        .unwrap_or_else(|err| panic!("read /dev/fd: {err}"))
        .filter_map(|entry| entry.ok()?.file_name().to_str()?.parse::<i32>().ok())
        .collect();
    names
        .into_iter()
        .filter(|fd| {
            // SAFETY: `fd` is a descriptor number from /dev/fd. F_GETFD takes no buffer.
            let flags = unsafe { libc::fcntl(*fd, libc::F_GETFD) };
            flags >= 0
        })
        .collect()
}

fn delete_user_filter(kq: i32) {
    let change = libc::kevent {
        ident: 0,
        filter: libc::EVFILT_USER,
        flags: libc::EV_DELETE,
        fflags: 0,
        data: 0,
        udata: std::ptr::null_mut(),
    };
    // SAFETY: `change` is one live kevent. The event list is null because this call only deletes.
    let n = unsafe { libc::kevent(kq, &change, 1, std::ptr::null_mut(), 0, std::ptr::null()) };
    assert!(
        n >= 0,
        "removing the wake event failed: errno {}",
        std::io::Error::last_os_error()
    );
}

#[test]
fn dropping_the_watcher_ends_its_thread_when_the_wake_event_is_gone() {
    let threads_before = thread_count();
    let before = fd_set();
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
    let added: Vec<i32> = fd_set().difference(&before).copied().collect();
    assert_eq!(added.len(), 1, "expected one new descriptor, saw {added:?}");
    delete_user_filter(added[0]);

    let (done_tx, done_rx) = mpsc::channel();
    std::thread::spawn(move || {
        drop(exits);
        let _ = done_tx.send(());
    });
    assert!(
        done_rx.recv_timeout(Duration::from_secs(2)).is_ok(),
        "drop did not return; the watcher thread is still blocked in kevent"
    );
    assert!(
        wait_until(|| thread_count() == threads_before),
        "watcher thread still alive (before {threads_before}, now {})",
        thread_count()
    );
}

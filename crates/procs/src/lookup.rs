//! Which harness owns a process.

use crate::harness::{Harness, recognize};
use crate::snapshot::{ProcId, Snapshot};

/// The harness behind a process, or why none was found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HarnessLookup {
    /// `process` is a process of `harness`: the named process, or its nearest matching ancestor.
    Found {
        /// The recognized harness.
        harness: Harness,
        /// Identity of the harness process.
        process: ProcId,
    },
    /// The process is running, and neither it nor an ancestor is a recognized harness.
    NotAHarness,
    /// The pid is not in the snapshot, or the live entry is no longer that process.
    NotRunning,
}

/// Nearest recognized harness for `pid`: the process itself, then ancestors, nearest first.
///
/// [`HarnessLookup::NotRunning`] when `pid` is not in `snapshot`, or the live entry does not
/// still have that start time and the caller's real uid. A pid that now belongs to another
/// user is not running. A running process with no harness ancestor is
/// [`HarnessLookup::NotAHarness`].
pub fn harness_of(snapshot: &Snapshot, pid: i32) -> HarnessLookup {
    let Some(process) = snapshot.get(pid) else {
        return HarnessLookup::NotRunning;
    };
    let id = process.id;
    if !same_live_process(snapshot, id) {
        return HarnessLookup::NotRunning;
    }
    if let Some(harness) = recognize(snapshot, process) {
        return HarnessLookup::Found {
            harness,
            process: id,
        };
    }
    for ancestor in snapshot.ancestors(id) {
        if let Some(harness) = recognize(snapshot, ancestor) {
            return HarnessLookup::Found {
                harness,
                process: ancestor.id,
            };
        }
    }
    HarnessLookup::NotAHarness
}

fn same_live_process(snapshot: &Snapshot, id: ProcId) -> bool {
    let source = snapshot.source();
    matches!(
        source.entry(id.pid),
        Ok(entry) if entry.start == id.start && entry.ruid == source.current_uid()
    )
}

//! What a process's children are doing.

use std::time::Duration;

use crate::error::{NotRunning, Unavailable};
use crate::snapshot::{ProcId, Snapshot};

/// One live descendant and the CPU time it has used.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChildActivity {
    pub process: ProcId,
    pub cpu_time: Duration,
}

/// Live descendants of `id`, nearest first, each with its cumulative CPU time.
///
/// The named process is not included. A descendant is left out when the verified
/// CPU read fails: it exited or was replaced, or the CPU time could not be read.
/// There is no per-child error on this list.
///
/// # Errors
///
/// [`NotRunning`] when `id` is not in this snapshot, or the live entry no longer
/// has that start time and the caller's real uid.
pub fn activity(snapshot: &Snapshot, id: ProcId) -> Result<Vec<ChildActivity>, NotRunning> {
    if !root_still_running(snapshot, id) {
        return Err(NotRunning);
    }
    let mut listed = Vec::new();
    for descendant in snapshot.descendants(id) {
        match snapshot.read_verified(descendant.id, |source, pid| source.cpu_time(pid)) {
            Ok(cpu_time) => listed.push(ChildActivity {
                process: descendant.id,
                cpu_time,
            }),
            Err(Unavailable::Exited | Unavailable::PermissionDenied | Unavailable::Malformed) => {}
        }
    }
    Ok(listed)
}

/// The snapshot still contains `id`, and the live entry is still that process.
fn root_still_running(snapshot: &Snapshot, id: ProcId) -> bool {
    if snapshot.get(id.pid).is_none_or(|process| process.id != id) {
        return false;
    }
    let source = snapshot.source();
    matches!(
        source.entry(id.pid),
        Ok(entry) if entry.start == id.start && entry.ruid == source.current_uid()
    )
}

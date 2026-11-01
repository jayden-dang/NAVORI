//! One user's processes at one moment.

use std::collections::{HashMap, VecDeque};
use std::fmt;
use std::path::PathBuf;
use std::sync::Arc;

use navori_macos::{PlatformError, ProcessSource, ReadError, StartTime};

use crate::error::{Field, ProcsError, Unavailable};

/// Identity of one process. The same pid at a later start time is a different identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ProcId {
    pub pid: i32,
    pub start: StartTime,
}

/// One process in a snapshot: parent pid, start time, and executable when it could be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Process {
    pub id: ProcId,
    pub parent: Option<i32>,
    pub exe: Field<PathBuf>,
}

/// The user's processes at one moment, plus later reads checked against that identity.
pub struct Snapshot {
    source: Arc<dyn ProcessSource>,
    processes: Vec<Process>,
    by_pid: HashMap<i32, usize>,
    children: HashMap<i32, Vec<usize>>,
}

impl fmt::Debug for Snapshot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Snapshot")
            .field("processes", &self.processes)
            .finish_non_exhaustive()
    }
}

impl Snapshot {
    /// Processes owned by the source's real uid, excluding zombies and pids that vanished
    /// or were replaced while their executable was read.
    ///
    /// # Errors
    ///
    /// [`ProcsError::Platform`] when the process list itself cannot be read. One process
    /// exiting or refusing a field does not fail the snapshot.
    pub fn take(source: Arc<dyn ProcessSource>) -> Result<Self, ProcsError> {
        let uid = source.current_uid();
        let first = source.list().map_err(platform)?;
        let mut processes = Vec::new();
        for entry in first {
            if entry.ruid != uid || entry.zombie {
                continue;
            }
            let exe = match source.exe(entry.pid) {
                Ok(path) => Ok(path),
                Err(ReadError::Gone) => continue,
                Err(err) => Err(unavailable_from(err)),
            };
            // `entry` is the first list. A replacement during `exe` must not keep that path.
            processes.push(Process {
                id: ProcId {
                    pid: entry.pid,
                    start: entry.start,
                },
                parent: Some(entry.ppid),
                exe,
            });
        }
        let second = source.list().map_err(platform)?;
        let live: HashMap<i32, StartTime> = second
            .into_iter()
            .map(|entry| (entry.pid, entry.start))
            .collect();
        processes.retain(|process| live.get(&process.id.pid) == Some(&process.id.start));
        let (by_pid, children) = index(&processes);
        Ok(Self {
            source,
            processes,
            by_pid,
            children,
        })
    }

    /// Every process kept in this snapshot, in list order.
    pub fn processes(&self) -> impl Iterator<Item = &Process> {
        self.processes.iter()
    }

    /// The process with this pid, if the snapshot kept one.
    pub fn get(&self, pid: i32) -> Option<&Process> {
        self.by_pid.get(&pid).map(|&index| &self.processes[index])
    }

    /// Ancestors that are in this snapshot, nearest first. Stops at a parent the snapshot
    /// does not contain.
    pub fn ancestors(&self, id: ProcId) -> Vec<&Process> {
        let Some(mut index) = self.index_of(id) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        let mut seen = vec![false; self.processes.len()];
        seen[index] = true;
        while let Some(parent_pid) = self.processes[index].parent {
            let Some(&parent) = self.by_pid.get(&parent_pid) else {
                break;
            };
            if seen[parent] {
                break;
            }
            seen[parent] = true;
            out.push(&self.processes[parent]);
            index = parent;
        }
        out
    }

    /// Descendants in this snapshot, nearest first (breadth-first).
    pub fn descendants(&self, id: ProcId) -> Vec<&Process> {
        let Some(start) = self.index_of(id) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        let mut queue = VecDeque::from([start]);
        let mut seen = vec![false; self.processes.len()];
        seen[start] = true;
        while let Some(index) = queue.pop_front() {
            let Some(kids) = self.children.get(&self.processes[index].id.pid) else {
                continue;
            };
            for &child in kids {
                if seen[child] {
                    continue;
                }
                seen[child] = true;
                out.push(&self.processes[child]);
                queue.push_back(child);
            }
        }
        out
    }

    /// Arguments of this identity, read only after the live process still matches it.
    pub fn argv(&self, id: ProcId) -> Field<Vec<String>> {
        self.read_verified(id, |source, pid| source.argv(pid))
    }

    /// Working directory of this identity, read only after the live process still matches it.
    pub fn cwd(&self, id: ProcId) -> Field<PathBuf> {
        self.read_verified(id, |source, pid| source.cwd(pid))
    }

    /// Live entry has this start time and the caller's real uid, then `read`, then the live
    /// entry still has this start time. A mismatch reads nothing, or discards what was read.
    pub(crate) fn read_verified<T>(
        &self,
        id: ProcId,
        read: impl FnOnce(&dyn ProcessSource, i32) -> Result<T, ReadError>,
    ) -> Field<T> {
        if self.index_of(id).is_none() {
            return Err(Unavailable::Exited);
        }
        let source = self.source();
        match source.entry(id.pid) {
            Ok(entry) if entry.start == id.start && entry.ruid == source.current_uid() => {}
            Ok(_) | Err(ReadError::Gone) => return Err(Unavailable::Exited),
            Err(err) => return Err(unavailable_from(err)),
        }
        let value = match read(source, id.pid) {
            Ok(value) => value,
            Err(err) => return Err(unavailable_from(err)),
        };
        match source.entry(id.pid) {
            Ok(entry) if entry.start == id.start => Ok(value),
            _ => Err(Unavailable::Exited),
        }
    }

    /// The process source this snapshot was taken from.
    pub(crate) fn source(&self) -> &dyn ProcessSource {
        self.source.as_ref()
    }

    fn index_of(&self, id: ProcId) -> Option<usize> {
        let index = *self.by_pid.get(&id.pid)?;
        (self.processes[index].id == id).then_some(index)
    }
}

fn platform(source: PlatformError) -> ProcsError {
    ProcsError::Platform { source }
}

fn unavailable_from(err: ReadError) -> Unavailable {
    match err {
        ReadError::Gone => Unavailable::Exited,
        ReadError::Denied => Unavailable::PermissionDenied,
        ReadError::Malformed => Unavailable::Malformed,
    }
}

fn index(processes: &[Process]) -> (HashMap<i32, usize>, HashMap<i32, Vec<usize>>) {
    let mut by_pid = HashMap::new();
    let mut children: HashMap<i32, Vec<usize>> = HashMap::new();
    for (index, process) in processes.iter().enumerate() {
        by_pid.entry(process.id.pid).or_insert(index);
        if let Some(parent) = process.parent {
            children.entry(parent).or_default().push(index);
        }
    }
    (by_pid, children)
}

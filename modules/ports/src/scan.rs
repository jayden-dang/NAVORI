use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::net::IpAddr;
use std::path::PathBuf;
use std::sync::Arc;

use navori_macos::ProcessSource;
use navori_procs::{Harness, ProcId, ProcsError, Snapshot, started_by};

use crate::kind::{is_docker_backend, kind};
use crate::place::place_listener;
use crate::state::{HarnessName, PortEntry};

/// A listener of the Docker backend, which a later stage attributes to a container.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackendPort {
    pub pid: i32,
    pub start_us: u64,
    pub port: u16,
    pub addresses: Vec<IpAddr>,
    pub exe: Option<PathBuf>,
}

/// The Harness that started each listener whose environment was read to the end, by process identity.
/// A read that failed has no entry, so the next scan reads again.
pub type HarnessMemo = HashMap<ProcId, Option<Harness>>;

/// What one scan found.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Scan {
    pub entries: Vec<PortEntry>,
    pub skipped: u32,
    pub backend: Vec<BackendPort>,
}

/// Reads the user's listening ports from a process source.
pub struct Scanner {
    source: Arc<dyn ProcessSource>,
    home: PathBuf,
}

impl std::fmt::Debug for Scanner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Scanner").finish_non_exhaustive()
    }
}

impl Scanner {
    pub fn new(source: Arc<dyn ProcessSource>, home: PathBuf) -> Self {
        Self { source, home }
    }

    /// Blocking: one process list, then each process's sockets. `memo` goes in and comes back holding
    /// only the listeners of this scan.
    ///
    /// # Errors
    ///
    /// [`ProcsError::Platform`] when the process list cannot be read. The memo comes back either way.
    pub fn scan(&self, memo: HarnessMemo) -> (Result<Scan, ProcsError>, HarnessMemo) {
        let mut kept = HarnessMemo::new();
        match self.scan_with(&memo, &mut kept) {
            Ok(scan) => (Ok(scan), kept),
            Err(err) => (Err(err), memo),
        }
    }

    fn scan_with(&self, memo: &HarnessMemo, kept: &mut HarnessMemo) -> Result<Scan, ProcsError> {
        let snapshot = Snapshot::take(Arc::clone(&self.source))?;
        let mut found = Vec::new();
        let mut unread = Vec::new();
        for process in snapshot.processes() {
            match snapshot.listeners(process.id) {
                Ok(listeners) if listeners.is_empty() => {}
                Ok(listeners) => found.push((process, listeners)),
                Err(navori_procs::Unavailable::Exited) => {}
                Err(_) => unread.push(process.id),
            }
        }
        let same = snapshot.still_same()?;
        let skipped = count(unread.iter().filter(|id| same.contains(id)));
        let mut entries = Vec::new();
        let mut backend = Vec::new();
        for (process, listeners) in found {
            if !same.contains(&process.id) {
                continue;
            }
            let exe = process.exe.clone().ok();
            let mut ports: BTreeMap<u16, BTreeSet<IpAddr>> = BTreeMap::new();
            for listener in listeners {
                ports
                    .entry(listener.port)
                    .or_default()
                    .insert(listener.addr);
            }
            if is_docker_backend(exe.as_deref()) {
                backend.extend(ports.into_iter().map(|(port, addresses)| BackendPort {
                    pid: process.id.pid,
                    start_us: process.id.start.0,
                    port,
                    addresses: addresses.into_iter().collect(),
                    exe: exe.clone(),
                }));
                continue;
            }
            let name = name_of(&snapshot, process.id, exe.as_deref());
            let harness = harness_for(&snapshot, process.id, memo, kept);
            let place = place_listener(snapshot.cwd(process.id), process.exe.clone(), &self.home);
            let kind = kind(exe.as_deref(), place.project.is_some());
            for (port, addresses) in ports {
                entries.push(PortEntry {
                    port,
                    addresses: addresses.into_iter().collect(),
                    pid: process.id.pid,
                    start_us: process.id.start.0,
                    name: name.clone(),
                    exe: exe.clone(),
                    place: place.clone(),
                    harness,
                    kind,
                    container: None,
                });
            }
        }
        Ok(Scan {
            entries,
            skipped,
            backend,
        })
    }
}

fn count<'a>(ids: impl Iterator<Item = &'a ProcId>) -> u32 {
    u32::try_from(ids.count()).unwrap_or(u32::MAX)
}

fn name_of(snapshot: &Snapshot, id: ProcId, exe: Option<&std::path::Path>) -> String {
    exe.and_then(|path| path.file_name())
        .map(|name| name.to_string_lossy().into_owned())
        .or_else(|| snapshot.name(id).ok())
        .unwrap_or_else(|| "unknown".to_owned())
}

fn harness_for(
    snapshot: &Snapshot,
    id: ProcId,
    memo: &HarnessMemo,
    kept: &mut HarnessMemo,
) -> Option<HarnessName> {
    let found = match memo.get(&id) {
        Some(found) => *found,
        None => match started_by(snapshot, id) {
            Ok(found) => found,
            Err(_) => return None,
        },
    };
    kept.insert(id, found);
    found.map(|harness| match harness {
        Harness::ClaudeCode => HarnessName::ClaudeCode,
        Harness::Codex => HarnessName::Codex,
        Harness::Grok => HarnessName::Grok,
    })
}

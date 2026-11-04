use std::cmp::Ordering;
use std::net::IpAddr;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Which Harness started a process.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HarnessName {
    ClaudeCode,
    Codex,
    Grok,
}

/// The branch checked out where a process runs, or the commit when none is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BranchName {
    Named(String),
    Detached(String),
}

/// What kind of thing holds a port.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PortKind {
    DevServer,
    Database,
    Docker,
    System,
    #[default]
    Other,
}

/// The container that holds a Docker port.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContainerRef {
    pub id: String,
    pub name: String,
}

/// Whether the Docker engine could be read. The text of `Unreadable` is fixed and names no path.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DockerStatus {
    #[default]
    NotSeen,
    Read,
    Unreadable(String),
}

/// Where a process runs: its project, worktree and branch.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Placement {
    pub project: Option<String>,
    pub worktree: Option<String>,
    pub branch: Option<BranchName>,
    pub missing_path: Option<PathBuf>,
}

/// One port held by one process, with every address it bound.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PortEntry {
    pub port: u16,
    pub addresses: Vec<IpAddr>,
    pub pid: i32,
    pub start_us: u64,
    pub name: String,
    pub exe: Option<PathBuf>,
    #[serde(flatten)]
    pub place: Placement,
    pub harness: Option<HarnessName>,
    pub kind: PortKind,
    pub container: Option<ContainerRef>,
}

/// One process and the ports it holds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProcessGroup {
    pub pid: i32,
    pub start_us: u64,
    pub name: String,
    pub exe: Option<PathBuf>,
    pub harness: Option<HarnessName>,
    pub entries: Vec<PortEntry>,
}

/// The processes that run in one worktree of a project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorktreeGroup {
    pub worktree: Option<String>,
    pub processes: Vec<ProcessGroup>,
}

/// The worktrees of one project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectGroup {
    pub project: Option<String>,
    pub worktrees: Vec<WorktreeGroup>,
}

/// What `ports.list` replies with.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PortsState {
    pub scanned_at_ms: Option<u64>,
    pub groups: Vec<ProjectGroup>,
    pub skipped: u32,
    pub docker: DockerStatus,
    pub problem: Option<String>,
}

impl PortsState {
    /// Every entry of every process, in group order.
    pub fn entries(&self) -> impl Iterator<Item = &PortEntry> {
        self.groups
            .iter()
            .flat_map(|project| &project.worktrees)
            .flat_map(|worktree| &worktree.processes)
            .flat_map(|process| &process.entries)
    }
}

/// Groups entries by project, then worktree, then process, in a total order.
pub fn group(mut entries: Vec<PortEntry>) -> Vec<ProjectGroup> {
    entries.sort_by(|a, b| {
        name_order(a.place.project.as_deref(), b.place.project.as_deref(), true)
            .then_with(|| {
                name_order(
                    a.place.worktree.as_deref(),
                    b.place.worktree.as_deref(),
                    false,
                )
            })
            .then_with(|| a.pid.cmp(&b.pid))
            .then_with(|| a.start_us.cmp(&b.start_us))
            .then_with(|| a.port.cmp(&b.port))
            .then_with(|| container_name(a).cmp(container_name(b)))
            .then_with(|| a.addresses.first().cmp(&b.addresses.first()))
    });
    let mut projects: Vec<ProjectGroup> = Vec::new();
    for entry in entries {
        let project = entry.place.project.clone();
        if projects.last().is_none_or(|last| last.project != project) {
            projects.push(ProjectGroup {
                project,
                worktrees: Vec::new(),
            });
        }
        let Some(current) = projects.last_mut() else {
            continue;
        };
        let worktree = entry.place.worktree.clone();
        if current
            .worktrees
            .last()
            .is_none_or(|last| last.worktree != worktree)
        {
            current.worktrees.push(WorktreeGroup {
                worktree,
                processes: Vec::new(),
            });
        }
        let Some(tree) = current.worktrees.last_mut() else {
            continue;
        };
        if tree
            .processes
            .last()
            .is_none_or(|last| last.pid != entry.pid || last.start_us != entry.start_us)
        {
            tree.processes.push(ProcessGroup {
                pid: entry.pid,
                start_us: entry.start_us,
                name: entry.name.clone(),
                exe: entry.exe.clone(),
                harness: entry.harness,
                entries: Vec::new(),
            });
        }
        if let Some(process) = tree.processes.last_mut() {
            process.entries.push(entry);
        }
    }
    projects
}

fn container_name(entry: &PortEntry) -> &str {
    entry.container.as_ref().map_or("", |c| c.name.as_str())
}

/// Case-insensitive, then exact. A missing name sorts last when `none_last`, else first.
fn name_order(a: Option<&str>, b: Option<&str>, none_last: bool) -> Ordering {
    match (a, b) {
        (None, None) => Ordering::Equal,
        (None, Some(_)) => {
            if none_last {
                Ordering::Greater
            } else {
                Ordering::Less
            }
        }
        (Some(_), None) => name_order(b, a, none_last).reverse(),
        (Some(a), Some(b)) => a
            .to_lowercase()
            .cmp(&b.to_lowercase())
            .then_with(|| a.cmp(b)),
    }
}

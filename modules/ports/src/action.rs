use navori_kernel::{ActionDoc, ModuleAction, Risk};
use serde::{Deserialize, Serialize};
use std::net::IpAddr;

use crate::infra::docker::is_container_id;

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PortsAction {
    /// Reply with the TCP ports the user's own processes listen on, from the last scan.
    List,
    /// Reply with the URL of a port in the last scan. It opens nothing and connects to nothing.
    Open {
        /// The port to give a URL for.
        port: u16,
        /// The bound address to use, when the port is bound to several that give different URLs.
        #[serde(default)]
        address: Option<IpAddr>,
    },
    /// Stop the listed processes and containers.
    Kill {
        /// Which pid, worktree or project the targets are meant to belong to. It never adds targets.
        selector: Selector,
        /// The exact processes and containers to stop. Must not be empty.
        targets: Vec<Target>,
    },
}

/// What the targets of a stop are expected to belong to.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Selector {
    /// One process, named by its pid.
    Pid(i32),
    /// Everything in one worktree of one project.
    Worktree { project: String, worktree: String },
    /// Everything in one project; no name means the group of ports with no project.
    Project(Option<String>),
}

/// One thing to stop.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Target {
    /// A process, named by its pid and its start time in microseconds so a reused pid is not mistaken for it.
    Process { pid: i32, start_us: u64 },
    /// A Docker container, named by its hex id.
    Container { id: String },
}

/// What happened to one target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Stopped,
    StoppedAlone,
    AlreadyGone,
    StillStopping,
    Stale,
    Protected,
    OutsideSelector,
    Failed,
}

/// The answer for one target, in request order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TargetResult {
    pub target: Target,
    pub result: Outcome,
    pub reason: Option<String>,
}

/// The answer to a stop request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct KillReply {
    pub results: Vec<TargetResult>,
}

/// Agent-decided cap on the targets of one stop request; a real project holds far fewer ports.
const MAX_TARGETS: usize = 256;

impl PortsAction {
    /// Rejects a request that is malformed before anything is read or signalled.
    pub(crate) fn validate(&self) -> Result<(), String> {
        let Self::Kill { selector, targets } = self else {
            return Ok(());
        };
        if targets.is_empty() {
            return Err("kill needs at least one target".to_owned());
        }
        if targets.len() > MAX_TARGETS {
            return Err(format!("kill takes at most {MAX_TARGETS} targets"));
        }
        let bad_pid = targets
            .iter()
            .any(|target| matches!(target, Target::Process { pid, .. } if *pid <= 1));
        if bad_pid || matches!(selector, Selector::Pid(pid) if *pid <= 1) {
            return Err("a process target needs a pid above 1".to_owned());
        }
        if let Selector::Pid(pid) = selector {
            let named =
                matches!(targets.as_slice(), [Target::Process { pid: only, .. }] if only == pid);
            if !named {
                return Err(format!(
                    "a pid selector needs exactly one process target with pid {pid}"
                ));
            }
        }
        let bad_id = targets
            .iter()
            .any(|target| matches!(target, Target::Container { id } if !is_container_id(id)));
        if bad_id {
            return Err("a container id must be 1 to 64 hex digits".to_owned());
        }
        Ok(())
    }
}

impl ModuleAction for PortsAction {
    fn risk(&self) -> Risk {
        match self {
            Self::List | Self::Open { .. } => Risk::Read,
            Self::Kill { .. } => Risk::Destructive,
        }
    }

    fn catalog() -> Vec<ActionDoc> {
        vec![
            ActionDoc {
                name: "list",
                description: "List the TCP ports the user's own processes listen on, grouped by project, worktree and process. It replies with the last scan, says when that scan was taken, and starts no scan of its own.",
                risk: Risk::Read,
            },
            ActionDoc {
                name: "open",
                description: "Give the http URL for a port found by the last scan, so a tool can open it. Loopback and wildcard addresses give localhost. When the port is bound to several addresses that give different URLs, name one with address. It launches nothing and never connects to the port.",
                risk: Risk::Read,
            },
            ActionDoc {
                name: "kill",
                description: "Stop processes and containers that hold ports. Send the exact targets from a ports list: a process is its pid and start time, a container is its id. The selector (a pid, a worktree of a project, or a project) is only a check that the targets belong where the caller thinks; it never adds targets. Navori re-reads the machine before it signals anything and refuses what changed, what is protected and what lies outside the selector. It replies with one result per target, in request order.",
                risk: Risk::Destructive,
            },
        ]
    }
}

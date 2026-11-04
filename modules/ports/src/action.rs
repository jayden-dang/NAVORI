use navori_kernel::{ActionDoc, ModuleAction, Risk};
use serde::Deserialize;

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PortsAction {
    /// Reply with the TCP ports the user's own processes listen on, from the last scan.
    List,
}

impl ModuleAction for PortsAction {
    fn risk(&self) -> Risk {
        match self {
            Self::List => Risk::Read,
        }
    }

    fn catalog() -> Vec<ActionDoc> {
        vec![ActionDoc {
            name: "list",
            description: "List the TCP ports the user's own processes listen on, grouped by project, worktree and process. It replies with the last scan, says when that scan was taken, and starts no scan of its own.",
            risk: Risk::Read,
        }]
    }
}

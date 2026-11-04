use navori_kernel::{ActionDoc, ModuleAction, Risk};
use serde::Deserialize;
use std::net::IpAddr;

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
}

impl ModuleAction for PortsAction {
    fn risk(&self) -> Risk {
        match self {
            Self::List | Self::Open { .. } => Risk::Read,
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
        ]
    }
}

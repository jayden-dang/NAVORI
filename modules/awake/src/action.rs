use navori_kernel::{ActionDoc, ModuleAction, Risk};
use serde::Deserialize;

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AwakeAction {
    /// Report the leases that are keeping this Mac awake.
    Status,
    /// Keep this Mac awake for `seconds` of wall-clock time.
    HoldFor { seconds: u64 },
    /// Keep this Mac awake until it is turned off.
    HoldUntilOff,
    /// Keep this Mac awake while process `pid` is still running.
    HoldCommand { pid: i32 },
    /// End every lease Navori is holding.
    ReleaseAll,
}

impl ModuleAction for AwakeAction {
    fn risk(&self) -> Risk {
        match self {
            Self::Status => Risk::Read,
            Self::HoldFor { .. }
            | Self::HoldUntilOff
            | Self::HoldCommand { .. }
            | Self::ReleaseAll => Risk::Write,
        }
    }

    fn catalog() -> Vec<ActionDoc> {
        vec![
            ActionDoc {
                name: "status",
                description: "Report the leases that are keeping this Mac awake.",
                risk: Risk::Read,
            },
            ActionDoc {
                name: "hold_for",
                description: "Keep this Mac awake for `seconds` of wall-clock time. A later duration or until-off request replaces the current one.",
                risk: Risk::Write,
            },
            ActionDoc {
                name: "hold_until_off",
                description: "Keep this Mac awake until it is turned off. A later duration request replaces this lease.",
                risk: Risk::Write,
            },
            ActionDoc {
                name: "hold_command",
                description: "Keep this Mac awake while process `pid` is still running. The lease has no deadline.",
                risk: Risk::Write,
            },
            ActionDoc {
                name: "release_all",
                description: "End every lease Navori is holding.",
                risk: Risk::Write,
            },
        ]
    }
}

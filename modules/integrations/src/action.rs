use navori_kernel::{ActionDoc, ModuleAction, Risk};
use serde::Deserialize;

pub use navori_protocol::Harness;

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum IntegrationsAction {
    /// Report, for each Harness, whether Navori's hooks are in place.
    Status,
    /// Add Navori's hooks to a Harness's Hook file.
    Install { harness: Harness },
    /// Bring Navori's hooks in a Harness's Hook file back to the correct set.
    Repair { harness: Harness },
    /// Remove Navori's hooks from a Harness's Hook file.
    Remove { harness: Harness },
}

impl ModuleAction for IntegrationsAction {
    fn risk(&self) -> Risk {
        match self {
            Self::Status => Risk::Read,
            Self::Install { .. } | Self::Repair { .. } | Self::Remove { .. } => Risk::Write,
        }
    }

    fn catalog() -> Vec<ActionDoc> {
        vec![
            ActionDoc {
                name: "status",
                description: "Report, for each Harness (claude, codex, grok), whether its config directory exists and whether Navori's hooks are installed, need repair or cannot be read, with the Actions that fit each row. Reads the files on this call only and changes nothing.",
                risk: Risk::Read,
            },
            ActionDoc {
                name: "install",
                description: "Add Navori's hooks for the running profile to the Hook file of `harness`. Other hooks in the file are left alone.",
                risk: Risk::Write,
            },
            ActionDoc {
                name: "repair",
                description: "Bring the running profile's hooks in the Hook file of `harness` to exactly the correct set. Other hooks in the file are left alone.",
                risk: Risk::Write,
            },
            ActionDoc {
                name: "remove",
                description: "Remove the running profile's hooks from the Hook file of `harness`. Other hooks in the file are left alone.",
                risk: Risk::Write,
            },
        ]
    }
}

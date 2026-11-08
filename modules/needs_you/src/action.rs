use navori_kernel::{ActionDoc, ModuleAction, Risk};
use navori_protocol::Envelope;
use serde::Deserialize;
use serde_json::Value;

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
pub struct Choice {
    pub label: String,
    #[serde(default)]
    #[schemars(with = "Option<Value>")]
    pub envelope: Option<Envelope>,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum NeedsYouAction {
    Raise {
        key: String,
        title: String,
        reason: String,
        #[serde(default)]
        expires_at_ms: Option<u64>,
        primary: Choice,
        secondary: Choice,
    },
}

impl ModuleAction for NeedsYouAction {
    fn risk(&self) -> Risk {
        match self {
            Self::Raise { .. } => Risk::Write,
        }
    }

    fn catalog() -> Vec<ActionDoc> {
        vec![ActionDoc {
            name: "raise",
            description: "Raise a Decision the user must answer. A repeat key replaces the open Decision.",
            risk: Risk::Write,
        }]
    }
}

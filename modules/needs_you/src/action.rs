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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Pick {
    Primary,
    Secondary,
}

impl Pick {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Primary => "primary",
            Self::Secondary => "secondary",
        }
    }
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
#[expect(
    clippy::large_enum_variant,
    reason = "Raise carries two Choice values; Answer is a small id/revision/pick"
)]
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
    Answer {
        id: i64,
        revision: u32,
        choice: Pick,
    },
    MarkViewed {
        ids: Vec<i64>,
    },
    ClearUpdates,
}

impl ModuleAction for NeedsYouAction {
    fn risk(&self) -> Risk {
        match self {
            Self::Raise { .. } => Risk::Write,
            Self::Answer { .. } => Risk::Destructive,
            Self::MarkViewed { .. } | Self::ClearUpdates => Risk::Write,
        }
    }

    fn catalog() -> Vec<ActionDoc> {
        vec![
            ActionDoc {
                name: "raise",
                description: "Raise a Decision the user must answer. A repeat key replaces the open Decision.",
                risk: Risk::Write,
            },
            ActionDoc {
                name: "answer",
                description: "Answer an open Decision with one of its choices.",
                risk: Risk::Destructive,
            },
            ActionDoc {
                name: "mark_viewed",
                description: "Mark the given Updates as viewed so they leave the list and stay in History.",
                risk: Risk::Write,
            },
            ActionDoc {
                name: "clear_updates",
                description: "Clear every unviewed Update into History.",
                risk: Risk::Write,
            },
        ]
    }
}

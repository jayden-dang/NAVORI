use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct NeedsYouState {
    pub decisions: Vec<OpenDecision>,
    pub updates: Vec<UnviewedUpdate>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OpenDecision {
    pub id: i64,
    pub key: String,
    pub revision: u32,
    pub title: String,
    pub reason: String,
    pub raised_at_ms: i64,
    pub expires_at_ms: Option<i64>,
    pub primary: ChoiceState,
    pub secondary: ChoiceState,
    pub state: DecisionState,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChoiceState {
    pub label: String,
    pub risk: Option<String>,
    /// Action name from the choice's envelope, when one is set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DecisionState {
    Open,
    Answering { choice: String },
    Failed { choice: String, error: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UnviewedUpdate {
    pub id: i64,
    pub kind: String,
    pub title: String,
    pub created_at_ms: i64,
}

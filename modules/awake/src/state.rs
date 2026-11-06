use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AwakeState {
    pub leases: Vec<LeaseState>,
    pub system_held: bool,
    pub display_held: bool,
    pub since_ms: Option<i64>,
    pub problem: Option<String>,
    pub tracking: bool,
    #[serde(default)]
    pub lid: bool,
    #[serde(default)]
    pub lid_held: bool,
    #[serde(default)]
    pub on_adapter: Option<bool>,
    #[serde(default)]
    pub lid_marker: bool,
    #[serde(default)]
    pub last_sleep_ms: Option<i64>,
    #[serde(default)]
    pub lid_problem: Option<String>,
}

impl AwakeState {
    pub(crate) fn idle() -> Self {
        Self {
            leases: Vec::new(),
            system_held: false,
            display_held: false,
            since_ms: None,
            problem: None,
            tracking: true,
            lid: false,
            lid_held: false,
            on_adapter: None,
            lid_marker: false,
            last_sleep_ms: None,
            lid_problem: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LeaseState {
    pub key: String,
    pub kind: String,
    pub harness: Option<String>,
    pub label: Option<String>,
    pub pid: Option<i32>,
    pub stopped: bool,
    pub ends_at_ms: Option<i64>,
    pub title: Option<String>,
}

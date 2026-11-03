use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AwakeState {
    pub leases: Vec<LeaseState>,
    pub system_held: bool,
    pub display_held: bool,
    pub since_ms: Option<i64>,
    pub problem: Option<String>,
}

impl AwakeState {
    pub(crate) fn idle() -> Self {
        Self {
            leases: Vec::new(),
            system_held: false,
            display_held: false,
            since_ms: None,
            problem: None,
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
}

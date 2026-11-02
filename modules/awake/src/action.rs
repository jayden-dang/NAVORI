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
    /// Keep the display on with its own lease, or end that lease.
    Display { on: bool },
    /// Keep this Mac awake while process `pid` is still running.
    HoldCommand { pid: i32 },
    /// Keep this Mac awake for the harness session `session_id`.
    HoldSession {
        session_id: String,
        pid: i32,
        ppid: i32,
    },
    /// Mark the harness session `session_id` as stopped.
    StopSession { session_id: String },
    /// End the session lease for `session_id`.
    EndSession { session_id: String },
    /// End every lease Navori is holding.
    ReleaseAll,
}

impl ModuleAction for AwakeAction {
    fn risk(&self) -> Risk {
        match self {
            Self::Status => Risk::Read,
            Self::HoldFor { .. }
            | Self::HoldUntilOff
            | Self::Display { .. }
            | Self::HoldCommand { .. }
            | Self::HoldSession { .. }
            | Self::StopSession { .. }
            | Self::EndSession { .. }
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
                name: "display",
                description: "Keep the display on with its own lease. Sending `on: false` ends that lease. It does not end a duration or until-off lease, and it is not stored.",
                risk: Risk::Write,
            },
            ActionDoc {
                name: "hold_command",
                description: "Keep this Mac awake while process `pid` is still running. The lease has no deadline.",
                risk: Risk::Write,
            },
            ActionDoc {
                name: "hold_session",
                description: "Keep this Mac awake for one harness session. The harness is resolved from `pid`, then `ppid`. A repeated hold keeps one lease, moves the deadline, and keeps the first hold.",
                risk: Risk::Write,
            },
            ActionDoc {
                name: "stop_session",
                description: "Mark a harness session as stopped. The lease stays through grace while a new descendant is running, and it ends at the cap.",
                risk: Risk::Write,
            },
            ActionDoc {
                name: "end_session",
                description: "End the session lease for `session_id` without reading the process list.",
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

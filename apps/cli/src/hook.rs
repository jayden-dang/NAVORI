//! `navori hook`: turns one Harness hook payload into at most one session Lease Action.

use std::time::Duration;

use serde::Deserialize;
use serde_json::{Value, json};

const SESSION_END_DEADLINE: Duration = Duration::from_millis(700);
const DEFAULT_DEADLINE: Duration = Duration::from_millis(1500);

#[derive(Debug, PartialEq)]
pub struct Plan {
    pub action: Value,
    pub deadline: Duration,
}

#[derive(Deserialize)]
struct Payload {
    hook_event_name: Option<String>,
    session_id: Option<String>,
    reason: Option<String>,
}

pub fn plan(stdin: &[u8], own_pid: u32, parent_pid: u32) -> Option<Plan> {
    let payload: Payload = serde_json::from_slice(stdin).ok()?;
    let session_id = payload.session_id?;
    let action = match payload.hook_event_name?.as_str() {
        "UserPromptSubmit" => {
            let pid = i32::try_from(own_pid).ok()?;
            let ppid = i32::try_from(parent_pid).ok()?;
            json!({ "hold_session": { "session_id": session_id, "pid": pid, "ppid": ppid } })
        }
        // Provisional for Claude Code and Codex: only Grok's `reason` rule is verified.
        "Stop" if matches!(payload.reason.as_deref(), None | Some("end_turn")) => {
            json!({ "stop_session": { "session_id": session_id } })
        }
        "StopCancelled" | "StopFailure" => {
            json!({ "stop_session": { "session_id": session_id } })
        }
        "SessionEnd" => {
            return Some(Plan {
                action: json!({ "end_session": { "session_id": session_id } }),
                deadline: SESSION_END_DEADLINE,
            });
        }
        _ => return None,
    };
    Some(Plan {
        action,
        deadline: DEFAULT_DEADLINE,
    })
}

//! `navori hook`: turns one Harness hook payload into at most one session Lease Action.

use std::io::Read;
use std::time::{Duration, Instant};

use navori_ipc::Client;
use navori_protocol::rpc::method;
use serde::Deserialize;
use serde_json::{Value, json};

const STDIN_CAP: u64 = 16 * 1024 * 1024;
const SESSION_END_DEADLINE: Duration = Duration::from_millis(700);
const DEFAULT_DEADLINE: Duration = Duration::from_millis(1500);

pub fn run(profile: &str, start: Instant) -> ! {
    std::panic::set_hook(Box::new(|_| {}));
    let _ = std::panic::catch_unwind(|| send_event(profile, start));
    std::process::exit(0)
}

fn send_event(profile: &str, start: Instant) {
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        if std::io::stdin()
            .take(STDIN_CAP)
            .read_to_end(&mut bytes)
            .is_ok()
        {
            let _ = sender.send(bytes);
        }
    });
    let Ok(stdin) = receiver.recv_timeout(SESSION_END_DEADLINE) else {
        return;
    };
    let Some(plan) = plan(
        &stdin,
        std::process::id(),
        std::os::unix::process::parent_id(),
    ) else {
        return;
    };
    let Some(remaining) = plan.deadline.checked_sub(start.elapsed()) else {
        return;
    };
    let Ok(socket) = super::socket_path(profile) else {
        return;
    };
    let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    else {
        return;
    };
    let call = async {
        let mut client = Client::connect(&socket).await.ok()?;
        client
            .call(
                method::DISPATCH,
                json!({ "module": "awake", "action": plan.action }),
            )
            .await
            .ok()
    };
    let _ = runtime.block_on(async { tokio::time::timeout(remaining, call).await });
}

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

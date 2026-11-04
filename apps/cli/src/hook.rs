//! `navori hook`: turns one Harness hook payload into at most one session Lease Action.

use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use navori_ipc::Client;
use navori_protocol::rpc::method;
use serde::Deserialize;
use serde_json::{Value, json};

const STDIN_CAP: u64 = 16 * 1024 * 1024;
const SESSION_END_DEADLINE: Duration = Duration::from_millis(700);
const DEFAULT_DEADLINE: Duration = Duration::from_millis(1500);
/// Claude Code rewrites its title records often: the furthest back seen was 31 KB from the end.
const TRANSCRIPT_TAIL: u64 = 128 * 1024;

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
    agent_id: Option<String>,
    background_tasks: Option<Value>,
    session_title: Option<String>,
    transcript_path: Option<PathBuf>,
    cwd: Option<String>,
}

struct TitleSource {
    named: Option<String>,
    transcript: Option<PathBuf>,
}

fn add_title(fields: &mut Value, title: Option<String>) {
    if let Some(title) = title {
        fields["title"] = json!(title);
    }
}

/// The name the user gave, then the Harness's own title. Only these leave the transcript.
fn session_title(source: &TitleSource) -> Option<String> {
    if let Some(title) = source.named.as_deref().map(str::trim)
        && !title.is_empty()
    {
        return Some(title.to_owned());
    }
    title_in_transcript(source.transcript.as_deref()?)
}

/// `custom-title` and `ai-title` are Claude Code's own records, not a documented format. Anything
/// else in the file reads as no title, so the row falls back to the branch.
fn title_in_transcript(path: &Path) -> Option<String> {
    let mut file = std::fs::File::open(path).ok()?;
    let length = file.metadata().ok()?.len();
    file.seek(SeekFrom::Start(length.saturating_sub(TRANSCRIPT_TAIL)))
        .ok()?;
    let mut bytes = Vec::new();
    file.take(TRANSCRIPT_TAIL).read_to_end(&mut bytes).ok()?;
    let text = String::from_utf8_lossy(&bytes);
    let mut latest_ai = None;
    for line in text.lines().rev() {
        // Prompt text can hold these words too, so only a whole record of that type counts.
        if !line.starts_with(r#"{"type":""#) {
            continue;
        }
        let Ok(record) = serde_json::from_str::<TitleRecord>(line) else {
            continue;
        };
        match record {
            TitleRecord::Custom { custom_title } => return Some(custom_title),
            TitleRecord::Ai { ai_title } => {
                latest_ai.get_or_insert(ai_title);
            }
        }
    }
    latest_ai
}

#[derive(Deserialize)]
#[serde(tag = "type")]
enum TitleRecord {
    #[serde(rename = "custom-title")]
    Custom {
        #[serde(rename = "customTitle")]
        custom_title: String,
    },
    #[serde(rename = "ai-title")]
    Ai {
        #[serde(rename = "aiTitle")]
        ai_title: String,
    },
}

/// Only whether work is in flight leaves the command, never what the work is.
fn with_background(mut fields: Value, background: bool, action: &str) -> Value {
    if background {
        fields["background"] = json!(true);
    }
    json!({ action: fields })
}

pub fn plan(stdin: &[u8], own_pid: u32, parent_pid: u32) -> Option<Plan> {
    let payload: Payload = serde_json::from_slice(stdin).ok()?;
    let title_source = TitleSource {
        named: payload.session_title,
        transcript: payload.transcript_path,
    };
    let session_id = payload.session_id?;
    let event = payload.hook_event_name?;
    // A list of any other shape must not lose the event itself.
    let background = payload
        .background_tasks
        .as_ref()
        .and_then(Value::as_array)
        .is_some_and(|tasks| !tasks.is_empty());
    let action = match event.as_str() {
        "UserPromptSubmit" => {
            let pid = i32::try_from(own_pid).ok()?;
            let ppid = i32::try_from(parent_pid).ok()?;
            let mut fields = json!({ "session_id": session_id, "pid": pid, "ppid": ppid });
            if let Some(cwd) = payload.cwd {
                fields["cwd"] = json!(cwd);
            }
            add_title(&mut fields, session_title(&title_source));
            json!({ "hold_session": fields })
        }
        // Provisional for Claude Code and Codex: only Grok's `reason` rule is verified.
        name @ ("Stop" | "StopCancelled" | "StopFailure" | "Interrupt")
            if name != "Stop" || matches!(payload.reason.as_deref(), None | Some("end_turn")) =>
        {
            let mut fields = json!({ "session_id": session_id });
            add_title(&mut fields, session_title(&title_source));
            with_background(fields, background, "stop_session")
        }
        "SubagentStart" => {
            let pid = i32::try_from(own_pid).ok()?;
            let ppid = i32::try_from(parent_pid).ok()?;
            let agent_id = payload.agent_id?;
            json!({ "subagent_start": {
                "session_id": session_id, "agent_id": agent_id, "pid": pid, "ppid": ppid
            } })
        }
        "SubagentStop" => {
            let agent_id = payload.agent_id?;
            with_background(
                json!({ "session_id": session_id, "agent_id": agent_id }),
                background,
                "subagent_stop",
            )
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

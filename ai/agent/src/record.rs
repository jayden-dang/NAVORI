use std::fmt;
use std::str::FromStr;

use navori_context::{Message, ToolCall, Usage};
use serde::{Deserialize, Serialize};
use ulid::Ulid;

const BODY_VERSION: u32 = 1;
const UNREADABLE_BODY: &str = r#"{"v":0}"#;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ConversationId(Ulid);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TurnId(Ulid);

macro_rules! ulid_id {
    ($name:ident) => {
        impl $name {
            pub fn new() -> Self {
                Self(Ulid::generate())
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(f)
            }
        }

        impl FromStr for $name {
            type Err = RecordError;

            fn from_str(s: &str) -> Result<Self, RecordError> {
                Ulid::from_string(s)
                    .map(Self)
                    .map_err(|_| RecordError::Read)
            }
        }
    };
}

ulid_id!(ConversationId);
ulid_id!(TurnId);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    User,
}

impl Origin {
    pub fn as_str(&self) -> &'static str {
        match self {
            Origin::User => "user",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Entry {
    ConversationStarted {
        id: ConversationId,
        ts_ms: i64,
    },
    TurnStarted {
        id: TurnId,
        conversation: ConversationId,
        seq: u32,
        origin: Origin,
        model: String,
        ts_ms: i64,
    },
    StepStarted {
        turn: TurnId,
        seq: u32,
        ts_ms: i64,
    },
    Message {
        turn: TurnId,
        seq: u32,
        step: Option<u32>,
        body: RecordedMessage,
        redactions: u32,
        ts_ms: i64,
    },
    Usage {
        turn: TurnId,
        step: u32,
        usage: Option<Usage>,
    },
    StepEnded {
        turn: TurnId,
        seq: u32,
        ts_ms: i64,
    },
    TurnEnded {
        turn: TurnId,
        end: String,
        ts_ms: i64,
    },
}

pub trait ConversationStore: Send + Sync {
    fn append(&self, entries: &[Entry]) -> Result<(), RecordError>;
    fn load(&self, conversation: &ConversationId) -> Result<ConversationLog, RecordError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoggedTurn {
    pub id: TurnId,
    pub seq: u32,
    pub origin: Origin,
    pub messages: Vec<(u32, Option<u32>, RecordedMessage)>,
    pub open_step: Option<u32>,
    pub end: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ConversationLog {
    pub turns: Vec<LoggedTurn>,
}

impl ConversationLog {
    pub fn from_entries(entries: Vec<Entry>) -> Result<ConversationLog, RecordError> {
        let mut turns: Vec<LoggedTurn> = Vec::new();
        for entry in entries {
            match entry {
                Entry::ConversationStarted { .. } | Entry::Usage { .. } => {}
                Entry::TurnStarted {
                    id, seq, origin, ..
                } => {
                    if turns.iter().any(|t| t.seq == seq) {
                        return Err(RecordError::Duplicate);
                    }
                    turns.push(LoggedTurn {
                        id,
                        seq,
                        origin,
                        messages: Vec::new(),
                        open_step: None,
                        end: None,
                    });
                }
                Entry::StepStarted { turn, seq, .. } => {
                    turn_mut(&mut turns, turn)?.open_step = Some(seq);
                }
                Entry::Message {
                    turn,
                    seq,
                    step,
                    body,
                    ..
                } => {
                    let logged = turn_mut(&mut turns, turn)?;
                    if let RecordedMessage::ToolResult { call_id, .. } = &body {
                        let answered = logged.messages.iter().any(|(_, _, m)| {
                            matches!(m, RecordedMessage::ToolResult { call_id: c, .. } if c == call_id)
                        });
                        if answered {
                            return Err(RecordError::Duplicate);
                        }
                    }
                    logged.messages.push((seq, step, body));
                }
                Entry::StepEnded { turn, .. } => {
                    turn_mut(&mut turns, turn)?.open_step = None;
                }
                Entry::TurnEnded { turn, end, .. } => {
                    let logged = turn_mut(&mut turns, turn)?;
                    logged.open_step = None;
                    logged.end = Some(end);
                }
            }
        }
        turns.sort_by_key(|t| t.seq);
        Ok(ConversationLog { turns })
    }
}

fn turn_mut(turns: &mut [LoggedTurn], id: TurnId) -> Result<&mut LoggedTurn, RecordError> {
    turns
        .iter_mut()
        .find(|t| t.id == id)
        .ok_or(RecordError::Read)
}

pub fn derive_messages(log: &ConversationLog) -> Vec<Message> {
    log.turns
        .iter()
        .flat_map(|t| t.messages.iter())
        .map(|(_, _, body)| match body {
            RecordedMessage::Input { text } => Message::User { text: text.clone() },
            RecordedMessage::Assistant {
                text,
                reasoning,
                tool_calls,
            } => Message::Assistant {
                text: text.clone(),
                reasoning: reasoning.clone(),
                tool_calls: tool_calls.clone(),
            },
            RecordedMessage::ToolResult { call_id, text, .. } => Message::Tool {
                call_id: call_id.clone(),
                text: text.clone(),
            },
        })
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum RecordError {
    #[error("unknown conversation")]
    UnknownConversation,
    #[error("conversation tables could not be migrated")]
    Migration,
    #[error("conversation entries could not be written")]
    Write,
    #[error("conversation entries could not be read")]
    Read,
    #[error("entry already recorded")]
    Duplicate,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecordedMessage {
    Input {
        text: String,
    },
    Assistant {
        text: Option<String>,
        reasoning: Option<String>,
        tool_calls: Vec<ToolCall>,
    },
    ToolResult {
        call_id: String,
        text: String,
        is_error: bool,
        synthetic: bool,
    },
}

#[derive(Serialize, Deserialize)]
struct CallBody {
    id: String,
    name: String,
    arguments: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct InputBody {
    v: u32,
    text: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct AssistantBody {
    v: u32,
    text: Option<String>,
    reasoning: Option<String>,
    tool_calls: Vec<CallBody>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ResultBody {
    v: u32,
    call_id: String,
    text: String,
    is_error: bool,
    synthetic: bool,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum AnyBody {
    Result(ResultBody),
    Assistant(AssistantBody),
    Input(InputBody),
}

impl RecordedMessage {
    pub fn to_body(&self) -> String {
        let json = match self {
            RecordedMessage::Input { text } => serde_json::to_string(&InputBody {
                v: BODY_VERSION,
                text: text.clone(),
            }),
            RecordedMessage::Assistant {
                text,
                reasoning,
                tool_calls,
            } => serde_json::to_string(&AssistantBody {
                v: BODY_VERSION,
                text: text.clone(),
                reasoning: reasoning.clone(),
                tool_calls: tool_calls
                    .iter()
                    .map(|c| CallBody {
                        id: c.id.clone(),
                        name: c.name.clone(),
                        arguments: c.arguments.clone(),
                    })
                    .collect(),
            }),
            RecordedMessage::ToolResult {
                call_id,
                text,
                is_error,
                synthetic,
            } => serde_json::to_string(&ResultBody {
                v: BODY_VERSION,
                call_id: call_id.clone(),
                text: text.clone(),
                is_error: *is_error,
                synthetic: *synthetic,
            }),
        };
        json.unwrap_or_else(|error| {
            tracing::error!(%error, "conversation body could not be serialized");
            UNREADABLE_BODY.to_owned()
        })
    }

    pub fn from_body(body: &str) -> Result<RecordedMessage, RecordError> {
        let parsed: AnyBody = serde_json::from_str(body).map_err(|_| RecordError::Read)?;
        let (version, message) = match parsed {
            AnyBody::Input(b) => (b.v, RecordedMessage::Input { text: b.text }),
            AnyBody::Assistant(b) => (
                b.v,
                RecordedMessage::Assistant {
                    text: b.text,
                    reasoning: b.reasoning,
                    tool_calls: b
                        .tool_calls
                        .into_iter()
                        .map(|c| ToolCall {
                            id: c.id,
                            name: c.name,
                            arguments: c.arguments,
                        })
                        .collect(),
                },
            ),
            AnyBody::Result(b) => (
                b.v,
                RecordedMessage::ToolResult {
                    call_id: b.call_id,
                    text: b.text,
                    is_error: b.is_error,
                    synthetic: b.synthetic,
                },
            ),
        };
        if version == BODY_VERSION {
            Ok(message)
        } else {
            Err(RecordError::Read)
        }
    }
}

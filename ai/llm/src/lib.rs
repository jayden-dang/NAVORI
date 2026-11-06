//! Provider-neutral LLM client.

#[cfg(feature = "fake")]
pub mod fake;
mod openai_compat;

use futures::future::BoxFuture;
use navori_secrets::Secret;

pub use openai_compat::OpenAiCompat;

pub const DEEPSEEK_BASE_URL: &str = "https://api.deepseek.com";
pub const DEEPSEEK_PROVIDER: &str = "deepseek";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Message {
    User {
        text: String,
    },
    Assistant {
        text: Option<String>,
        reasoning: Option<String>,
        tool_calls: Vec<ToolCall>,
    },
    Tool {
        call_id: String,
        text: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolDef {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<Message>,
    pub tools: Vec<ToolDef>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Usage {
    pub input: u64,
    pub output: u64,
    pub cache_hit: Option<u64>,
    pub cache_miss: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Finish {
    Stop,
    ToolCalls,
    Length,
    Other(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatReply {
    pub text: Option<String>,
    pub reasoning: Option<String>,
    pub tool_calls: Vec<ToolCall>,
    pub usage: Option<Usage>,
    pub finish: Finish,
}

#[derive(Debug, thiserror::Error)]
pub enum LlmError {
    #[error("could not reach the provider")]
    Network,
    #[error("the provider did not answer in time")]
    Timeout,
    #[error("the provider answered with status {0}{hint}", hint = status_hint(*.0))]
    Status(u16),
    #[error("the provider's answer had no text")]
    Malformed,
}

impl LlmError {
    /// A fixed name for the failure, safe to log.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Network => "network",
            Self::Timeout => "timeout",
            Self::Status(_) => "status",
            Self::Malformed => "malformed",
        }
    }
}

fn status_hint(status: u16) -> &'static str {
    match status {
        401 => " (check the key saved in Settings → AI)",
        402 => " (check the provider account balance)",
        404 | 422 => " (check the model id given to --model)",
        429 => " (try again shortly)",
        _ => "",
    }
}

pub trait LlmProvider: Send + Sync {
    fn complete<'a>(
        &'a self,
        req: ChatRequest,
        key: &'a Secret,
    ) -> BoxFuture<'a, Result<ChatReply, LlmError>>;
}

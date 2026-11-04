//! Provider-neutral LLM client.

mod openai_compat;

use futures::future::BoxFuture;
use navori_secrets::Secret;

pub use openai_compat::OpenAiCompat;

pub const DEEPSEEK_BASE_URL: &str = "https://api.deepseek.com";
pub const DEEPSEEK_PROVIDER: &str = "deepseek";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    User,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    pub role: Role,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<Message>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatReply {
    pub text: String,
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

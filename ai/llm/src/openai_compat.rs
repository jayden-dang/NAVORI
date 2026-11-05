use std::fmt;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use futures::future::BoxFuture;
use navori_secrets::Secret;
use serde::{Deserialize, Serialize};
use tokio::sync::OnceCell;

use crate::{
    ChatReply, ChatRequest, Finish, LlmError, LlmProvider, Message, ToolCall, ToolDef, Usage,
};

const TIMEOUT: Duration = Duration::from_secs(120);

pub struct OpenAiCompat {
    base_url: String,
    client: OnceCell<reqwest::Client>,
    builds: AtomicUsize,
}

impl OpenAiCompat {
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            client: OnceCell::new(),
            builds: AtomicUsize::new(0),
        }
    }

    pub fn client_builds(&self) -> usize {
        self.builds.load(Ordering::SeqCst)
    }

    async fn client(&self) -> Result<&reqwest::Client, LlmError> {
        self.client
            .get_or_try_init(|| async {
                let client = reqwest::Client::builder()
                    .timeout(TIMEOUT)
                    .redirect(reqwest::redirect::Policy::none())
                    .build()
                    .map_err(|_| LlmError::Network)?;
                self.builds.fetch_add(1, Ordering::SeqCst);
                Ok(client)
            })
            .await
    }

    async fn send(&self, req: ChatRequest, key: &Secret) -> Result<ChatReply, LlmError> {
        let body = WireRequest {
            model: &req.model,
            messages: req.messages.iter().map(wire_message).collect(),
            tools: req.tools.iter().map(wire_tool).collect(),
            stream: false,
        };
        let url = format!("{}/chat/completions", self.base_url.trim_end_matches('/'));
        let response = self
            .client()
            .await?
            .post(url)
            .bearer_auth(key.expose())
            .json(&body)
            .send()
            .await
            .map_err(|e| classify(&e, LlmError::Network))?;
        let status = response.status();
        if !status.is_success() {
            return Err(LlmError::Status(status.as_u16()));
        }
        let parsed: WireReply = response
            .json()
            .await
            .map_err(|e| classify(&e, LlmError::Malformed))?;
        let choice = parsed
            .choices
            .into_iter()
            .next()
            .ok_or(LlmError::Malformed)?;
        let text = choice.message.content.filter(|text| !text.is_empty());
        let tool_calls: Vec<ToolCall> = choice
            .message
            .tool_calls
            .into_iter()
            .map(|call| ToolCall {
                id: call.id,
                name: call.function.name,
                arguments: call.function.arguments,
            })
            .collect();
        if text.is_none() && tool_calls.is_empty() {
            return Err(LlmError::Malformed);
        }
        Ok(ChatReply {
            text,
            reasoning: choice.message.reasoning_content,
            tool_calls,
            usage: parsed.usage.map(|usage| Usage {
                input: usage.prompt_tokens,
                output: usage.completion_tokens,
                cache_hit: usage.prompt_cache_hit_tokens,
                cache_miss: usage.prompt_cache_miss_tokens,
            }),
            finish: finish(choice.finish_reason),
        })
    }
}

fn finish(reason: Option<String>) -> Finish {
    match reason.as_deref() {
        Some("stop") | None => Finish::Stop,
        Some("tool_calls") => Finish::ToolCalls,
        Some("length") => Finish::Length,
        Some(other) => Finish::Other(other.to_owned()),
    }
}

fn wire_message(message: &Message) -> WireMessage<'_> {
    match message {
        Message::User { text } => WireMessage {
            role: "user",
            content: Some(text),
            reasoning_content: None,
            tool_calls: Vec::new(),
            tool_call_id: None,
        },
        Message::Assistant {
            text,
            reasoning,
            tool_calls,
        } => WireMessage {
            role: "assistant",
            content: text.as_deref(),
            reasoning_content: reasoning.as_deref(),
            tool_calls: tool_calls
                .iter()
                .map(|call| WireCall {
                    id: &call.id,
                    kind: "function",
                    function: WireCallFunction {
                        name: &call.name,
                        arguments: &call.arguments,
                    },
                })
                .collect(),
            tool_call_id: None,
        },
        Message::Tool { call_id, text } => WireMessage {
            role: "tool",
            content: Some(text),
            reasoning_content: None,
            tool_calls: Vec::new(),
            tool_call_id: Some(call_id),
        },
    }
}

fn wire_tool(tool: &ToolDef) -> WireTool<'_> {
    WireTool {
        kind: "function",
        function: WireToolFunction {
            name: &tool.name,
            description: &tool.description,
            parameters: &tool.parameters,
        },
    }
}

/// A timeout is `Timeout`; anything else is `otherwise`. The error text is dropped on purpose.
fn classify(error: &reqwest::Error, otherwise: LlmError) -> LlmError {
    if error.is_timeout() {
        LlmError::Timeout
    } else {
        otherwise
    }
}

impl fmt::Debug for OpenAiCompat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OpenAiCompat")
            .field("base_url", &self.base_url)
            .field("client_built", &self.client.initialized())
            .finish()
    }
}

impl LlmProvider for OpenAiCompat {
    fn complete<'a>(
        &'a self,
        req: ChatRequest,
        key: &'a Secret,
    ) -> BoxFuture<'a, Result<ChatReply, LlmError>> {
        Box::pin(self.send(req, key))
    }
}

#[derive(Serialize)]
struct WireRequest<'a> {
    model: &'a str,
    messages: Vec<WireMessage<'a>>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<WireTool<'a>>,
    stream: bool,
}

#[derive(Serialize)]
struct WireMessage<'a> {
    role: &'static str,
    content: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reasoning_content: Option<&'a str>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tool_calls: Vec<WireCall<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<&'a str>,
}

#[derive(Serialize)]
struct WireCall<'a> {
    id: &'a str,
    #[serde(rename = "type")]
    kind: &'static str,
    function: WireCallFunction<'a>,
}

#[derive(Serialize)]
struct WireCallFunction<'a> {
    name: &'a str,
    arguments: &'a str,
}

#[derive(Serialize)]
struct WireTool<'a> {
    #[serde(rename = "type")]
    kind: &'static str,
    function: WireToolFunction<'a>,
}

#[derive(Serialize)]
struct WireToolFunction<'a> {
    name: &'a str,
    description: &'a str,
    parameters: &'a serde_json::Value,
}

#[derive(Deserialize)]
struct WireReply {
    choices: Vec<WireChoice>,
    usage: Option<WireUsage>,
}

#[derive(Deserialize)]
struct WireChoice {
    message: WireContent,
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct WireContent {
    content: Option<String>,
    reasoning_content: Option<String>,
    #[serde(default)]
    tool_calls: Vec<WireReplyCall>,
}

#[derive(Deserialize)]
struct WireReplyCall {
    id: String,
    function: WireReplyFunction,
}

#[derive(Deserialize)]
struct WireReplyFunction {
    name: String,
    arguments: String,
}

#[derive(Deserialize)]
struct WireUsage {
    prompt_tokens: u64,
    completion_tokens: u64,
    prompt_cache_hit_tokens: Option<u64>,
    prompt_cache_miss_tokens: Option<u64>,
}

use std::fmt;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use futures::future::BoxFuture;
use navori_secrets::Secret;
use serde::{Deserialize, Serialize};
use tokio::sync::OnceCell;

use crate::{ChatReply, ChatRequest, LlmError, LlmProvider, Role};

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
            messages: req
                .messages
                .iter()
                .map(|m| WireMessage {
                    role: match m.role {
                        Role::User => "user",
                    },
                    content: &m.text,
                })
                .collect(),
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
            .map_err(transport_error)?;
        let status = response.status();
        if !status.is_success() {
            return Err(LlmError::Status(status.as_u16()));
        }
        let parsed: WireReply = response.json().await.map_err(|e| {
            if e.is_timeout() {
                LlmError::Timeout
            } else {
                LlmError::Malformed
            }
        })?;
        parsed
            .choices
            .into_iter()
            .next()
            .map(|c| c.message.content)
            .filter(|text| !text.is_empty())
            .map(|text| ChatReply { text })
            .ok_or(LlmError::Malformed)
    }
}

fn transport_error(error: reqwest::Error) -> LlmError {
    if error.is_timeout() {
        LlmError::Timeout
    } else {
        LlmError::Network
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
    stream: bool,
}

#[derive(Serialize)]
struct WireMessage<'a> {
    role: &'static str,
    content: &'a str,
}

#[derive(Deserialize)]
struct WireReply {
    choices: Vec<WireChoice>,
}

#[derive(Deserialize)]
struct WireChoice {
    message: WireContent,
}

#[derive(Deserialize)]
struct WireContent {
    content: String,
}

use std::fmt;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use futures::future::BoxFuture;
use navori_redact::{Redacted, redact_text};
use navori_secrets::{Secret, SecretKey, SecretStore};
use serde_json::Value;
use tokio::task::spawn_blocking;

pub use navori_llm::{ChatReply, ChatRequest, Finish, Message, ToolCall, ToolDef, Usage};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SendError(pub String);

pub trait Sender: Send + Sync {
    fn send<'a>(
        &'a self,
        key: &'a Secret,
        model: &'a str,
        text: &'a str,
    ) -> BoxFuture<'a, Result<String, SendError>>;
}

pub trait ChatSender: Send + Sync {
    fn send<'a>(
        &'a self,
        key: &'a Secret,
        request: ChatRequest,
    ) -> BoxFuture<'a, Result<ChatReply, SendError>>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shielded {
    pub text: String,
    pub count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outbound {
    pub request: ChatRequest,
    pub prior_redactions: u32,
}

/// Why an audit row was not recorded. Fixed names only: they go to logs, never to the user.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuditError {
    Migration,
    Write,
    ShieldPass,
    Unfinished,
}

impl AuditError {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Migration => "migration",
            Self::Write => "write",
            Self::ShieldPass => "shield_pass",
            Self::Unfinished => "unfinished",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Authorized,
    Blocked,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditRow {
    pub ts_ms: i64,
    pub provider: String,
    pub kinds: Vec<String>,
    pub redacted_count: u32,
    pub outcome: Outcome,
}

pub trait AuditSink: Send + Sync {
    fn record(&self, row: AuditRow) -> Result<(), AuditError>;
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AskError {
    #[error("No key set. Add one in Settings → AI.")]
    NoKey,
    #[error("{0}")]
    Keychain(String),
    #[error("The Shield failed, so nothing was sent.")]
    Shield,
    #[error("The audit log could not be recorded, so nothing was sent.")]
    Audit,
    #[error("{0}")]
    Send(String),
}

pub struct Gateway {
    secrets: Arc<dyn SecretStore>,
    key: SecretKey,
    sender: Arc<dyn Sender>,
    chat_sender: Option<Arc<dyn ChatSender>>,
    audit: Arc<dyn AuditSink>,
    provider: &'static str,
    shield: fn(&str) -> Redacted,
}

impl fmt::Debug for Gateway {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Gateway")
            .field("provider", &self.provider)
            .finish_non_exhaustive()
    }
}

impl Gateway {
    pub fn new(
        secrets: Arc<dyn SecretStore>,
        key: SecretKey,
        sender: Arc<dyn Sender>,
        audit: Arc<dyn AuditSink>,
        provider: &'static str,
    ) -> Self {
        Self::with_shield(secrets, key, sender, audit, provider, redact_text)
    }

    pub fn with_shield(
        secrets: Arc<dyn SecretStore>,
        key: SecretKey,
        sender: Arc<dyn Sender>,
        audit: Arc<dyn AuditSink>,
        provider: &'static str,
        shield: fn(&str) -> Redacted,
    ) -> Self {
        Self {
            secrets,
            key,
            sender,
            chat_sender: None,
            audit,
            provider,
            shield,
        }
    }

    #[must_use]
    pub fn with_chat_sender(mut self, chat_sender: Arc<dyn ChatSender>) -> Self {
        self.chat_sender = Some(chat_sender);
        self
    }

    pub fn shield(&self, text: &str) -> Option<Shielded> {
        let redacted = self.inspect(text)?;
        let fresh = redacted
            .findings
            .iter()
            .filter(|finding| {
                !text
                    .get(finding.start..finding.end)
                    .is_some_and(|span| span.starts_with("[REDACTED:"))
            })
            .count();
        Some(Shielded {
            text: redacted.text,
            count: u32::try_from(fresh).unwrap_or(u32::MAX),
        })
    }

    pub async fn send_chat(&self, outbound: Outbound) -> Result<ChatReply, AskError> {
        let Some(chat_sender) = self.chat_sender.as_ref() else {
            return Err(AskError::Send("no chat sender is configured".to_owned()));
        };
        let secret = self.read_key().await?;

        let Outbound {
            mut request,
            prior_redactions,
        } = outbound;
        let Some(fresh) = self.shield_request(&mut request) else {
            tracing::warn!(
                provider = self.provider,
                reason = "shield_refused",
                "chat refused"
            );
            if let Err(error) = self.write_row(Outcome::Blocked, 0).await {
                tracing::warn!(
                    provider = self.provider,
                    reason = error.as_str(),
                    "blocked audit row not recorded"
                );
            }
            return Err(AskError::Shield);
        };

        let count = prior_redactions.saturating_add(fresh);
        if let Err(error) = self.write_row(Outcome::Authorized, count).await {
            tracing::warn!(
                provider = self.provider,
                reason = error.as_str(),
                "context audit write failed"
            );
            return Err(AskError::Audit);
        }

        chat_sender
            .send(&secret, request)
            .await
            .map_err(|SendError(message)| AskError::Send(message))
    }

    fn shield_request(&self, request: &mut ChatRequest) -> Option<u32> {
        let mut total = 0u32;
        for message in &mut request.messages {
            match message {
                Message::User { text } | Message::Tool { text, .. } => {
                    total = total.saturating_add(self.shield_in_place(text)?);
                }
                Message::Assistant {
                    text,
                    reasoning,
                    tool_calls,
                } => {
                    for part in [text, reasoning].into_iter().flatten() {
                        total = total.saturating_add(self.shield_in_place(part)?);
                    }
                    for call in tool_calls {
                        total = total.saturating_add(self.shield_in_place(&mut call.arguments)?);
                    }
                }
            }
        }
        for tool in &mut request.tools {
            total = total.saturating_add(self.shield_in_place(&mut tool.name)?);
            total = total.saturating_add(self.shield_in_place(&mut tool.description)?);
            total = total.saturating_add(self.shield_value(&mut tool.parameters)?);
        }
        Some(total)
    }

    fn shield_in_place(&self, text: &mut String) -> Option<u32> {
        let shielded = self.shield(text)?;
        *text = shielded.text;
        Some(shielded.count)
    }

    fn shield_value(&self, value: &mut Value) -> Option<u32> {
        match value {
            Value::String(text) => self.shield_in_place(text),
            Value::Array(items) => items.iter_mut().try_fold(0u32, |total, item| {
                Some(total.saturating_add(self.shield_value(item)?))
            }),
            Value::Object(map) => {
                let mut total = 0u32;
                for (key, item) in map.iter_mut() {
                    if self.shield(key)?.text != *key {
                        return None;
                    }
                    total = total.saturating_add(self.shield_value(item)?);
                }
                Some(total)
            }
            _ => Some(0),
        }
    }

    pub async fn ask(&self, model: &str, text: &str) -> Result<String, AskError> {
        let secret = self.read_key().await?;

        let Some(redacted) = self.inspect(text) else {
            tracing::warn!(
                provider = self.provider,
                reason = "shield_refused",
                "ask refused"
            );
            if let Err(error) = self.write_row(Outcome::Blocked, 0).await {
                tracing::warn!(
                    provider = self.provider,
                    reason = error.as_str(),
                    "blocked audit row not recorded"
                );
            }
            return Err(AskError::Shield);
        };

        let count = u32::try_from(redacted.findings.len()).unwrap_or(u32::MAX);
        if let Err(error) = self.write_row(Outcome::Authorized, count).await {
            tracing::warn!(
                provider = self.provider,
                reason = error.as_str(),
                "context audit write failed"
            );
            return Err(AskError::Audit);
        }

        self.sender
            .send(&secret, model, &redacted.text)
            .await
            .map_err(|SendError(message)| AskError::Send(message))
    }

    async fn read_key(&self) -> Result<Secret, AskError> {
        let secrets = Arc::clone(&self.secrets);
        let key = self.key.clone();
        let (reason, message) = match spawn_blocking(move || secrets.get(&key)).await {
            Ok(Ok(Some(secret))) => return Ok(secret),
            Ok(Ok(None)) => return Err(AskError::NoKey),
            Ok(Err(error)) => ("keychain_failed", error.to_string()),
            Err(_) => (
                "keychain_unfinished",
                "the Keychain call did not finish".to_owned(),
            ),
        };
        tracing::warn!(provider = self.provider, reason, "ask refused");
        Err(AskError::Keychain(message))
    }

    fn inspect(&self, text: &str) -> Option<Redacted> {
        let redacted = catch_unwind(AssertUnwindSafe(|| (self.shield)(text))).ok()?;
        well_formed(text, &redacted).then_some(redacted)
    }

    async fn write_row(&self, outcome: Outcome, redacted_count: u32) -> Result<(), AuditError> {
        let row = AuditRow {
            ts_ms: now_ms(),
            provider: self.provider.to_owned(),
            kinds: vec!["text".to_owned()],
            redacted_count,
            outcome,
        };
        let audit = Arc::clone(&self.audit);
        spawn_blocking(move || audit.record(row))
            .await
            .unwrap_or(Err(AuditError::Unfinished))
    }
}

/// Findings must be sorted, non-overlapping and cut `text` on character boundaries.
fn well_formed(text: &str, redacted: &Redacted) -> bool {
    let mut previous_end = 0;
    redacted.findings.iter().all(|finding| {
        let ok = previous_end <= finding.start
            && finding.start <= finding.end
            && finding.end <= text.len()
            && text.is_char_boundary(finding.start)
            && text.is_char_boundary(finding.end);
        previous_end = finding.end;
        ok
    })
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|elapsed| i64::try_from(elapsed.as_millis()).ok())
        .unwrap_or(0)
}

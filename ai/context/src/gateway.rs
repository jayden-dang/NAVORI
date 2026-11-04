use std::fmt;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use futures::future::BoxFuture;
use navori_redact::{Redacted, redact_text};
use navori_secrets::{Secret, SecretKey, SecretStore};
use tokio::task::spawn_blocking;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AuditError;

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
            audit,
            provider,
            shield,
        }
    }

    pub async fn ask(&self, model: &str, text: &str) -> Result<String, AskError> {
        let secret = self.read_key().await?;

        let Some(redacted) = self.inspect(text) else {
            let _ = self.write_row(Outcome::Blocked, 0).await;
            return Err(AskError::Shield);
        };

        let count = u32::try_from(redacted.findings.len()).unwrap_or(u32::MAX);
        if self.write_row(Outcome::Authorized, count).await.is_err() {
            tracing::warn!("context audit write failed");
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
        match spawn_blocking(move || secrets.get(&key)).await {
            Ok(Ok(Some(secret))) => Ok(secret),
            Ok(Ok(None)) => Err(AskError::NoKey),
            Ok(Err(error)) => Err(AskError::Keychain(error.to_string())),
            Err(_) => Err(AskError::Keychain(
                "the Keychain call did not finish".to_owned(),
            )),
        }
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
            .unwrap_or(Err(AuditError))
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

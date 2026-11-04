//! The AI wiring of the composition root: the lazily opened Keychain handle and the
//! Profile-to-service mapping.

use std::fmt;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;

use futures::future::BoxFuture;
use navori_config::Profile;
use navori_context::{
    AskError, AuditError, AuditRow, AuditSink, Gateway, Outcome, SendError, Sender,
};
use navori_llm::{
    ChatRequest, DEEPSEEK_BASE_URL, DEEPSEEK_PROVIDER, LlmProvider, Message, OpenAiCompat, Role,
};
use navori_protocol::rpc;
use navori_redact::redact_text;
use navori_secrets::{
    Deleted, KeychainService, KeychainStore, OpenError, Secret, SecretKey, SecretStore,
};
use navori_store::{Migration, Store};
use tokio::sync::OnceCell;

use crate::CoreError;

type OpenSecrets =
    Arc<dyn Fn(KeychainService) -> Result<Arc<dyn SecretStore>, OpenError> + Send + Sync>;
type MakeSender = Arc<dyn Fn() -> Arc<dyn Sender> + Send + Sync>;

/// The start-time factories of the AI side. Tests inject a fake Keychain and a fake sender.
#[derive(Clone)]
pub struct AiDeps {
    pub open_secrets: OpenSecrets,
    pub make_sender: MakeSender,
}

impl fmt::Debug for AiDeps {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AiDeps").finish_non_exhaustive()
    }
}

impl AiDeps {
    pub fn production() -> Self {
        Self {
            open_secrets: Arc::new(|service| {
                let store: Arc<dyn SecretStore> = Arc::new(KeychainStore::open(service)?);
                Ok(store)
            }),
            make_sender: Arc::new(|| {
                Arc::new(LlmSender::new(OpenAiCompat::new(DEEPSEEK_BASE_URL)))
            }),
        }
    }
}

#[derive(Debug)]
pub struct LlmSender {
    provider: OpenAiCompat,
}

impl LlmSender {
    pub fn new(provider: OpenAiCompat) -> Self {
        Self { provider }
    }
}

impl Sender for LlmSender {
    fn send<'a>(
        &'a self,
        key: &'a Secret,
        model: &'a str,
        text: &'a str,
    ) -> BoxFuture<'a, Result<String, SendError>> {
        Box::pin(async move {
            let request = ChatRequest {
                model: model.to_owned(),
                messages: vec![Message {
                    role: Role::User,
                    text: text.to_owned(),
                }],
            };
            self.provider
                .complete(request, key)
                .await
                .map(|reply| reply.text)
                .map_err(|error| SendError(error.to_string()))
        })
    }
}

const AUDIT_OWNER: &str = "context";

const AUDIT_MIGRATIONS: &[Migration] = &[Migration {
    version: 1,
    name: "context_audit",
    sql: "
        CREATE TABLE context_audit (
          id              INTEGER PRIMARY KEY AUTOINCREMENT,
          ts_ms           INTEGER NOT NULL,
          provider        TEXT NOT NULL,
          kinds           TEXT NOT NULL,
          redacted_count  INTEGER NOT NULL,
          outcome         TEXT NOT NULL CHECK (outcome IN ('authorized','blocked'))
        );
    ",
}];

#[derive(Debug)]
pub struct StoreAudit {
    store: Store,
    migrated: std::sync::Mutex<bool>,
}

impl StoreAudit {
    pub fn new(store: Store) -> Self {
        Self {
            store,
            migrated: std::sync::Mutex::new(false),
        }
    }

    fn ensure_table(&self) -> Result<(), AuditError> {
        let mut migrated = self
            .migrated
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if !*migrated {
            self.store
                .migrate(AUDIT_OWNER, AUDIT_MIGRATIONS)
                .map_err(|_| AuditError)?;
            *migrated = true;
        }
        Ok(())
    }
}

impl AuditSink for StoreAudit {
    fn record(&self, row: AuditRow) -> Result<(), AuditError> {
        self.ensure_table()?;
        let (provider, kinds) = catch_unwind(AssertUnwindSafe(|| {
            (
                redact_text(&row.provider).text,
                redact_text(&row.kinds.join(",")).text,
            )
        }))
        .map_err(|_| AuditError)?;
        let outcome = match row.outcome {
            Outcome::Authorized => "authorized",
            Outcome::Blocked => "blocked",
        };
        self.store
            .with_conn(|conn| {
                conn.execute(
                    "INSERT INTO context_audit (ts_ms, provider, kinds, redacted_count, outcome) VALUES (?1, ?2, ?3, ?4, ?5)",
                    (row.ts_ms, provider, kinds, row.redacted_count, outcome),
                )
            })
            .map(|_| ())
            .map_err(|_| AuditError)
    }
}

pub fn ask_error_to_rpc(err: AskError) -> rpc::Error {
    let code = match &err {
        AskError::NoKey => rpc::code::ASK_NO_KEY,
        AskError::Keychain(_) => rpc::code::ASK_KEYCHAIN,
        AskError::Shield | AskError::Audit => rpc::code::ASK_BLOCKED,
        AskError::Send(_) => rpc::code::ASK_PROVIDER,
    };
    rpc::Error {
        code,
        message: err.to_string(),
        data: None,
    }
}

pub fn service_for(profile: Profile) -> KeychainService {
    match profile {
        Profile::Release => KeychainService::RELEASE,
        Profile::Dev => KeychainService::DEV,
    }
}

pub(crate) struct AiRuntime {
    profile: Profile,
    deps: AiDeps,
    store: Store,
    secrets: OnceCell<Arc<dyn SecretStore>>,
    gateway: OnceCell<Arc<Gateway>>,
}

impl fmt::Debug for AiRuntime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AiRuntime")
            .field("profile", &self.profile)
            .finish_non_exhaustive()
    }
}

fn secrets_error(message: impl ToString) -> CoreError {
    CoreError::Secrets {
        message: message.to_string(),
    }
}

fn did_not_finish(_: tokio::task::JoinError) -> CoreError {
    secrets_error("the Keychain call did not finish")
}

impl AiRuntime {
    pub(crate) fn new(profile: Profile, deps: AiDeps, store: Store) -> Self {
        Self {
            profile,
            deps,
            store,
            secrets: OnceCell::new(),
            gateway: OnceCell::new(),
        }
    }

    pub(crate) async fn gateway(&self) -> Result<Arc<Gateway>, AskError> {
        self.gateway
            .get_or_try_init(|| async {
                let secrets = self
                    .secrets()
                    .await
                    .map_err(|error| AskError::Keychain(error.to_string()))?;
                let sender = (self.deps.make_sender)();
                let audit = Arc::new(StoreAudit::new(self.store.clone()));
                Ok(Arc::new(Gateway::new(
                    secrets,
                    slot().map_err(|error| AskError::Keychain(error.to_string()))?,
                    sender,
                    audit,
                    DEEPSEEK_PROVIDER,
                )))
            })
            .await
            .cloned()
    }

    /// Opens the Keychain on first use. A failed open is not remembered, so the next call retries.
    async fn secrets(&self) -> Result<Arc<dyn SecretStore>, CoreError> {
        self.secrets
            .get_or_try_init(|| async {
                let open = self.deps.open_secrets.clone();
                let service = service_for(self.profile);
                tokio::task::spawn_blocking(move || open(service))
                    .await
                    .map_err(did_not_finish)?
                    .map_err(secrets_error)
            })
            .await
            .cloned()
    }

    pub(crate) async fn save_key(&self, key: String) -> Result<(), CoreError> {
        let trimmed = key.trim();
        if trimmed.is_empty() {
            return Err(secrets_error("Nothing was entered."));
        }
        let secret = Secret::new(trimmed);
        let slot = slot()?;
        let store = self.secrets().await?;
        tokio::task::spawn_blocking(move || store.set(&slot, &secret))
            .await
            .map_err(did_not_finish)?
            .map_err(secrets_error)
    }

    pub(crate) async fn remove_key(&self) -> Result<bool, CoreError> {
        let slot = slot()?;
        let store = self.secrets().await?;
        let deleted = tokio::task::spawn_blocking(move || store.delete(&slot))
            .await
            .map_err(did_not_finish)?
            .map_err(secrets_error)?;
        Ok(deleted == Deleted::Removed)
    }
}

fn slot() -> Result<SecretKey, CoreError> {
    SecretKey::new("llm-provider", DEEPSEEK_PROVIDER).map_err(secrets_error_key)
}

fn secrets_error_key(_: navori_secrets::KeyError) -> CoreError {
    secrets_error("the provider key slot is not valid")
}

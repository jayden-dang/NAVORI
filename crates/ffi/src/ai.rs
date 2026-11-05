//! The AI wiring of the composition root: the lazily opened Keychain handle and the
//! Profile-to-service mapping.

use std::fmt;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;

use futures::future::BoxFuture;
use navori_agent::{
    Agent, ConversationId, ConversationLog, ConversationStore, Entry, Limits, Origin, RecordError,
    RecordedMessage, TurnId, TurnStartError,
};
use navori_config::Profile;
use navori_context::{
    AskError, AuditError, AuditRow, AuditSink, ChatSender, Gateway, Outcome, SendError, Sender,
};
use navori_kernel::EventBus;
use navori_llm::{
    ChatReply, ChatRequest, DEEPSEEK_BASE_URL, DEEPSEEK_PROVIDER, LlmError, LlmProvider, Message,
    OpenAiCompat, Usage,
};
use navori_protocol::rpc;
use navori_redact::redact_text;
use navori_secrets::{
    Deleted, KeychainService, KeychainStore, OpenError, Secret, SecretKey, SecretStore,
};
use navori_store::{Migration, Store};
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use tokio::sync::OnceCell;

use crate::CoreError;

type OpenSecrets =
    Arc<dyn Fn(KeychainService) -> Result<Arc<dyn SecretStore>, OpenError> + Send + Sync>;
type MakeSender = Arc<dyn Fn() -> Arc<dyn Sender> + Send + Sync>;
type MakeProvider = Arc<dyn Fn() -> Arc<dyn LlmProvider> + Send + Sync>;
type MakeConversations = Arc<dyn Fn(Store) -> Arc<dyn ConversationStore> + Send + Sync>;

/// The start-time factories of the AI side. Tests inject a fake Keychain and a fake sender.
#[derive(Clone)]
pub struct AiDeps {
    pub open_secrets: OpenSecrets,
    pub make_sender: MakeSender,
    pub make_provider: MakeProvider,
    pub make_conversations: MakeConversations,
}

impl fmt::Debug for AiDeps {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AiDeps").finish_non_exhaustive()
    }
}

impl AiDeps {
    pub fn production() -> Self {
        let provider = Arc::new(OpenAiCompat::new(DEEPSEEK_BASE_URL));
        Self {
            open_secrets: Arc::new(|service| {
                let store: Arc<dyn SecretStore> = Arc::new(KeychainStore::open(service)?);
                Ok(store)
            }),
            make_sender: {
                let provider = provider.clone();
                Arc::new(move || Arc::new(LlmSender::new(provider.clone())))
            },
            make_provider: Arc::new(move || {
                let shared: Arc<dyn LlmProvider> = provider.clone();
                shared
            }),
            make_conversations: Arc::new(|store| Arc::new(StoreConversations::new(store))),
        }
    }
}

#[derive(Debug)]
pub struct LlmSender {
    provider: Arc<OpenAiCompat>,
}

impl LlmSender {
    pub fn new(provider: Arc<OpenAiCompat>) -> Self {
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
                messages: vec![Message::User {
                    text: text.to_owned(),
                }],
                tools: vec![],
            };
            self.provider
                .complete(request, key)
                .await
                .and_then(|reply| {
                    reply
                        .text
                        .filter(|text| !text.is_empty())
                        .ok_or(LlmError::Malformed)
                })
                .map_err(|error| {
                    tracing::warn!(
                        provider = DEEPSEEK_PROVIDER,
                        kind = error.kind(),
                        "provider request failed"
                    );
                    SendError(error.to_string())
                })
        })
    }
}

#[derive(Clone)]
pub struct LlmChatSender {
    provider: Arc<dyn LlmProvider>,
}

impl fmt::Debug for LlmChatSender {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LlmChatSender").finish_non_exhaustive()
    }
}

impl LlmChatSender {
    pub fn new(provider: Arc<dyn LlmProvider>) -> Self {
        Self { provider }
    }
}

impl ChatSender for LlmChatSender {
    fn send<'a>(
        &'a self,
        key: &'a Secret,
        request: ChatRequest,
    ) -> BoxFuture<'a, Result<ChatReply, SendError>> {
        Box::pin(async move {
            self.provider.complete(request, key).await.map_err(|error| {
                tracing::warn!(
                    provider = DEEPSEEK_PROVIDER,
                    kind = error.kind(),
                    "provider request failed"
                );
                SendError(error.to_string())
            })
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
                .map_err(|_| AuditError::Migration)?;
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
        .map_err(|_| AuditError::ShieldPass)?;
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
            .map_err(|_| AuditError::Write)
    }
}

const SESSION_OWNER: &str = "ai_session";

const SESSION_MIGRATIONS: &[Migration] = &[Migration {
    version: 1,
    name: "ai_session_tables",
    sql: "
        CREATE TABLE ai_session_conversation (
          id          TEXT PRIMARY KEY,
          created_ms  INTEGER NOT NULL
        );
        CREATE TABLE ai_session_turn (
          id               TEXT PRIMARY KEY,
          conversation_id  TEXT NOT NULL REFERENCES ai_session_conversation(id),
          seq              INTEGER NOT NULL,
          origin           TEXT NOT NULL,
          model            TEXT NOT NULL,
          started_ms       INTEGER NOT NULL,
          ended_ms         INTEGER,
          end_reason       TEXT,
          UNIQUE (conversation_id, seq)
        );
        CREATE TABLE ai_session_step (
          turn_id     TEXT NOT NULL REFERENCES ai_session_turn(id),
          seq         INTEGER NOT NULL,
          started_ms  INTEGER NOT NULL,
          ended_ms    INTEGER,
          PRIMARY KEY (turn_id, seq)
        );
        CREATE TABLE ai_session_message (
          turn_id      TEXT NOT NULL REFERENCES ai_session_turn(id),
          seq          INTEGER NOT NULL,
          step         INTEGER,
          kind         TEXT NOT NULL,
          call_id      TEXT,
          body         TEXT NOT NULL,
          redactions   INTEGER NOT NULL,
          created_ms   INTEGER NOT NULL,
          PRIMARY KEY (turn_id, seq)
        );
        CREATE UNIQUE INDEX ai_session_one_result_per_call
          ON ai_session_message (turn_id, call_id) WHERE kind = 'tool_result';
        CREATE TABLE ai_session_usage (
          turn_id             TEXT NOT NULL REFERENCES ai_session_turn(id),
          step                INTEGER NOT NULL,
          reported            INTEGER NOT NULL CHECK (reported IN (0,1)),
          input_tokens        INTEGER,
          output_tokens       INTEGER,
          cache_hit_tokens    INTEGER,
          cache_miss_tokens   INTEGER,
          PRIMARY KEY (turn_id, step)
        );
        CREATE INDEX ai_session_turn_by_conversation
          ON ai_session_turn (conversation_id, seq);
    ",
}];

#[derive(Debug)]
pub struct StoreConversations {
    store: Store,
    migrated: std::sync::Mutex<bool>,
}

impl StoreConversations {
    pub fn new(store: Store) -> Self {
        Self {
            store,
            migrated: std::sync::Mutex::new(false),
        }
    }

    fn ensure_table(&self) -> Result<(), RecordError> {
        let mut migrated = self
            .migrated
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if !*migrated {
            self.store
                .migrate(SESSION_OWNER, SESSION_MIGRATIONS)
                .map_err(|error| {
                    tracing::error!(owner = SESSION_OWNER, %error, "conversation tables could not be migrated");
                    RecordError::Migration
                })?;
            *migrated = true;
        }
        Ok(())
    }
}

fn count(value: u64) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

fn write_error(error: rusqlite::Error) -> RecordError {
    match &error {
        rusqlite::Error::SqliteFailure(failure, _)
            if failure.extended_code == rusqlite::ffi::SQLITE_CONSTRAINT_UNIQUE
                || failure.extended_code == rusqlite::ffi::SQLITE_CONSTRAINT_PRIMARYKEY =>
        {
            RecordError::Duplicate
        }
        _ => {
            tracing::warn!(code = sqlite_code(&error), "conversation write failed");
            RecordError::Write
        }
    }
}

fn sqlite_code(error: &rusqlite::Error) -> i32 {
    match error {
        rusqlite::Error::SqliteFailure(failure, _) => failure.extended_code,
        _ => -1,
    }
}

fn insert(tx: &Transaction<'_>, entry: &Entry) -> rusqlite::Result<()> {
    match entry {
        Entry::ConversationStarted { id, ts_ms } => tx.execute(
            "INSERT INTO ai_session_conversation (id, created_ms) VALUES (?1, ?2)",
            params![id.to_string(), ts_ms],
        ),
        Entry::TurnStarted {
            id,
            conversation,
            seq,
            origin,
            model,
            ts_ms,
        } => tx.execute(
            "INSERT INTO ai_session_turn (id, conversation_id, seq, origin, model, started_ms) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                id.to_string(),
                conversation.to_string(),
                seq,
                origin.as_str(),
                model,
                ts_ms
            ],
        ),
        Entry::StepStarted { turn, seq, ts_ms } => tx.execute(
            "INSERT INTO ai_session_step (turn_id, seq, started_ms) VALUES (?1, ?2, ?3)",
            params![turn.to_string(), seq, ts_ms],
        ),
        Entry::Message {
            turn,
            seq,
            step,
            body,
            redactions,
            ts_ms,
        } => {
            let (kind, call_id) = match body {
                RecordedMessage::Input { .. } => ("input", None),
                RecordedMessage::Assistant { .. } => ("assistant", None),
                RecordedMessage::ToolResult { call_id, .. } => ("tool_result", Some(call_id)),
            };
            tx.execute(
                "INSERT INTO ai_session_message (turn_id, seq, step, kind, call_id, body, redactions, created_ms) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    turn.to_string(),
                    seq,
                    step,
                    kind,
                    call_id,
                    body.to_body(),
                    redactions,
                    ts_ms
                ],
            )
        }
        Entry::Usage { turn, step, usage } => match usage {
            Some(Usage {
                input,
                output,
                cache_hit,
                cache_miss,
            }) => tx.execute(
                "INSERT INTO ai_session_usage (turn_id, step, reported, input_tokens, output_tokens, cache_hit_tokens, cache_miss_tokens) VALUES (?1, ?2, 1, ?3, ?4, ?5, ?6)",
                params![
                    turn.to_string(),
                    step,
                    count(*input),
                    count(*output),
                    cache_hit.map(count),
                    cache_miss.map(count)
                ],
            ),
            None => tx.execute(
                "INSERT INTO ai_session_usage (turn_id, step, reported) VALUES (?1, ?2, 0)",
                params![turn.to_string(), step],
            ),
        },
        Entry::StepEnded { turn, seq, ts_ms } => tx.execute(
            "UPDATE ai_session_step SET ended_ms = ?3 WHERE turn_id = ?1 AND seq = ?2",
            params![turn.to_string(), seq, ts_ms],
        ),
        Entry::TurnEnded { turn, end, ts_ms } => tx.execute(
            "UPDATE ai_session_turn SET ended_ms = ?2, end_reason = ?3 WHERE id = ?1",
            params![turn.to_string(), ts_ms, end],
        ),
    }
    .map(|_| ())
}

type TurnRow = (
    String,
    u32,
    String,
    String,
    i64,
    Option<i64>,
    Option<String>,
);

fn read_entries(
    conn: &Connection,
    conversation: &ConversationId,
) -> Result<Vec<Entry>, RecordError> {
    let read = |error: rusqlite::Error| {
        tracing::warn!(code = sqlite_code(&error), "conversation read failed");
        RecordError::Read
    };
    let id = conversation.to_string();
    let created: Option<i64> = conn
        .query_row(
            "SELECT created_ms FROM ai_session_conversation WHERE id = ?1",
            [&id],
            |row| row.get(0),
        )
        .optional()
        .map_err(read)?;
    let created = created.ok_or(RecordError::UnknownConversation)?;
    let mut entries = vec![Entry::ConversationStarted {
        id: *conversation,
        ts_ms: created,
    }];
    let mut turns = conn
        .prepare("SELECT id, seq, origin, model, started_ms, ended_ms, end_reason FROM ai_session_turn WHERE conversation_id = ?1 ORDER BY seq")
        .map_err(read)?;
    let turn_rows: Vec<TurnRow> = turns
        .query_map([&id], |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
                row.get(5)?,
                row.get(6)?,
            ))
        })
        .map_err(read)?
        .collect::<rusqlite::Result<_>>()
        .map_err(read)?;
    for (turn_id, seq, origin, model, started, ended, end_reason) in turn_rows {
        let turn: TurnId = turn_id.parse()?;
        if origin != Origin::User.as_str() {
            return Err(RecordError::Read);
        }
        entries.push(Entry::TurnStarted {
            id: turn,
            conversation: *conversation,
            seq,
            origin: Origin::User,
            model,
            ts_ms: started,
        });
        let mut steps = conn
            .prepare("SELECT seq, started_ms, ended_ms FROM ai_session_step WHERE turn_id = ?1 ORDER BY seq")
            .map_err(read)?;
        let step_rows: Vec<(u32, i64, Option<i64>)> = steps
            .query_map([&turn_id], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })
            .map_err(read)?
            .collect::<rusqlite::Result<_>>()
            .map_err(read)?;
        for (step, started, ended) in step_rows {
            entries.push(Entry::StepStarted {
                turn,
                seq: step,
                ts_ms: started,
            });
            if let Some(ended) = ended {
                entries.push(Entry::StepEnded {
                    turn,
                    seq: step,
                    ts_ms: ended,
                });
            }
        }
        let mut messages = conn
            .prepare("SELECT seq, step, body, redactions, created_ms FROM ai_session_message WHERE turn_id = ?1 ORDER BY seq")
            .map_err(read)?;
        let message_rows: Vec<(u32, Option<u32>, String, u32, i64)> = messages
            .query_map([&turn_id], |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            })
            .map_err(read)?
            .collect::<rusqlite::Result<_>>()
            .map_err(read)?;
        for (seq, step, body, redactions, created) in message_rows {
            entries.push(Entry::Message {
                turn,
                seq,
                step,
                body: RecordedMessage::from_body(&body)?,
                redactions,
                ts_ms: created,
            });
        }
        if let (Some(ended), Some(end)) = (ended, end_reason) {
            entries.push(Entry::TurnEnded {
                turn,
                end,
                ts_ms: ended,
            });
        }
    }
    Ok(entries)
}

impl ConversationStore for StoreConversations {
    fn append(&self, entries: &[Entry]) -> Result<(), RecordError> {
        self.ensure_table()?;
        self.store.with_conn(|conn| {
            let tx = conn.unchecked_transaction().map_err(write_error)?;
            for entry in entries {
                insert(&tx, entry).map_err(write_error)?;
            }
            tx.commit().map_err(write_error)
        })
    }

    fn load(&self, conversation: &ConversationId) -> Result<ConversationLog, RecordError> {
        self.ensure_table()?;
        let entries = self
            .store
            .with_conn(|conn| read_entries(conn, conversation))?;
        ConversationLog::from_entries(entries)
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
    bus: EventBus,
    secrets: OnceCell<Arc<dyn SecretStore>>,
    gateway: OnceCell<Arc<Gateway>>,
    agent: OnceCell<Arc<Agent>>,
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
    tracing::warn!("keychain call did not finish");
    secrets_error("the Keychain call did not finish")
}

impl AiRuntime {
    pub(crate) fn new(profile: Profile, deps: AiDeps, store: Store, bus: EventBus) -> Self {
        Self {
            profile,
            deps,
            store,
            bus,
            secrets: OnceCell::new(),
            gateway: OnceCell::new(),
            agent: OnceCell::new(),
        }
    }

    pub(crate) async fn agent(&self) -> Result<Arc<Agent>, TurnStartError> {
        self.agent
            .get_or_try_init(|| async {
                let secrets = self
                    .secrets()
                    .await
                    .map_err(|error| TurnStartError::Keychain(error.to_string()))?;
                let chat: Arc<dyn ChatSender> =
                    Arc::new(LlmChatSender::new((self.deps.make_provider)()));
                let audit = Arc::new(StoreAudit::new(self.store.clone()));
                let gateway = Gateway::new(
                    secrets,
                    slot().map_err(|error| TurnStartError::Keychain(error.to_string()))?,
                    (self.deps.make_sender)(),
                    audit,
                    DEEPSEEK_PROVIDER,
                )
                .with_chat_sender(chat);
                let conversations = (self.deps.make_conversations)(self.store.clone());
                Ok(Arc::new(Agent::new(
                    Arc::new(gateway),
                    conversations,
                    self.bus.clone(),
                    Limits::default(),
                )))
            })
            .await
            .cloned()
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
                    .map_err(|error| {
                        tracing::warn!("keychain open failed");
                        secrets_error(error)
                    })
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
    tracing::warn!("provider key slot invalid");
    secrets_error("the provider key slot is not valid")
}

//! The AI wiring of the composition root: the lazily opened Keychain handle and the
//! Profile-to-service mapping.

use std::fmt;
use std::sync::Arc;

use futures::future::BoxFuture;
use navori_agent::{Agent, ConversationStore, Limits, StoreConversations, TurnStartError};
use navori_config::Profile;
use navori_context::{AskError, ChatSender, Gateway, SendError, Sender, StoreAudit};
use navori_kernel::EventBus;
use navori_llm::{
    ChatReply, ChatRequest, DEEPSEEK_BASE_URL, DEEPSEEK_PROVIDER, LlmError, LlmProvider, Message,
    OpenAiCompat,
};
use navori_protocol::rpc;
use navori_secrets::{
    Deleted, KeychainService, KeychainStore, OpenError, Secret, SecretKey, SecretStore,
};
use navori_store::Store;
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
                let gateway = self
                    .build_gateway()
                    .await
                    .map_err(TurnStartError::Keychain)?;
                let chat: Arc<dyn ChatSender> =
                    Arc::new(LlmChatSender::new((self.deps.make_provider)()));
                let conversations = (self.deps.make_conversations)(self.store.clone());
                Ok(Arc::new(Agent::new(
                    Arc::new(gateway.with_chat_sender(chat)),
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
                self.build_gateway()
                    .await
                    .map(Arc::new)
                    .map_err(AskError::Keychain)
            })
            .await
            .cloned()
    }

    /// A fresh Gateway over the shared Keychain, the Store audit and the profile's sender.
    async fn build_gateway(&self) -> Result<Gateway, String> {
        let secrets = self.secrets().await.map_err(|error| error.to_string())?;
        let sender = (self.deps.make_sender)();
        let audit = Arc::new(StoreAudit::new(self.store.clone()));
        Ok(Gateway::new(
            secrets,
            slot().map_err(|error| error.to_string())?,
            sender,
            audit,
            DEEPSEEK_PROVIDER,
        ))
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

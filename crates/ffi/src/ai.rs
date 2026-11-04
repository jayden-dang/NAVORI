//! The AI wiring of the composition root: the lazily opened Keychain handle and the
//! Profile-to-service mapping.

use std::fmt;
use std::sync::Arc;

use futures::future::BoxFuture;
use navori_config::Profile;
use navori_context::{SendError, Sender};
use navori_secrets::{
    Deleted, KeychainService, KeychainStore, OpenError, Secret, SecretKey, SecretStore,
};
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
            make_sender: Arc::new(|| Arc::new(Unwired)),
        }
    }
}

/// Stands in for the provider client until the Gateway is wired to it.
struct Unwired;

impl Sender for Unwired {
    fn send<'a>(
        &'a self,
        _: &'a Secret,
        _: &'a str,
        _: &'a str,
    ) -> BoxFuture<'a, Result<String, SendError>> {
        Box::pin(async { Err(SendError("no provider is connected yet".to_owned())) })
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
    secrets: OnceCell<Arc<dyn SecretStore>>,
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
    pub(crate) fn new(profile: Profile, deps: AiDeps) -> Self {
        Self {
            profile,
            deps,
            secrets: OnceCell::new(),
        }
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
    SecretKey::new("llm-provider", navori_llm::DEEPSEEK_PROVIDER).map_err(secrets_error_key)
}

fn secrets_error_key(_: navori_secrets::KeyError) -> CoreError {
    secrets_error("the provider key slot is not valid")
}

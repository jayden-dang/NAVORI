use std::fmt;
use std::sync::Arc;

use keyring_core::api::CredentialStoreApi;
use keyring_core::{Entry, Error};

#[cfg(target_os = "macos")]
use crate::OpenError;
use crate::{Deleted, KeychainService, Operation, Secret, SecretError, SecretKey, SecretStore};

/// The Keychain-backed store. A backend error is matched by variant and dropped unformatted:
/// `keyring_core::Error::BadEncoding` carries the stored bytes and prints them in `Debug`.
pub struct KeychainStore {
    service: KeychainService,
    backend: Arc<dyn CredentialStoreApi + Send + Sync>,
}

impl KeychainStore {
    /// Open the user's regular (login) Keychain for `service`.
    #[cfg(target_os = "macos")]
    pub fn open(service: KeychainService) -> Result<Self, OpenError> {
        let backend = apple_native_keyring_store::keychain::Store::new().map_err(|_| OpenError)?;
        Ok(Self { service, backend })
    }

    /// Build a store over any `keyring-core` backend. Test seam, available with feature `fake`.
    #[cfg(feature = "fake")]
    pub fn with_backend(
        service: KeychainService,
        backend: Arc<dyn CredentialStoreApi + Send + Sync>,
    ) -> Self {
        Self { service, backend }
    }

    fn fail(&self, operation: Operation, key: &SecretKey) -> SecretError {
        SecretError::new(operation, self.service, key)
    }

    /// The one place a backend error is mapped: `NoEntry` is `Ok(None)`, every other error is
    /// dropped unformatted and becomes our `SecretError`.
    fn call<T>(
        &self,
        operation: Operation,
        key: &SecretKey,
        run: impl FnOnce(&Entry) -> Result<T, Error>,
    ) -> Result<Option<T>, SecretError> {
        let entry = self
            .backend
            .build(self.service.as_str(), &key.account(), None)
            .map_err(|_| self.fail(operation, key))?;
        match run(&entry) {
            Ok(value) => Ok(Some(value)),
            Err(Error::NoEntry) => Ok(None),
            Err(_) => Err(self.fail(operation, key)),
        }
    }
}

impl fmt::Debug for KeychainStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("KeychainStore")
            .field("service", &self.service)
            .finish()
    }
}

impl SecretStore for KeychainStore {
    fn set(&self, key: &SecretKey, value: &Secret) -> Result<(), SecretError> {
        self.call(Operation::Set, key, |entry| {
            entry.set_password(value.expose())
        })?
        .map(drop)
        .ok_or_else(|| self.fail(Operation::Set, key))
    }

    fn get(&self, key: &SecretKey) -> Result<Option<Secret>, SecretError> {
        Ok(self
            .call(Operation::Get, key, Entry::get_password)?
            .map(Secret::new))
    }

    fn exists(&self, key: &SecretKey) -> Result<bool, SecretError> {
        Ok(self
            .call(Operation::Exists, key, Entry::get_credential)?
            .is_some())
    }

    fn delete(&self, key: &SecretKey) -> Result<Deleted, SecretError> {
        Ok(
            match self.call(Operation::Delete, key, Entry::delete_credential)? {
                Some(()) => Deleted::Removed,
                None => Deleted::WasAbsent,
            },
        )
    }
}

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

    fn entry(&self, operation: Operation, key: &SecretKey) -> Result<Entry, SecretError> {
        self.backend
            .build(self.service.as_str(), &key.account(), None)
            .map_err(|_| SecretError::new(operation, self.service, key))
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
        self.entry(Operation::Set, key)?
            .set_password(value.expose())
            .map_err(|_| SecretError::new(Operation::Set, self.service, key))
    }

    fn get(&self, key: &SecretKey) -> Result<Option<Secret>, SecretError> {
        match self.entry(Operation::Get, key)?.get_password() {
            Ok(text) => Ok(Some(Secret::new(text))),
            Err(Error::NoEntry) => Ok(None),
            Err(_) => Err(SecretError::new(Operation::Get, self.service, key)),
        }
    }

    fn exists(&self, key: &SecretKey) -> Result<bool, SecretError> {
        match self.entry(Operation::Exists, key)?.get_credential() {
            Ok(_) => Ok(true),
            Err(Error::NoEntry) => Ok(false),
            Err(_) => Err(SecretError::new(Operation::Exists, self.service, key)),
        }
    }

    fn delete(&self, key: &SecretKey) -> Result<Deleted, SecretError> {
        match self.entry(Operation::Delete, key)?.delete_credential() {
            Ok(()) => Ok(Deleted::Removed),
            Err(Error::NoEntry) => Ok(Deleted::WasAbsent),
            Err(_) => Err(SecretError::new(Operation::Delete, self.service, key)),
        }
    }
}

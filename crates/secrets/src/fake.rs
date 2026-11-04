//! A test double for the Keychain, enabled by feature `fake` and never linked into the app.
use std::collections::HashMap;
use std::fmt;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use crate::{Deleted, KeychainService, Operation, Secret, SecretError, SecretKey, SecretStore};

#[derive(Default)]
struct State {
    items: HashMap<(String, String), Secret>,
    failures: Vec<Operation>,
}

/// A shared in-memory vault keyed by (service, account). Clones share the vault.
#[derive(Clone, Default)]
pub struct FakeKeychain {
    state: Arc<Mutex<State>>,
}

impl FakeKeychain {
    pub fn new() -> Self {
        Self::default()
    }

    /// Make the next call of `operation` on any store over this vault fail.
    pub fn fail_next(&self, operation: Operation) {
        self.lock().failures.push(operation);
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl fmt::Debug for FakeKeychain {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FakeKeychain")
            .field("items", &self.lock().items.len())
            .finish()
    }
}

/// A [`SecretStore`] over a [`FakeKeychain`] for one service.
pub struct MemoryStore {
    service: KeychainService,
    vault: FakeKeychain,
}

impl MemoryStore {
    pub fn new(service: KeychainService, vault: &FakeKeychain) -> Self {
        Self {
            service,
            vault: vault.clone(),
        }
    }

    fn slot(&self, key: &SecretKey) -> (String, String) {
        (self.service.as_str().to_owned(), key.account())
    }

    fn check(
        &self,
        state: &mut State,
        operation: Operation,
        key: &SecretKey,
    ) -> Result<(), SecretError> {
        match state
            .failures
            .iter()
            .position(|queued| *queued == operation)
        {
            Some(index) => {
                state.failures.remove(index);
                Err(SecretError::new(operation, self.service, key))
            }
            None => Ok(()),
        }
    }
}

impl fmt::Debug for MemoryStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MemoryStore")
            .field("service", &self.service)
            .finish()
    }
}

impl SecretStore for MemoryStore {
    fn set(&self, key: &SecretKey, value: &Secret) -> Result<(), SecretError> {
        let mut state = self.vault.lock();
        self.check(&mut state, Operation::Set, key)?;
        state
            .items
            .insert(self.slot(key), Secret::new(value.expose()));
        Ok(())
    }

    fn get(&self, key: &SecretKey) -> Result<Option<Secret>, SecretError> {
        let mut state = self.vault.lock();
        self.check(&mut state, Operation::Get, key)?;
        Ok(state
            .items
            .get(&self.slot(key))
            .map(|found| Secret::new(found.expose())))
    }

    fn exists(&self, key: &SecretKey) -> Result<bool, SecretError> {
        let mut state = self.vault.lock();
        self.check(&mut state, Operation::Exists, key)?;
        Ok(state.items.contains_key(&self.slot(key)))
    }

    fn delete(&self, key: &SecretKey) -> Result<Deleted, SecretError> {
        let mut state = self.vault.lock();
        self.check(&mut state, Operation::Delete, key)?;
        Ok(if state.items.remove(&self.slot(key)).is_some() {
            Deleted::Removed
        } else {
            Deleted::WasAbsent
        })
    }
}

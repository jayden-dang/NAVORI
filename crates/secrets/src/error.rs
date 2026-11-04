use std::fmt;

use crate::{KeychainService, SecretKey};

/// Which store call failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operation {
    Set,
    Get,
    Exists,
    Delete,
}

impl fmt::Display for Operation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Set => "set",
            Self::Get => "get",
            Self::Exists => "exists",
            Self::Delete => "delete",
        })
    }
}

/// A Keychain call failed. It names the call, the service, the kind and the name, and never
/// a value or any text from the backend. The cause is not told apart, so the fixed advice
/// lists the possible causes (locked, access denied, unreadable item) instead of guessing one.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error(
    "{operation} failed for {kind}/{name} in service {service}; the Keychain could not complete the call (it may be locked, access may be denied, or the item may be unreadable), so check Keychain Access and retry"
)]
pub struct SecretError {
    operation: Operation,
    service: &'static str,
    kind: String,
    name: String,
}

impl SecretError {
    pub fn operation(&self) -> Operation {
        self.operation
    }

    pub(crate) fn new(operation: Operation, service: KeychainService, key: &SecretKey) -> Self {
        Self {
            operation,
            service: service.as_str(),
            kind: key.kind().to_owned(),
            name: key.name().to_owned(),
        }
    }
}

/// The Keychain store could not be built. The cause is not told apart, and no backend text
/// is carried.
#[derive(Debug, thiserror::Error)]
#[error(
    "the Keychain store could not be opened; the Keychain may be unavailable, so check Keychain Access and retry"
)]
pub struct OpenError;

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
/// a value or any text from the backend.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error(
    "{operation} failed for {kind}/{name} in service {service}; unlock the Keychain, allow Navori access and retry"
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

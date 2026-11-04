use crate::{Secret, SecretError, SecretKey};

/// What `delete` did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Deleted {
    Removed,
    WasAbsent,
}

/// Navori's secrets. Synchronous: callers keep these calls off the actor thread and read lazily.
/// "Nothing stored" is an outcome (`None`, `false`, `WasAbsent`), never an error.
pub trait SecretStore: Send + Sync {
    fn set(&self, key: &SecretKey, value: &Secret) -> Result<(), SecretError>;
    fn get(&self, key: &SecretKey) -> Result<Option<Secret>, SecretError>;
    fn exists(&self, key: &SecretKey) -> Result<bool, SecretError>;
    fn delete(&self, key: &SecretKey) -> Result<Deleted, SecretError>;
}

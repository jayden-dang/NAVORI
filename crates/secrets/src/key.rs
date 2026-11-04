use std::fmt;

/// Which half of a secret key is invalid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyPart {
    Kind,
    Name,
}

impl fmt::Display for KeyPart {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Kind => "kind",
            Self::Name => "name",
        })
    }
}

/// What is wrong with it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyProblem {
    Empty,
    Slash,
    Control,
}

impl fmt::Display for KeyProblem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Empty => "is empty",
            Self::Slash => "contains `/`",
            Self::Control => "contains a control character",
        })
    }
}

/// A secret key was refused before any Keychain call.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("secret key {part} {problem}; use a non-empty {part} without `/` or control characters")]
pub struct KeyError {
    pub part: KeyPart,
    pub problem: KeyProblem,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecretKey {
    kind: String,
    name: String,
}

impl SecretKey {
    pub fn new(kind: &str, name: &str) -> Result<Self, KeyError> {
        check(KeyPart::Kind, kind)?;
        check(KeyPart::Name, name)?;
        Ok(Self {
            kind: kind.to_owned(),
            name: name.to_owned(),
        })
    }

    pub(crate) fn account(&self) -> String {
        format!("{}/{}", self.kind, self.name)
    }

    pub(crate) fn kind(&self) -> &str {
        &self.kind
    }

    pub(crate) fn name(&self) -> &str {
        &self.name
    }
}

fn check(part: KeyPart, text: &str) -> Result<(), KeyError> {
    let problem = if text.is_empty() {
        KeyProblem::Empty
    } else if text.contains('/') {
        KeyProblem::Slash
    } else if text.chars().any(char::is_control) {
        KeyProblem::Control
    } else {
        return Ok(());
    };
    Err(KeyError { part, problem })
}

/// The Keychain service that holds one Profile's items. The composition root maps a Profile to one constant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeychainService(&'static str);

impl KeychainService {
    pub const RELEASE: Self = Self("dev.navori");
    pub const DEV: Self = Self("dev.navori.dev");

    pub fn as_str(self) -> &'static str {
        self.0
    }
}

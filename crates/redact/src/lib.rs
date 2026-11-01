//! Sensitive Data Shield.
//!
//! Everything that leaves the machine (LLM requests) or is persisted (event journal, memory)
//! passes through this crate first. Policy: **see everything except secrets**.
//!
//! - [`redact_text`] replaces secrets in free text with `[REDACTED:<rule>]`.
//! - [`redact_json`] does the same for every string inside a JSON value, and blanks values
//!   stored under secret-named keys (`password`, `api_key`, …).
//! - [`is_sensitive_path`] tells whether a file must never be read on behalf of the AI.

use std::path::{Component, Path, PathBuf};
use std::sync::LazyLock;

use regex::Regex;
use serde_json::Value;

/// One secret found in the input. Byte offsets refer to the original input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub rule: &'static str,
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Redacted {
    pub text: String,
    /// Sorted by `start`, non-overlapping.
    pub findings: Vec<Finding>,
}

/// A detection rule. When `group` is set, only that capture group is the secret
/// (e.g. the password inside a URL) and the surrounding match is preserved.
struct Rule {
    name: &'static str,
    re: Regex,
    group: Option<usize>,
}

fn rule(name: &'static str, pattern: &str, group: Option<usize>) -> Rule {
    // Patterns are compile-time constants covered by tests; failure is a programming error.
    #[allow(clippy::expect_used)]
    let re = Regex::new(pattern).expect("built-in redaction pattern must compile");
    Rule { name, re, group }
}

/// Order matters: on overlapping matches the earlier rule wins.
static RULES: LazyLock<Vec<Rule>> = LazyLock::new(|| {
    vec![
        rule(
            "private_key",
            r"(?s)-----BEGIN [A-Z0-9 ]*PRIVATE KEY-----.*?(?:-----END [A-Z0-9 ]*PRIVATE KEY-----|\z)",
            None,
        ),
        rule("anthropic_key", r"\bsk-ant-[A-Za-z0-9_\-]{20,}", None),
        rule(
            "github_token",
            r"\b(?:ghp|gho|ghu|ghs|ghr)_[A-Za-z0-9]{36,}\b",
            None,
        ),
        rule("github_token", r"\bgithub_pat_[A-Za-z0-9_]{22,}", None),
        rule(
            "stripe_key",
            r"\b[sr]k_(?:live|test)_[A-Za-z0-9]{16,}",
            None,
        ),
        rule("sk_api_key", r"\bsk-(?:proj-)?[A-Za-z0-9_\-]{20,}", None),
        rule("aws_access_key", r"\b(?:AKIA|ASIA)[0-9A-Z]{16}\b", None),
        rule("slack_token", r"\bxox[abprs]-[A-Za-z0-9\-]{10,}", None),
        rule("google_api_key", r"\bAIza[0-9A-Za-z_\-]{35}", None),
        rule("xai_key", r"\bxai-[A-Za-z0-9]{20,}", None),
        rule(
            "jwt",
            r"\beyJ[A-Za-z0-9_\-]{8,}\.eyJ[A-Za-z0-9_\-]{8,}\.[A-Za-z0-9_\-]{8,}",
            None,
        ),
        rule(
            "bearer_token",
            r"(?i)\bbearer\s+([A-Za-z0-9._~+/=\-]{20,})",
            Some(1),
        ),
        rule(
            "url_password",
            r"\b[a-zA-Z][a-zA-Z0-9+.\-]*://[^\s:/@]+:([^\s@/]+)@",
            Some(1),
        ),
        rule(
            "secret_assignment",
            r#"(?i)\b[A-Z0-9_\-]*(?:SECRET|TOKEN|PASSWORD|PASSWD|PWD|API_?KEY|PRIVATE_?KEY|ACCESS_?KEY|CREDENTIALS?)[A-Z0-9_\-]*["']?\s*[:=]\s*["']?([^\s"',;]{6,})"#,
            Some(1),
        ),
    ]
});

/// Keys whose JSON values are always treated as secrets, whatever they contain.
fn is_secret_key(key: &str) -> bool {
    let k = key.to_ascii_lowercase().replace(['-', '.'], "_");
    const EXACT: &[&str] = &[
        "password",
        "passwd",
        "pwd",
        "secret",
        "token",
        "credential",
        "credentials",
    ];
    const SUFFIX: &[&str] = &[
        "_password",
        "_secret",
        "_token",
        "api_key",
        "apikey",
        "private_key",
        "access_key",
        "client_secret",
    ];
    EXACT.contains(&k.as_str()) || SUFFIX.iter().any(|s| k.ends_with(s))
}

pub fn redact_text(input: &str) -> Redacted {
    let mut found: Vec<Finding> = Vec::new();
    for r in RULES.iter() {
        for caps in r.re.captures_iter(input) {
            let m = match r.group {
                Some(g) => caps.get(g),
                None => caps.get(0),
            };
            let Some(m) = m else { continue };
            if m.start() == m.end() {
                continue;
            }
            let overlaps = found.iter().any(|f| m.start() < f.end && f.start < m.end());
            if !overlaps {
                found.push(Finding {
                    rule: r.name,
                    start: m.start(),
                    end: m.end(),
                });
            }
        }
    }
    found.sort_by_key(|f| f.start);

    let mut text = String::with_capacity(input.len());
    let mut cursor = 0;
    for f in &found {
        text.push_str(&input[cursor..f.start]);
        text.push_str("[REDACTED:");
        text.push_str(f.rule);
        text.push(']');
        cursor = f.end;
    }
    text.push_str(&input[cursor..]);
    Redacted {
        text,
        findings: found,
    }
}

/// Redacts every string in `value` in place. Returns the number of secrets removed.
pub fn redact_json(value: &mut Value) -> usize {
    match value {
        Value::String(s) => {
            let out = redact_text(s);
            let n = out.findings.len();
            if n > 0 {
                *s = out.text;
            }
            n
        }
        Value::Array(items) => items.iter_mut().map(redact_json).sum(),
        Value::Object(map) => {
            let mut n = 0;
            for (k, v) in map.iter_mut() {
                if is_secret_key(k) && !v.is_null() {
                    *v = Value::String("[REDACTED:secret_field]".to_owned());
                    n += 1;
                } else {
                    n += redact_json(v);
                }
            }
            n
        }
        Value::Null | Value::Bool(_) | Value::Number(_) => 0,
    }
}

/// Paths (relative to `$HOME`) that hold credentials.
const HOME_SENSITIVE: &[&str] = &[
    ".ssh",
    ".gnupg",
    ".aws/credentials",
    ".aws/config",
    ".config/gh/hosts.yml",
    ".netrc",
    ".docker/config.json",
    ".kube/config",
    ".npmrc",
    ".pypirc",
    ".git-credentials",
    "Library/Keychains",
    ".claude/.credentials.json",
    ".grok/auth.json",
    ".grok/mcp_credentials.json",
    ".codex/auth.json",
];

const SENSITIVE_EXTENSIONS: &[&str] = &[
    "pem", "key", "p12", "pfx", "jks", "keystore", "ppk", "asc", "gpg",
];
const ENV_TEMPLATES: &[&str] = &[".env.example", ".env.sample", ".env.template", ".env.dist"];

/// Lexically normalizes `.` and `..` without touching the filesystem.
fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in path.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// Whether `path` must never be read or sent anywhere on behalf of the AI.
pub fn is_sensitive_path(path: &Path, home: &Path) -> bool {
    let path = normalize(path);

    if let Ok(rel) = path.strip_prefix(normalize(home))
        && HOME_SENSITIVE.iter().any(|s| rel.starts_with(s))
    {
        return true;
    }

    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    if (name == ".env" || name.starts_with(".env.")) && !ENV_TEMPLATES.contains(&name) {
        return true;
    }
    if name.starts_with("id_rsa")
        || name.starts_with("id_ed25519")
        || name.starts_with("id_ecdsa")
        || name.starts_with("id_dsa")
    {
        return true;
    }
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| SENSITIVE_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
}

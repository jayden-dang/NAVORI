//! Wire types shared by the kernel, IPC, CLI and the FFI layer.
//!
//! Everything here is plain data (serde). No behavior lives in this crate.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// How dangerous an action is. Drives approval policy for UI, CLI and AI callers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Risk {
    /// Observes state only.
    Read,
    /// Changes state in a recoverable way.
    Write,
    /// Irreversible or disruptive (kill a process, merge a PR). Always needs explicit approval.
    Destructive,
}

/// Static description of one action a module accepts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActionSpec {
    pub module: String,
    pub name: String,
    pub description: String,
    pub risk: Risk,
    /// JSON Schema of the module's whole action enum (shared by all actions of a module).
    pub input_schema: Value,
}

/// A type-erased command addressed to a module.
///
/// `action` is the externally tagged JSON form of the module's action enum,
/// e.g. `"ping"` or `{"hold": {"minutes": 30}}`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Envelope {
    pub module: String,
    pub action: Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Debug,
    Info,
    Warn,
    Error,
}

/// A domain event. Persisted to the journal (after redaction) and broadcast live.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Event {
    /// ULID, lexicographically sortable by creation time.
    pub id: String,
    /// Unix epoch milliseconds.
    pub ts_ms: i64,
    pub module: String,
    pub kind: String,
    pub severity: Severity,
    /// One human-readable sentence.
    pub summary: String,
    pub payload: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum ModuleState {
    Starting,
    Running,
    /// Crashed and waiting for the supervisor to restart it.
    Restarting {
        attempt: u32,
        last_error: String,
    },
    /// Gave up after too many crashes.
    Failed {
        last_error: String,
    },
    Stopped,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModuleInfo {
    pub id: String,
    pub state: ModuleState,
    pub restarts: u32,
}

/// Which mark the status icon shows. The Kernel chooses it; the Shell only draws it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrayGlyph {
    Normal,
    /// A Lease holds the Mac awake.
    Lease,
    /// Navori AI is working.
    AiRunning,
    Problem,
}

/// The composed status icon state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrayState {
    pub glyph: TrayGlyph,
    /// Something waits for the user.
    pub attention: bool,
    /// One line for the status item's text and accessibility description.
    pub summary: String,
}

/// Snapshot of the running core, served to the UI, CLI and `doctor`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CoreStatus {
    pub version: String,
    pub profile: String,
    pub pid: u32,
    pub started_at_ms: i64,
    pub uptime_ms: u64,
    pub data_dir: String,
    pub socket_path: String,
    pub modules: Vec<ModuleInfo>,
    /// The composed status icon state.
    pub tray: TrayState,
}

/// One diagnostic check reported by `doctor`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Check {
    pub name: String,
    pub ok: bool,
    pub detail: String,
}

/// Params of `core.ask`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AskRequest {
    pub model: String,
    pub text: String,
}

/// Result of `core.ask`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AskReply {
    pub reply: String,
}

/// JSON-RPC 2.0 over newline-delimited JSON on the local Unix socket.
pub mod rpc {
    use super::{Deserialize, Serialize, Value};

    pub const VERSION: &str = "2.0";

    pub mod method {
        pub const STATUS: &str = "core.status";
        pub const ACTIONS: &str = "core.actions";
        pub const DISPATCH: &str = "core.dispatch";
        pub const EVENTS: &str = "core.events";
        pub const DOCTOR: &str = "core.doctor";
        pub const ASK: &str = "core.ask";
    }

    /// Standard JSON-RPC error codes plus Navori-specific ones (-32000..-32099).
    pub mod code {
        pub const PARSE_ERROR: i64 = -32700;
        pub const INVALID_REQUEST: i64 = -32600;
        pub const METHOD_NOT_FOUND: i64 = -32601;
        pub const INVALID_PARAMS: i64 = -32602;
        pub const INTERNAL: i64 = -32603;
        pub const UNKNOWN_MODULE: i64 = -32001;
        pub const ACTION_FAILED: i64 = -32002;
        pub const MODULE_UNAVAILABLE: i64 = -32003;
        pub const ASK_NO_KEY: i64 = -32004;
        pub const ASK_KEYCHAIN: i64 = -32005;
        pub const ASK_BLOCKED: i64 = -32006;
        pub const ASK_PROVIDER: i64 = -32007;
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    pub struct Request {
        pub jsonrpc: String,
        pub id: Value,
        pub method: String,
        #[serde(default)]
        pub params: Value,
    }

    impl Request {
        pub fn new(id: impl Into<Value>, method: &str, params: Value) -> Self {
            Self {
                jsonrpc: VERSION.to_owned(),
                id: id.into(),
                method: method.to_owned(),
                params,
            }
        }
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    pub struct Error {
        pub code: i64,
        pub message: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub data: Option<Value>,
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    pub struct Response {
        pub jsonrpc: String,
        pub id: Value,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub result: Option<Value>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub error: Option<Error>,
    }

    impl Response {
        pub fn ok(id: Value, result: Value) -> Self {
            Self {
                jsonrpc: VERSION.to_owned(),
                id,
                result: Some(result),
                error: None,
            }
        }

        pub fn err(id: Value, code: i64, message: impl Into<String>) -> Self {
            Self {
                jsonrpc: VERSION.to_owned(),
                id,
                result: None,
                error: Some(Error {
                    code,
                    message: message.into(),
                    data: None,
                }),
            }
        }
    }
}

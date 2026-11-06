//! Coding-agent harness recognition.

use std::path::Path;

use crate::error::{Field, Unavailable};
use crate::location::{Location, resolve_location};
use navori_macos::Marker;

use crate::snapshot::{ProcId, Process, Snapshot};

pub use navori_protocol::Harness;

/// The terminal application an Agent session was launched from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminalHost {
    Terminal,
    ITerm2,
    Ghostty,
    WezTerm,
    Kitty,
    Alacritty,
    Warp,
    VsCode,
    Cursor,
    Zed,
}

/// A running coding agent, its project location, and the terminal it was launched from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentSession {
    pub harness: Harness,
    pub process: ProcId,
    pub location: Field<Location>,
    pub terminal: Option<TerminalHost>,
}

/// Case-sensitive. Direct rules use the executable path. A runtime rule uses `argv[1]`
/// and is considered only when that path's basename is `node` or `bun`.
struct CatalogEntry {
    harness: Harness,
    direct: &'static [Direct],
    runtime: Option<&'static str>,
}

enum Direct {
    Contains(&'static str),
    Basename(&'static str),
    /// Basename starts with `prefix` and the executable path contains `under`.
    PrefixUnder {
        prefix: &'static str,
        under: &'static str,
    },
}

const CATALOG: &[CatalogEntry] = &[
    CatalogEntry {
        harness: Harness::ClaudeCode,
        direct: &[
            Direct::Contains("/claude/versions/"),
            Direct::Basename("claude"),
        ],
        runtime: Some("/@anthropic-ai/claude-code/"),
    },
    CatalogEntry {
        harness: Harness::Codex,
        direct: &[Direct::Basename("codex")],
        runtime: Some("/@openai/codex/"),
    },
    CatalogEntry {
        harness: Harness::Grok,
        direct: &[
            Direct::Basename("grok"),
            Direct::PrefixUnder {
                prefix: "grok-",
                under: "/.grok/",
            },
        ],
        runtime: None,
    },
];

const RUNTIMES: &[&str] = &["node", "bun"];

/// Matched as `/<bundle>.app/Contents/` in an ancestor's executable path.
const TERMINAL_HOSTS: &[(&str, TerminalHost)] = &[
    ("/Terminal.app/Contents/", TerminalHost::Terminal),
    ("/iTerm.app/Contents/", TerminalHost::ITerm2),
    ("/Ghostty.app/Contents/", TerminalHost::Ghostty),
    ("/WezTerm.app/Contents/", TerminalHost::WezTerm),
    ("/kitty.app/Contents/", TerminalHost::Kitty),
    ("/Alacritty.app/Contents/", TerminalHost::Alacritty),
    ("/Warp.app/Contents/", TerminalHost::Warp),
    ("/Visual Studio Code.app/Contents/", TerminalHost::VsCode),
    ("/Cursor.app/Contents/", TerminalHost::Cursor),
    ("/Zed.app/Contents/", TerminalHost::Zed),
];

/// The harness of this process, or `None` when the catalog does not match.
/// Argument vectors are read only for `node` and `bun` executables.
pub(crate) fn recognize(snapshot: &Snapshot, process: &Process) -> Option<Harness> {
    let path = process.exe.as_ref().ok()?;
    if let Some(harness) = direct_hit(path) {
        return Some(harness);
    }
    if !is_runtime(path) {
        return None;
    }
    let argv = match snapshot.argv(process.id) {
        Ok(argv) => argv,
        Err(_) => {
            tracing::debug!(pid = process.id.pid, "arguments could not be read");
            return None;
        }
    };
    runtime_hit(argv.get(1)?)
}

/// Environment names a Harness sets for the processes it starts. Matched exactly, in this order.
pub const MARKERS: [(&str, Harness); 3] = [
    ("CLAUDE_CODE_CHILD_SESSION", Harness::ClaudeCode),
    ("CODEX_THREAD_ID", Harness::Codex),
    ("GROK_AGENT", Harness::Grok),
];

/// The Harness that started this process: the nearest recognized process in its ancestor chain,
/// including itself, then the one marker its own environment carries. Two markers name none.
///
/// # Errors
///
/// The process is no longer the one `id` names, or its environment could not be read. Neither is
/// a "no Harness" answer.
pub fn started_by(snapshot: &Snapshot, id: ProcId) -> Field<Option<Harness>> {
    let Some(process) = snapshot.get(id.pid).filter(|process| process.id == id) else {
        return Err(Unavailable::Exited);
    };
    let nearest = std::iter::once(process)
        .chain(snapshot.ancestors(id))
        .find_map(|candidate| recognize(snapshot, candidate));
    if nearest.is_some() {
        return Ok(nearest);
    }
    let names = MARKERS.map(|(name, _)| name);
    Ok(match snapshot.env_marker(id, &names)? {
        Marker::One(index) => MARKERS.get(index).map(|(_, harness)| *harness),
        Marker::None | Marker::Several => None,
    })
}

/// Recognized harness processes, except one whose ancestor is the same harness.
/// An unreadable working directory leaves the session in place with that location unavailable.
/// The terminal is the nearest ancestor inside a recognized `.app` bundle, if any.
pub fn sessions(snapshot: &Snapshot) -> Vec<AgentSession> {
    let found: Vec<(&Process, Harness)> = snapshot
        .processes()
        .filter_map(|process| recognize(snapshot, process).map(|harness| (process, harness)))
        .collect();

    found
        .iter()
        .filter(|(process, harness)| !same_harness_above(snapshot, &found, process, *harness))
        .map(|(process, harness)| AgentSession {
            harness: *harness,
            process: process.id,
            location: snapshot.cwd(process.id).map(|cwd| resolve_location(&cwd)),
            terminal: terminal_host(snapshot, process.id),
        })
        .collect()
}

fn direct_hit(path: &Path) -> Option<Harness> {
    let text = path.to_string_lossy();
    let base = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    CATALOG.iter().find_map(|entry| {
        entry
            .direct
            .iter()
            .any(|rule| rule_matches(rule, &text, &base))
            .then_some(entry.harness)
    })
}

fn rule_matches(rule: &Direct, text: &str, base: &str) -> bool {
    match rule {
        Direct::Contains(needle) => text.contains(needle),
        Direct::Basename(name) => base == *name,
        Direct::PrefixUnder { prefix, under } => base.starts_with(prefix) && text.contains(under),
    }
}

fn is_runtime(path: &Path) -> bool {
    path.file_name()
        .is_some_and(|name| RUNTIMES.iter().any(|runtime| name == *runtime))
}

fn runtime_hit(script: &str) -> Option<Harness> {
    CATALOG.iter().find_map(|entry| {
        entry
            .runtime
            .filter(|needle| script.contains(needle))
            .map(|_| entry.harness)
    })
}

fn same_harness_above(
    snapshot: &Snapshot,
    found: &[(&Process, Harness)],
    process: &Process,
    harness: Harness,
) -> bool {
    snapshot.ancestors(process.id).iter().any(|ancestor| {
        found
            .iter()
            .any(|(other, other_harness)| other.id == ancestor.id && *other_harness == harness)
    })
}

fn terminal_host(snapshot: &Snapshot, id: ProcId) -> Option<TerminalHost> {
    snapshot.ancestors(id).into_iter().find_map(|ancestor| {
        let path = ancestor.exe.as_ref().ok()?;
        terminal_of(path)
    })
}

fn terminal_of(path: &Path) -> Option<TerminalHost> {
    let text = path.to_string_lossy();
    TERMINAL_HOSTS
        .iter()
        .find_map(|(needle, host)| text.contains(needle).then_some(*host))
}

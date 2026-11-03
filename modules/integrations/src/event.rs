use crate::action::Harness;

pub const HOOK_INSTALLED: &str = "integrations.hook_installed";
pub const HOOK_REPAIRED: &str = "integrations.hook_repaired";
pub const HOOK_REMOVED: &str = "integrations.hook_removed";
pub const HOOK_FAILED: &str = "integrations.hook_failed";

pub(crate) fn name(harness: Harness) -> &'static str {
    match harness {
        Harness::Claude => "Claude Code",
        Harness::Codex => "Codex",
        Harness::Grok => "Grok Build",
    }
}

pub(crate) fn not_found(harness: Harness) -> String {
    let dir = match harness {
        Harness::Claude => "~/.claude",
        Harness::Codex => "~/.codex",
        Harness::Grok => "~/.grok",
    };
    format!(
        "There is no {dir} directory, so {} is not set up on this Mac.",
        name(harness)
    )
}

pub(crate) fn unreadable(failure: &str) -> String {
    format!("The hook file cannot be read: {failure}.")
}

pub(crate) fn codex_unverified() -> String {
    "The Codex timeout unit is not verified yet.".to_owned()
}

pub(crate) fn installed(harness: Harness) -> String {
    format!("Installed Navori's hooks for {}.", name(harness))
}

pub(crate) fn already_installed(harness: Harness) -> String {
    format!(
        "Navori's hooks for {} were already in place.",
        name(harness)
    )
}

pub(crate) fn repaired(harness: Harness) -> String {
    format!("Repaired Navori's hooks for {}.", name(harness))
}

pub(crate) fn already_correct(harness: Harness) -> String {
    format!("Navori's hooks for {} were already correct.", name(harness))
}

pub(crate) fn failed(harness: Harness, verb: &str) -> String {
    format!("Could not {verb} Navori's hooks for {}.", name(harness))
}

use crate::action::Harness;
use crate::infra::files;

pub const HOOK_INSTALLED: &str = "integrations.hook_installed";
pub const HOOK_REPAIRED: &str = "integrations.hook_repaired";
pub const HOOK_REMOVED: &str = "integrations.hook_removed";
pub const HOOK_FAILED: &str = "integrations.hook_failed";

pub(crate) fn name(harness: Harness) -> &'static str {
    harness.display()
}

pub(crate) fn not_found(harness: Harness) -> String {
    format!(
        "There is no ~/{} directory, so {} is not set up on this Mac.",
        files::dir_name(harness),
        name(harness)
    )
}

pub(crate) fn unreadable(failure: &str) -> String {
    format!("The hook file cannot be read: {failure}.")
}

pub(crate) fn codex_needs_trust() -> String {
    "Codex runs these hooks only after you trust them: open Codex and run /hooks.".to_owned()
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

pub(crate) fn removed(harness: Harness) -> String {
    format!("Removed Navori's hooks from {}.", name(harness))
}

pub(crate) fn nothing_to_remove(harness: Harness) -> String {
    format!(
        "There were no hooks of Navori to remove from {}.",
        name(harness)
    )
}

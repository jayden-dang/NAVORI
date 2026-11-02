pub const LEASE_STARTED: &str = "awake.lease_started";
pub const LEASE_ENDED: &str = "awake.lease_ended";

pub(crate) fn started(source: &str) -> String {
    format!("Keeping this Mac awake: {source}.")
}

pub(crate) fn ended_deadline(source: &str) -> String {
    format!("Stopped keeping this Mac awake: {source} reached its end time.")
}

pub(crate) fn ended_release_all() -> String {
    "Stopped keeping this Mac awake: Release all.".to_owned()
}

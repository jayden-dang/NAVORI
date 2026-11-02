pub const LEASE_STARTED: &str = "awake.lease_started";
pub const LEASE_ENDED: &str = "awake.lease_ended";

const MINUTE: u64 = 60;
const HOUR: u64 = 3600;
const DAY: u64 = 86_400;

pub(crate) fn started(source: &str) -> String {
    format!("Keeping this Mac awake: {source}.")
}

pub(crate) fn duration_source(seconds: u64) -> String {
    if seconds == HOUR {
        return "1 hour".to_owned();
    }
    if seconds.is_multiple_of(DAY) {
        return counted(seconds / DAY, "day");
    }
    if seconds.is_multiple_of(HOUR) {
        return counted(seconds / HOUR, "hour");
    }
    if seconds.is_multiple_of(MINUTE) {
        return counted(seconds / MINUTE, "minute");
    }
    counted(seconds, "second")
}

fn counted(n: u64, unit: &str) -> String {
    if n == 1 {
        format!("1 {unit}")
    } else {
        format!("{n} {unit}s")
    }
}

pub(crate) fn ended_deadline(source: &str) -> String {
    format!("Stopped keeping this Mac awake: {source} reached its end time.")
}

pub(crate) fn ended_release_all() -> String {
    "Stopped keeping this Mac awake: Release all.".to_owned()
}

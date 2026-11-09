use std::time::SystemTime;

/// Seconds east of UTC for the local zone at `at`.
pub trait LocalCalendar: Send + Sync {
    fn utc_offset(&self, at: SystemTime) -> i32;
}

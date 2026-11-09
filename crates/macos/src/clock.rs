use std::mem::MaybeUninit;
use std::time::{SystemTime, UNIX_EPOCH};

/// Seconds east of UTC for the local zone at `at`.
pub fn local_utc_offset(at: SystemTime) -> i32 {
    let secs = match at.duration_since(UNIX_EPOCH) {
        Ok(d) => i64::try_from(d.as_secs()).unwrap_or(i64::MAX),
        Err(e) => -i64::try_from(e.duration().as_secs()).unwrap_or(i64::MAX),
    };
    let time = secs as libc::time_t;
    let mut tm = MaybeUninit::<libc::tm>::uninit();
    let ptr = unsafe {
        // SAFETY: `time` and `tm` are stack locals; localtime_r writes one `tm` into `tm`
        // and returns that pointer, or null on failure. It does not retain either pointer.
        libc::localtime_r(&raw const time, tm.as_mut_ptr())
    };
    if ptr.is_null() {
        return 0;
    }
    let tm = unsafe {
        // SAFETY: localtime_r returned non-null, so it initialized `tm`.
        tm.assume_init()
    };
    tm.tm_gmtoff as i32
}

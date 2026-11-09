use std::time::{Duration, SystemTime, UNIX_EPOCH};

use navori_kernel::ModuleCtx;

use crate::NeedsYou;
use crate::millis;
use crate::stored;

pub fn soonest_deadline(ctx: &ModuleCtx<NeedsYou>) -> Option<SystemTime> {
    match stored::soonest_expires_at_ms(&ctx.store) {
        Ok(Some(ms)) => u64::try_from(ms)
            .ok()
            .map(|millis| UNIX_EPOCH + Duration::from_millis(millis)),
        Ok(None) => None,
        Err(err) => {
            tracing::warn!(error = %err, "needs_you deadline read failed");
            None
        }
    }
}

pub fn expire_due(ctx: &ModuleCtx<NeedsYou>) -> Result<(), String> {
    let now_ms = millis(ctx.wait_handle().now())?;
    for row in stored::due_expiries(&ctx.store, now_ms)? {
        stored::expire_to_update(&ctx.store, row.id, &row.title, now_ms)?;
    }
    Ok(())
}

pub fn mark_viewed(ctx: &ModuleCtx<NeedsYou>, ids: &[i64]) -> Result<(), String> {
    let now_ms = millis(ctx.wait_handle().now())?;
    stored::mark_updates_viewed(&ctx.store, ids, now_ms)
}

pub fn clear_updates(ctx: &ModuleCtx<NeedsYou>) -> Result<(), String> {
    let now_ms = millis(ctx.wait_handle().now())?;
    stored::clear_unresolved_updates(&ctx.store, now_ms)
}

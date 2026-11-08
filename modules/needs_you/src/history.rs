//! History reads and retention prune for Decisions and Updates.

use std::time::Duration;

use navori_config::JournalSweep;
use navori_kernel::ModuleCtx;
use navori_store::Store;
use rusqlite::params;
use serde::Serialize;

use crate::NeedsYou;
use crate::millis;

const DAY_MS: i64 = 24 * 60 * 60 * 1000;

pub const PRUNE_EVERY: Duration = Duration::from_secs(24 * 60 * 60);

#[derive(Debug, Clone, Serialize)]
pub struct HistoryEntry {
    pub kind: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chosen_label: Option<String>,
    pub resolved_at_ms: i64,
}

pub fn list(
    ctx: &ModuleCtx<NeedsYou>,
    retention: JournalSweep,
) -> Result<Vec<HistoryEntry>, String> {
    let now_ms = millis(ctx.wait_handle().now())?;
    let cutoff = match retention {
        JournalSweep::KeepDays(days) => now_ms.saturating_sub(i64::from(days) * DAY_MS),
        JournalSweep::Paused => i64::MIN,
    };
    load_history(&ctx.store, cutoff)
}

pub fn prune(ctx: &ModuleCtx<NeedsYou>, days: u32) -> Result<(), String> {
    let now_ms = millis(ctx.wait_handle().now())?;
    let cutoff = now_ms.saturating_sub(i64::from(days) * DAY_MS);
    delete_older_than(&ctx.store, cutoff)
}

fn load_history(store: &Store, cutoff: i64) -> Result<Vec<HistoryEntry>, String> {
    store.with_conn(|conn| {
        let mut stmt = conn
            .prepare(
                "SELECT kind, title, chosen_label, resolved_at_ms FROM (
                    SELECT 'decision' AS kind,
                           title,
                           CASE chosen
                             WHEN 'primary' THEN primary_label
                             WHEN 'secondary' THEN secondary_label
                             ELSE NULL
                           END AS chosen_label,
                           resolved_at_ms
                    FROM needs_you_decisions
                    WHERE state = 'answered'
                      AND resolved_at_ms IS NOT NULL
                      AND resolved_at_ms >= ?1
                    UNION ALL
                    SELECT 'update' AS kind,
                           title,
                           NULL AS chosen_label,
                           resolved_at_ms
                    FROM needs_you_updates
                    WHERE resolution IS NOT NULL
                      AND resolved_at_ms IS NOT NULL
                      AND resolved_at_ms >= ?1
                 )
                 ORDER BY resolved_at_ms DESC, kind DESC",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([cutoff], |row| {
                Ok(HistoryEntry {
                    kind: row.get(0)?,
                    title: row.get(1)?,
                    chosen_label: row.get(2)?,
                    resolved_at_ms: row.get(3)?,
                })
            })
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())
    })
}

fn delete_older_than(store: &Store, cutoff: i64) -> Result<(), String> {
    store.with_conn(|conn| {
        conn.execute(
            "DELETE FROM needs_you_decisions WHERE resolved_at_ms < ?1",
            params![cutoff],
        )
        .map_err(|e| e.to_string())?;
        conn.execute(
            "DELETE FROM needs_you_updates WHERE resolved_at_ms < ?1",
            params![cutoff],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    })
}

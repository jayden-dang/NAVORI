use std::time::{SystemTime, UNIX_EPOCH};

use navori_kernel::ModuleCtx;
use navori_macos::StartTime;
use navori_procs::ProcId;

use super::{
    PLATFORM_PROBLEM, RowFate, STORE_PROBLEM, at, stored_command, stored_key, stored_session,
};
use crate::domain::Ended;
use crate::infra::ProcessClass;
use crate::infra::stored::{self, StoredKind, StoredLease};

impl super::Awake {
    pub(super) fn note_store(&mut self, now: SystemTime) {
        self.problems.store = Some(STORE_PROBLEM.to_owned());
        self.schedule_retry(now);
    }

    pub(super) fn note_platform(&mut self, now: SystemTime, err: impl std::fmt::Display) {
        tracing::warn!(error = %err, "a stored lease could not be watched");
        self.problems.platform = Some(PLATFORM_PROBLEM.to_owned());
        self.schedule_retry(now);
    }

    pub(super) fn save_session(
        &mut self,
        ctx: &ModuleCtx<Self>,
        now: SystemTime,
        session_id: &str,
    ) {
        let Some(saved) = self.table.session_saved(session_id) else {
            return;
        };
        self.upsert_row(ctx, now, stored_session(&saved));
    }

    pub(super) fn save_command(&mut self, ctx: &ModuleCtx<Self>, now: SystemTime, process: ProcId) {
        if !self.table.has_command(process) {
            return;
        }
        self.upsert_row(ctx, now, stored_command(process));
    }

    pub(super) fn upsert_row(&mut self, ctx: &ModuleCtx<Self>, now: SystemTime, row: StoredLease) {
        match stored::upsert(&ctx.store, &row) {
            Ok(()) => self.clear_dirty(&row.key),
            Err(err) => {
                tracing::warn!(error = %err, key = %row.key, "lease row was not written");
                self.dirty.insert(row.key);
                self.note_store(now);
            }
        }
    }

    pub(super) fn delete_stored(&mut self, ctx: &ModuleCtx<Self>, now: SystemTime, key: &str) {
        match stored::delete(&ctx.store, key) {
            Ok(()) => self.clear_dirty(key),
            Err(err) => {
                tracing::warn!(error = %err, key, "lease row was not deleted");
                self.dirty.insert(key.to_owned());
                self.note_store(now);
            }
        }
    }

    pub(super) fn clear_dirty(&mut self, key: &str) {
        self.dirty.remove(key);
        if self.dirty.is_empty() {
            self.problems.store = None;
        }
    }

    pub(super) fn forget_ended(&mut self, ctx: &ModuleCtx<Self>, now: SystemTime, ended: &[Ended]) {
        for end in ended {
            if let Some(key) = stored_key(&end.lease) {
                self.delete_stored(ctx, now, &key);
            }
        }
    }

    pub(super) fn flush_dirty(&mut self, ctx: &ModuleCtx<Self>, now: SystemTime) {
        if self.dirty.is_empty() {
            return;
        }
        let keys: Vec<String> = self.dirty.iter().cloned().collect();
        for key in keys {
            let result = match self.row_for(&key) {
                Some(row) => stored::upsert(&ctx.store, &row),
                None => stored::delete(&ctx.store, &key),
            };
            match result {
                Ok(()) => {
                    self.dirty.remove(&key);
                }
                Err(err) => {
                    tracing::warn!(error = %err, key = %key, "a stored lease retry failed");
                }
            }
        }
        if self.dirty.is_empty() {
            self.problems.store = None;
        } else {
            self.note_store(now);
        }
    }

    pub(super) fn row_for(&self, key: &str) -> Option<StoredLease> {
        self.table
            .saved_sessions()
            .into_iter()
            .map(|saved| stored_session(&saved))
            .chain(self.table.saved_commands().into_iter().map(stored_command))
            .find(|row| row.key == key)
    }

    pub(super) async fn restore_if_needed(&mut self, ctx: &ModuleCtx<Self>, now: SystemTime) {
        if self.restored {
            return;
        }
        let rows = match stored::load(&ctx.store) {
            Ok(rows) => rows,
            Err(err) => {
                tracing::warn!(error = %err, "stored leases could not be read");
                self.note_store(now);
                return;
            }
        };
        if rows.is_empty() {
            self.finish_restore();
            return;
        }
        let mut installed = Vec::new();
        let mut armed = Vec::new();
        let mut retry = false;
        for row in rows {
            match self.classify_row(ctx, now, &row, &mut armed) {
                RowFate::Keep(id) => {
                    if !self.install_row(&row, id, None) {
                        self.delete_stored(ctx, now, &row.key);
                        continue;
                    }
                    // Settle and hold this row before the next one is read.
                    self.turn(ctx, now).await;
                    if self.installed_row_remains(&row, id) {
                        installed.push((row, id));
                    }
                }
                RowFate::Drop => {}
                RowFate::Retry => retry = true,
                RowFate::Halt => {
                    self.disarm_unheld(&armed);
                    return;
                }
            }
        }
        self.label_installed(&installed).await;
        self.disarm_unheld(&armed);
        if retry {
            return;
        }
        self.finish_restore();
    }

    pub(super) fn installed_row_remains(&self, row: &StoredLease, id: ProcId) -> bool {
        match row.kind {
            StoredKind::Command => self.table.has_command(id),
            StoredKind::Session => row
                .session_id
                .as_deref()
                .is_some_and(|session_id| self.table.session_process(session_id).is_some()),
        }
    }

    /// One snapshot for every row that is still installed after settle.
    pub(super) async fn label_installed(&mut self, rows: &[(StoredLease, ProcId)]) {
        let rows: Vec<(StoredLease, ProcId)> = rows
            .iter()
            .filter(|(row, id)| self.installed_row_remains(row, *id))
            .cloned()
            .collect();
        if rows.is_empty() {
            return;
        }
        let command_ids: Vec<ProcId> = rows
            .iter()
            .filter(|(row, _)| row.kind == StoredKind::Command)
            .map(|(_, id)| *id)
            .collect();
        let session_ids: Vec<ProcId> = rows
            .iter()
            .filter(|(row, _)| row.kind == StoredKind::Session)
            .map(|(_, id)| *id)
            .collect();
        let labels = self.platform.fresh_labels(&command_ids, &session_ids).await;
        let mut command_at = 0;
        let mut session_at = 0;
        for (row, id) in &rows {
            match row.kind {
                StoredKind::Command => {
                    let label = at(
                        labels.as_ref().map(|pair| pair.commands.as_slice()),
                        command_at,
                    );
                    command_at += 1;
                    self.table.set_command_label(*id, label);
                }
                StoredKind::Session => {
                    let place = at(
                        labels.as_ref().map(|pair| pair.sessions.as_slice()),
                        session_at,
                    );
                    session_at += 1;
                    if let Some(session_id) = row.session_id.as_deref() {
                        self.table.set_session_place(session_id, place);
                    }
                }
            }
        }
    }

    pub(super) fn classify_row(
        &mut self,
        ctx: &ModuleCtx<Self>,
        now: SystemTime,
        row: &StoredLease,
        armed: &mut Vec<ProcId>,
    ) -> RowFate {
        let id = ProcId {
            pid: row.pid,
            start: StartTime(row.process_start_us),
        };
        // Watch before the liveness check so an exit in that gap is still delivered.
        if !armed.contains(&id) {
            if let Err(err) = self.arm(id) {
                self.note_platform(now, err);
                return RowFate::Halt;
            }
            armed.push(id);
        }
        match self.platform.classify(row.pid) {
            Err(err) => {
                self.note_platform(now, err);
                RowFate::Halt
            }
            Ok(ProcessClass::Unreadable) => {
                self.note_platform(now, "reading the process failed");
                RowFate::Retry
            }
            Ok(ProcessClass::Live(live) | ProcessClass::OtherUser(live)) if live == id => {
                RowFate::Keep(live)
            }
            Ok(_) => {
                self.delete_stored(ctx, now, &row.key);
                RowFate::Drop
            }
        }
    }

    pub(super) fn install_row(
        &mut self,
        row: &StoredLease,
        id: ProcId,
        label: Option<String>,
    ) -> bool {
        match row.kind {
            StoredKind::Command => {
                self.table.hold_command(id, UNIX_EPOCH, label);
                true
            }
            StoredKind::Session => {
                let Some(session_id) = row.session_id.clone() else {
                    return false;
                };
                // A repeat hold clears the stop mark and the next check.
                if self.table.session_process(&session_id).is_some() {
                    return true;
                }
                let Some(harness) = row.harness else {
                    return false;
                };
                let Some(first_hold) = row.first_hold else {
                    return false;
                };
                let Some(deadline) = row.deadline else {
                    return false;
                };
                self.table.hold_session(
                    session_id.clone(),
                    harness,
                    id,
                    first_hold,
                    deadline,
                    None,
                );
                if let Some(mark) = row.stop_mark {
                    self.table.stop_session(&session_id, mark, false);
                }
                true
            }
        }
    }

    pub(super) fn disarm_unheld(&mut self, armed: &[ProcId]) {
        for id in armed {
            if self.table.leases_on(*id) > 0 {
                continue;
            }
            if let Some(watcher) = self.watcher.as_mut() {
                watcher.unwatch(*id);
            }
        }
        if self.table.process_ids().is_empty() {
            self.watcher = None;
        }
    }

    pub(super) fn finish_restore(&mut self) {
        self.restored = true;
        self.problems.platform = None;
        if self.dirty.is_empty() {
            self.problems.store = None;
        }
    }
}

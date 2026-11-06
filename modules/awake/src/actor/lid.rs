use std::time::SystemTime;

use navori_kernel::{ActionError, ModuleCtx, Severity};
use navori_macos::PowerKind;
use serde_json::json;
use tokio::io::unix::AsyncFd;

use super::{
    LAST_SLEEP_PROBLEM, LAST_SLEEP_READ_PROBLEM, LID_CLEAR_PROBLEM, LID_MARKER_PROBLEM,
    LID_MARKER_READ_PROBLEM, LID_READ_PROBLEM, LID_REFUSED_PROBLEM, LID_SAVE_OFF_PROBLEM,
    LidProblemKind, POWER_PROBLEM, Started,
};
use crate::event::{self, LID_CHANGED};
use crate::infra::stored;

impl super::Awake {
    pub(super) fn load_last_sleep(&mut self, ctx: &ModuleCtx<Self>) {
        match stored::load_last_sleep(&ctx.store) {
            Ok(value) => self.last_sleep_ms = value,
            Err(err) => {
                tracing::warn!(error = %err, "the last sleep could not be read");
                self.lid_problems
                    .raise(LidProblemKind::LastSleepRead, LAST_SLEEP_READ_PROBLEM);
            }
        }
    }

    pub(super) fn load_lid(&mut self, ctx: &ModuleCtx<Self>) {
        match stored::load_lid(&ctx.store) {
            Ok(value) => {
                self.lid_on = value.unwrap_or(false);
                self.lid_problems.clear(LidProblemKind::LidRead);
            }
            Err(err) => {
                tracing::warn!(error = %err, "the Lid closed setting could not be read");
                self.lid_on = false;
                self.lid_problems
                    .raise(LidProblemKind::LidRead, LID_READ_PROBLEM);
            }
        }
    }

    /// Registers the watch before the read, so a change in between is not lost.
    pub(super) fn arm_power(&mut self, now: SystemTime) {
        let watch = match self.platform.watch_power() {
            Ok(watch) => watch,
            Err(err) => {
                tracing::warn!(error = %err, "the power source could not be watched");
                self.note_power_dead(now);
                return;
            }
        };
        // The crate forbids unsafe, so it cannot call `AsyncFd::register`; `PowerWatch` owns its
        // descriptor and is never closed elsewhere, which is what `register` requires.
        #[expect(
            deprecated,
            reason = "AsyncFd::register is unsafe and this crate forbids unsafe"
        )]
        let registered = AsyncFd::new(watch);
        match registered {
            Ok(fd) => self.power = Some(fd),
            Err(err) => {
                tracing::warn!(error = %err, "the power source watch could not be polled");
                self.note_power_dead(now);
                return;
            }
        }
        self.read_power(now);
        if self.power.is_some() {
            self.lid_problems.clear(LidProblemKind::Power);
        }
    }

    pub(super) fn disarm_power(&mut self) {
        self.power = None;
        self.on_adapter = None;
    }

    pub(super) fn read_power(&mut self, now: SystemTime) {
        match self.platform.power_kind() {
            Ok(kind) => self.on_adapter = Some(kind == PowerKind::Adapter),
            Err(err) => {
                tracing::warn!(error = %err, "the power source could not be read");
                self.note_power_dead(now);
            }
        }
    }

    pub(super) fn note_power_dead(&mut self, now: SystemTime) {
        self.power = None;
        if !self.lid_on {
            return;
        }
        tracing::warn!("the power-source watch stopped; it reads as battery until the retry");
        self.on_adapter = Some(false);
        self.lid_problems
            .raise(LidProblemKind::Power, POWER_PROBLEM);
        self.schedule_retry(now);
    }

    /// Stores first, so a failure changes nothing, as Tracking agents does.
    pub(super) fn set_lid(
        &mut self,
        ctx: &ModuleCtx<Self>,
        on: bool,
        now: SystemTime,
    ) -> Result<Option<Started>, ActionError> {
        if self.lid_on == on {
            return Ok(None);
        }
        if let Err(err) = stored::save_lid(&ctx.store, on) {
            tracing::warn!(error = %err, "the Lid closed setting could not be stored");
            return Err(ActionError::Failed(
                "the Lid closed setting could not be saved, so it did not change".to_owned(),
            ));
        }
        self.lid_on = on;
        self.lid_problems.clear_all_but_give_back();
        if on {
            self.arm_power(now);
        } else {
            self.disarm_power();
        }
        self.emit_lid_changed(ctx, on);
        Ok(None)
    }

    pub(super) fn emit_lid_changed(&self, ctx: &ModuleCtx<Self>, on: bool) {
        ctx.emit(
            LID_CHANGED,
            Severity::Info,
            event::lid_changed(on).to_owned(),
            json!({ "on": on }),
        );
    }

    pub(super) fn record_sleep(&mut self, ctx: &ModuleCtx<Self>, at_ms: i64) {
        let reason = self.platform.sleep_reason();
        match stored::save_last_sleep(&ctx.store, at_ms) {
            Ok(()) => {
                self.lid_problems.clear(LidProblemKind::LastSleep);
                self.lid_problems.clear(LidProblemKind::LastSleepRead);
            }
            Err(err) => {
                tracing::warn!(error = %err, "the last sleep could not be stored");
                self.lid_problems
                    .raise(LidProblemKind::LastSleep, LAST_SLEEP_PROBLEM);
            }
        }
        self.last_sleep_ms = Some(at_ms);
        ctx.emit(
            event::SLEPT,
            Severity::Info,
            event::slept().to_owned(),
            json!({ "at_ms": at_ms, "reason": reason }),
        );
    }

    /// The change notification can be lost across a sleep, so the source is read again.
    pub(super) fn on_woke(&mut self, now: SystemTime) {
        if self.power.is_some() {
            self.read_power(now);
        }
    }

    pub(super) fn lid_wanted(&self) -> bool {
        self.lid_restored
            && self.lid_on
            && self.table.wants().system
            && self.on_adapter == Some(true)
    }

    pub(super) fn pending_lid_clear(&self) -> bool {
        self.lid_restored && self.lid_marker && self.lid.is_none()
    }

    pub(super) fn reconcile_lid(&mut self, ctx: &ModuleCtx<Self>, now: SystemTime) {
        let want = self.lid_wanted();
        if let Some(guard) = self.lid.take() {
            if want {
                self.lid = Some(guard);
            } else {
                match guard.release() {
                    Ok(()) => {
                        self.lid_given_back(&ctx.store, self.give_back_reason());
                    }
                    Err(err) => self.note_lid_clear_failed(&err, now),
                }
            }
            return;
        }
        if self.pending_lid_clear() && self.retry_at.is_none() {
            match self.platform.give_back_lid() {
                Ok(()) => {
                    self.lid_given_back(&ctx.store, self.give_back_reason());
                }
                Err(err) => self.note_lid_clear_failed(&err, now),
            }
        }
        if want && self.lid.is_none() && !self.lid_marker {
            self.hold_lid(ctx);
        }
    }

    /// Runs until it succeeds or the marker proves unreadable, so a restart never repeats it.
    pub(super) fn restore_lid_once(&mut self, ctx: &ModuleCtx<Self>, now: SystemTime) {
        if self.lid_restored {
            return;
        }
        if !self.lid_marker {
            match stored::load_lid_held(&ctx.store) {
                Ok(true) => self.lid_marker = true,
                Ok(false) => {
                    self.lid_restored = true;
                    return;
                }
                Err(err) => {
                    tracing::warn!(error = %err, "the lid marker could not be read");
                    self.lid_problems
                        .raise(LidProblemKind::MarkerRead, LID_MARKER_READ_PROBLEM);
                    self.lid_restored = true;
                    return;
                }
            }
        }
        match self.platform.give_back_lid() {
            Ok(()) => {
                self.lid_restored = true;
                self.lid_given_back(&ctx.store, "restore");
            }
            Err(err) => self.note_lid_clear_failed(&err, now),
        }
    }

    pub(super) fn release_lid_on_quit(&mut self, ctx: &ModuleCtx<Self>) {
        if let Some(guard) = self.lid.take() {
            match guard.release() {
                Ok(()) => {
                    self.lid_given_back(&ctx.store, "quit");
                }
                Err(err) => tracing::warn!(error = %err, "the lid could not be given back on quit"),
            }
        }
    }

    pub(super) fn give_back_reason(&self) -> &'static str {
        if !self.lid_on {
            "off"
        } else if self.on_adapter != Some(true) {
            "battery"
        } else {
            "lease"
        }
    }

    pub(super) fn log_lid_given_back(&self, reason: &'static str) {
        tracing::info!(
            reason,
            lid_on = self.lid_on,
            on_adapter = ?self.on_adapter,
            "the lid setting was given back"
        );
    }

    pub(super) fn lid_given_back(&mut self, store: &navori_store::Store, reason: &'static str) {
        self.log_lid_given_back(reason);
        self.finish_lid_clear(store);
    }

    pub(super) fn hold_lid(&mut self, ctx: &ModuleCtx<Self>) {
        // The marker goes to disk before the lid changes, so a crash cannot leave the lid set
        // with nothing recording it.
        if let Err(err) = stored::save_lid_held(&ctx.store) {
            tracing::warn!(error = %err, "the lid marker could not be stored");
            self.lid_problems
                .raise(LidProblemKind::Marker, LID_MARKER_PROBLEM);
            return;
        }
        self.lid_marker = true;
        self.clear_lid_hold_problem();
        match self.platform.hold_lid() {
            Ok(guard) => {
                self.lid = Some(guard);
                self.clear_lid_hold_problem();
                tracing::info!(reason = "lease", on_adapter = ?self.on_adapter, "the lid is held");
            }
            Err(err) => {
                tracing::warn!(error = %err, "the lid could not be held");
                self.lid_on = false;
                self.disarm_power();
                if let Err(err) = stored::save_lid(&ctx.store, false) {
                    tracing::warn!(error = %err, "the Lid closed setting could not be stored");
                    self.lid_problems
                        .raise(LidProblemKind::SaveOff, LID_SAVE_OFF_PROBLEM);
                }
                self.emit_lid_changed(ctx, false);
                self.finish_lid_clear(&ctx.store);
                self.lid_problems
                    .raise(LidProblemKind::Refused, LID_REFUSED_PROBLEM);
            }
        }
    }

    /// The lid is clear, so the marker goes; a failed delete leaves the mirror set and retries.
    pub(super) fn finish_lid_clear(&mut self, store: &navori_store::Store) {
        match stored::delete_lid_held(store) {
            Ok(()) => {
                self.lid_marker = false;
                self.clear_lid_hold_problem();
            }
            Err(err) => tracing::warn!(error = %err, "the lid marker could not be deleted"),
        }
    }

    pub(super) fn note_lid_clear_failed(&mut self, err: &navori_macos::LidError, now: SystemTime) {
        tracing::warn!(error = %err, "the lid setting could not be given back");
        self.lid_problems
            .raise(LidProblemKind::Clear, LID_CLEAR_PROBLEM);
        self.schedule_retry(now);
    }

    pub(super) fn clear_lid_hold_problem(&mut self) {
        self.lid_problems.clear(LidProblemKind::Marker);
        self.lid_problems.clear(LidProblemKind::Refused);
        self.lid_problems.clear(LidProblemKind::Clear);
    }
}

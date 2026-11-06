use std::time::{Duration, SystemTime};

use navori_kernel::{ActionError, ModuleCtx, Severity};
use navori_procs::{ProcId, ProcsError};

use super::{
    EXITS_PROBLEM, MAX_SECONDS, Started, check_subagent_ids, platform_failed, process_of,
    rejected_hold, truncate_us, valid_session_id, watch_failed,
};
use crate::domain::{DisplayChange, Ended, LeaseKey};
use crate::event::{self, TRACKING_CHANGED};
use crate::infra::stored;
use crate::infra::{ExecutableName, ProcessClass};
use serde_json::json;

impl super::Awake {
    /// Runs on every entry, so a restart repairs itself. A read failure keeps the current
    /// value: on at the first entry, which leaves the Mac awake rather than asleep mid-run.
    pub(super) fn load_tracking(&mut self, ctx: &ModuleCtx<Self>) {
        match stored::load_tracking(&ctx.store) {
            Ok(Some(on)) => self.table.set_tracking(on),
            Ok(None) => {}
            Err(err) => tracing::warn!(error = %err, "the tracking setting could not be read"),
        }
    }

    pub(super) fn keep_for(
        &mut self,
        ctx: &ModuleCtx<Self>,
        now: SystemTime,
        seconds: u64,
    ) -> Result<Option<Started>, ActionError> {
        let replaced = self.hold_for(now, seconds)?;
        if let Some(ended) = replaced {
            self.emit_ended(ctx, std::slice::from_ref(&ended));
        }
        self.display(ctx, false, now);
        Ok(Some(Started::Duration(seconds)))
    }

    pub(super) fn keep_indefinitely(
        &mut self,
        ctx: &ModuleCtx<Self>,
        now: SystemTime,
    ) -> Option<Started> {
        if let Some(ended) = self.table.end_manual() {
            self.emit_ended(ctx, std::slice::from_ref(&ended));
        }
        self.display(ctx, true, now)
    }

    /// Stores first, so a failure changes nothing, as Release all does.
    pub(super) fn set_tracking(
        &mut self,
        ctx: &ModuleCtx<Self>,
        on: bool,
    ) -> Result<Option<Started>, ActionError> {
        if self.table.tracking() == on {
            return Ok(None);
        }
        if let Err(err) = stored::save_tracking(&ctx.store, on) {
            tracing::warn!(error = %err, "the tracking setting could not be stored");
            return Err(ActionError::Failed(
                "the Tracking agents setting could not be saved, so it did not change".to_owned(),
            ));
        }
        self.table.set_tracking(on);
        ctx.emit(
            TRACKING_CHANGED,
            Severity::Info,
            event::tracking_changed(on).to_owned(),
            json!({ "on": on }),
        );
        Ok(None)
    }

    pub(super) fn hold_until_off(&mut self, now: SystemTime) -> Option<Ended> {
        let started = truncate_us(now);
        self.table.hold_until_off(started)
    }

    pub(super) fn display(
        &mut self,
        ctx: &ModuleCtx<Self>,
        on: bool,
        now: SystemTime,
    ) -> Option<Started> {
        let started = truncate_us(now);
        match self.table.set_display(on, started) {
            DisplayChange::Created => Some(Started::Display),
            DisplayChange::Ended(ended) => {
                self.emit_ended(ctx, std::slice::from_ref(&ended));
                None
            }
            DisplayChange::Unchanged => None,
        }
    }

    pub(super) fn hold_for(
        &mut self,
        now: SystemTime,
        seconds: u64,
    ) -> Result<Option<Ended>, ActionError> {
        if seconds == 0 || seconds > MAX_SECONDS {
            return Err(ActionError::Invalid(format!(
                "duration must be from 1 second through 7 days, not {seconds} seconds"
            )));
        }
        let started = truncate_us(now);
        let Some(deadline) = started.checked_add(Duration::from_secs(seconds)) else {
            return Err(ActionError::Invalid(format!(
                "duration of {seconds} seconds does not fit on the clock; use 7 days or less"
            )));
        };
        let deadline = truncate_us(deadline);
        Ok(self.table.hold_for(started, deadline))
    }

    pub(super) fn hold_command(
        &mut self,
        ctx: &ModuleCtx<Self>,
        now: SystemTime,
        pid: i32,
    ) -> Result<Option<Started>, ActionError> {
        if pid <= 0 {
            return Err(ActionError::Invalid(format!(
                "pid must be positive, not {pid}"
            )));
        }
        let id = match self.platform.classify(pid).map_err(platform_failed)? {
            ProcessClass::Live(id) => id,
            ProcessClass::OtherUser(_) => {
                return Err(ActionError::Invalid(format!(
                    "pid {pid} belongs to another user"
                )));
            }
            ProcessClass::Finished => return Ok(None),
            ProcessClass::Unreadable => {
                return Err(platform_failed("reading the process failed"));
            }
        };
        let label = match self
            .platform
            .executable_name(id.pid)
            .map_err(platform_failed)?
        {
            ExecutableName::Gone => return Ok(None),
            ExecutableName::Known(None) => {
                tracing::debug!(pid = id.pid, "the executable name could not be read");
                None
            }
            ExecutableName::Known(name) => name,
        };
        if self.table.has_command(id) {
            return Ok(None);
        }
        self.arm(id)?;
        self.table.hold_command(id, truncate_us(now), label.clone());
        self.save_command(ctx, now, id);
        let key = LeaseKey::Command {
            pid: id.pid,
            start_us: id.start.0,
        }
        .text();
        Ok(Some(Started::Command {
            key,
            name: label.unwrap_or_else(|| "command".to_owned()),
        }))
    }

    pub(super) async fn hold_session(
        &mut self,
        ctx: &ModuleCtx<Self>,
        now: SystemTime,
        session_id: String,
        pid: i32,
        ppid: i32,
        cwd: Option<&str>,
    ) -> Result<Option<Started>, ActionError> {
        if !valid_session_id(&session_id) {
            return Err(ActionError::Invalid(
                "session_id must be 1 to 128 bytes of letters, digits, '.', '_', ':', or '-'"
                    .to_owned(),
            ));
        }
        if pid <= 0 {
            return Err(ActionError::Invalid(format!(
                "pid must be positive, not {pid}"
            )));
        }
        if ppid < 0 {
            return Err(ActionError::Invalid(format!(
                "ppid must be zero or positive, not {ppid}"
            )));
        }
        // First hold is this arrival, before the snapshot.
        let arrived = truncate_us(now);
        let found = match self.platform.resolve_session(pid, ppid, cwd).await {
            Ok(Ok(found)) => found,
            Ok(Err(rejected)) => return Err(rejected_hold(rejected, pid, ppid)),
            Err(err) => {
                tracing::error!(%err, "session snapshot task stopped");
                return Err(ActionError::Failed(
                    "the process list could not be read (snapshot task stopped); retry the hold"
                        .to_owned(),
                ));
            }
        };
        let (harness, process, place) = found;
        let Some(deadline) = arrived.checked_add(self.table.timing().session_ttl) else {
            return Err(ActionError::Invalid(
                "session ttl does not fit on the clock; retry the hold".to_owned(),
            ));
        };
        let deadline = truncate_us(deadline);
        let previous = self.table.session_process(&session_id);
        if previous != Some(process) {
            self.arm(process)?;
        }
        let created = self.table.hold_session(
            session_id.clone(),
            harness,
            process,
            arrived,
            deadline,
            place,
        );
        if let Some(old) = previous.filter(|old| *old != process) {
            self.unwatch_if_idle(old);
        }
        self.save_session(ctx, now, &session_id);
        if !created {
            return Ok(None);
        }
        Ok(Some(Started::Session {
            key: LeaseKey::Session { session_id }.text(),
            name: harness.display().to_owned(),
        }))
    }

    pub(super) async fn subagent_start(
        &mut self,
        ctx: &ModuleCtx<Self>,
        now: SystemTime,
        session_id: &str,
        agent_id: &str,
        pid: i32,
        ppid: i32,
    ) -> Result<Option<Started>, ActionError> {
        check_subagent_ids(session_id, agent_id)?;
        let arrived = truncate_us(now);
        let Some(deadline) = arrived.checked_add(self.table.timing().session_ttl) else {
            return Err(ActionError::Invalid(
                "session ttl does not fit on the clock; retry the hold".to_owned(),
            ));
        };
        let deadline = truncate_us(deadline);
        // A session with a lease needs no process read. One with none gets a Stop mark: its
        // main turn may be over, so no `Stop` may ever come, and grace must end it.
        let started = if self.table.session_process(session_id).is_some() {
            None
        } else {
            let started = self
                .hold_session(ctx, now, session_id.to_owned(), pid, ppid, None)
                .await?;
            self.table.stop_session(session_id, arrived, false);
            started
        };
        self.table.subagent_started(session_id, agent_id, deadline);
        self.save_session(ctx, now, session_id);
        Ok(started)
    }

    pub(super) fn subagent_stop(
        &mut self,
        now: SystemTime,
        session_id: &str,
        agent_id: &str,
        background: bool,
    ) -> Result<Option<Started>, ActionError> {
        check_subagent_ids(session_id, agent_id)?;
        self.table
            .subagent_stopped(session_id, agent_id, now, background);
        Ok(None)
    }

    pub(super) fn end_session(
        &mut self,
        ctx: &ModuleCtx<Self>,
        session_id: &str,
        now: SystemTime,
    ) -> Result<Option<Started>, ActionError> {
        if !valid_session_id(session_id) {
            return Err(ActionError::Invalid(
                "session_id must be 1 to 128 bytes of letters, digits, '.', '_', ':', or '-'"
                    .to_owned(),
            ));
        }
        if let Some(ended) = self.table.end_session(session_id) {
            if let Some(id) = process_of(&ended.lease) {
                self.unwatch_if_idle(id);
            }
            self.forget_ended(ctx, now, std::slice::from_ref(&ended));
            self.emit_ended(ctx, &[ended]);
        }
        Ok(None)
    }

    pub(super) fn stop_session(
        &mut self,
        ctx: &ModuleCtx<Self>,
        session_id: &str,
        now: SystemTime,
        background: bool,
    ) -> Result<Option<Started>, ActionError> {
        if !valid_session_id(session_id) {
            return Err(ActionError::Invalid(
                "session_id must be 1 to 128 bytes of letters, digits, '.', '_', ':', or '-'"
                    .to_owned(),
            ));
        }
        self.table.stop_session(session_id, now, background);
        self.save_session(ctx, now, session_id);
        Ok(None)
    }

    pub(super) fn release_all(
        &mut self,
        ctx: &ModuleCtx<Self>,
    ) -> Result<Option<Started>, ActionError> {
        if let Err(err) = stored::clear(&ctx.store) {
            tracing::warn!(error = %err, "release all could not delete stored leases");
            // No dirty key, so a store problem here would never be retried.
            return Err(ActionError::Failed(
                "stored leases could not be deleted, so nothing was released".to_owned(),
            ));
        }
        self.dirty.clear();
        self.problems.store = None;
        let ended = self.table.release_all();
        let ids: Vec<ProcId> = ended
            .iter()
            .filter_map(|end| process_of(&end.lease))
            .collect();
        for id in ids {
            self.unwatch_if_idle(id);
        }
        self.emit_ended(ctx, &ended);
        Ok(None)
    }

    pub(super) fn arm(&mut self, id: ProcId) -> Result<(), ActionError> {
        if self.table.leases_on(id) > 0 {
            return Ok(());
        }
        if self.watcher.is_none() {
            let watcher = self
                .platform
                .build_watcher()
                .map_err(|source| watch_failed(ProcsError::Platform { source }))?;
            self.watcher = Some(watcher);
        }
        let result = {
            let Some(watcher) = self.watcher.as_mut() else {
                return Err(ActionError::Failed(
                    "exit watches could not be started; no command lease was held".to_owned(),
                ));
            };
            watcher.watch(id)
        };
        if let Err(err) = result {
            if self.table.process_ids().is_empty() {
                self.watcher = None;
            }
            return Err(watch_failed(err));
        }
        Ok(())
    }

    pub(super) fn on_process_exit(&mut self, ctx: &ModuleCtx<Self>, id: ProcId, now: SystemTime) {
        let ended = self.table.on_exit(id);
        self.unwatch_if_idle(id);
        self.forget_ended(ctx, now, &ended);
        self.emit_ended(ctx, &ended);
    }

    pub(super) fn unwatch_if_idle(&mut self, id: ProcId) {
        if self.table.leases_on(id) > 0 {
            return;
        }
        if let Some(watcher) = self.watcher.as_mut() {
            watcher.unwatch(id);
        }
        if self.table.process_ids().is_empty() {
            self.watcher = None;
            self.problems.exits = None;
        }
    }

    pub(super) fn note_exits_dead(&mut self, now: SystemTime) {
        self.watcher = None;
        if self.table.process_ids().is_empty() {
            self.problems.exits = None;
            return;
        }
        tracing::warn!("exit watches stopped; a command lease has no end signal until the retry");
        self.problems.exits = Some(EXITS_PROBLEM.to_owned());
        self.schedule_retry(now);
    }

    pub(super) fn rebuild_exits_now(&mut self, now: SystemTime) {
        if self.table.process_ids().is_empty() {
            self.problems.exits = None;
            return;
        }
        match self.rebuild_watcher() {
            Ok(()) => self.problems.exits = None,
            Err(err) => {
                tracing::warn!(error = %err, "exit watches could not be rebuilt");
                self.problems.exits = Some(EXITS_PROBLEM.to_owned());
                self.schedule_retry(now);
            }
        }
    }

    pub(super) fn rebuild_watcher(&mut self) -> Result<(), String> {
        let mut watcher = self
            .platform
            .build_watcher()
            .map_err(|err| err.to_string())?;
        for id in self.table.process_ids() {
            if let Err(err) = watcher.watch(id) {
                self.watcher = Some(watcher);
                return Err(err.to_string());
            }
        }
        self.watcher = Some(watcher);
        Ok(())
    }
}

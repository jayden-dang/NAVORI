use std::collections::BTreeSet;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use navori_kernel::{ActionError, ModuleCtx, Request, Severity, WaitHandle, Wake};
use navori_macos::StartTime;
use navori_procs::{Exited, ProcId, ProcWatcher, ProcsError};
use serde_json::{Value, json};

use crate::action::AwakeAction;
use crate::domain::{
    self, EndReason, Ended, Held, Lease, LeaseKey, LeaseSource, LeaseTable, Reconciler, Timing,
};
use crate::event::{self, LEASE_ENDED, LEASE_STARTED};
use crate::infra::stored::{self, StoredKind, StoredLease};
use crate::infra::{ExecutableName, Platform, ProcessClass, Rejected};
use crate::state::AwakeState;

const MAX_SECONDS: u64 = 7 * 24 * 60 * 60;
const RETRY: Duration = Duration::from_secs(60);
const SYSTEM_PROBLEM: &str = "macOS refused the request. Navori keeps trying.";
const DISPLAY_PROBLEM: &str = "macOS refused to keep the display on. Navori keeps trying.";
const RELEASE_PROBLEM: &str = "Releasing a power assertion failed.";
const EXITS_PROBLEM: &str =
    "Exit watches stopped, so a command lease has no end signal. Navori retries in 60 seconds.";
const STORE_PROBLEM: &str = "Awake could not save a lease. Navori retries in 60 seconds.";
const PLATFORM_PROBLEM: &str = "Awake could not watch a process, so a stored lease stays on disk. Navori retries in 60 seconds.";

#[derive(Debug, Default)]
struct Problems {
    system: Option<String>,
    display: Option<String>,
    release: Option<String>,
    store: Option<String>,
    platform: Option<String>,
    exits: Option<String>,
}

impl Problems {
    fn first(&self) -> Option<String> {
        self.system
            .clone()
            .or_else(|| self.display.clone())
            .or_else(|| self.release.clone())
            .or_else(|| self.store.clone())
            .or_else(|| self.platform.clone())
            .or_else(|| self.exits.clone())
    }
}

pub struct Awake {
    platform: Platform,
    table: LeaseTable,
    reconciler: Reconciler,
    published: AwakeState,
    watcher: Option<ProcWatcher>,
    problems: Problems,
    retry_at: Option<SystemTime>,
    restored: bool,
    dirty: BTreeSet<String>,
}

impl std::fmt::Debug for Awake {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Awake")
            .field("platform", &self.platform)
            .field("table", &self.table)
            .field("reconciler", &self.reconciler)
            .finish_non_exhaustive()
    }
}

impl Awake {
    pub fn new(platform: Platform, name: impl Into<String>, timing: Timing) -> Self {
        let reconciler = Reconciler::new(platform.power(), name);
        Self {
            platform,
            table: LeaseTable::new(timing),
            reconciler,
            published: AwakeState::idle(),
            watcher: None,
            problems: Problems::default(),
            retry_at: None,
            restored: false,
            dirty: BTreeSet::new(),
        }
    }

    pub(crate) async fn run_loop(&mut self, ctx: &mut ModuleCtx<Self>) -> anyhow::Result<()> {
        let mut wait = ctx.wait_handle();
        stored::migrate(&ctx.store)?;
        self.restore_if_needed(ctx, wait.now()).await;
        loop {
            let now = wait.now();
            let due = self.take_due_retry(now);
            if due && self.problems.exits.is_some() {
                self.rebuild_exits_now(now);
            }
            if due && !self.restored {
                self.restore_if_needed(ctx, now).await;
            }
            self.turn(ctx, now).await;
            let deadline = self.deadline();
            tokio::select! {
                biased;
                exit = watcher_next(&mut self.watcher) => {
                    match exit {
                        Some(exited) => self.on_process_exit(ctx, exited.id, wait.now()),
                        None => self.note_exits_dead(wait.now()),
                    }
                }
                req = ctx.recv() => {
                    let Some(req) = req else {
                        return Ok(());
                    };
                    self.on_request(ctx, &wait, req).await;
                }
                wake = wait.wait_until(deadline) => {
                    match wake {
                        Wake::Due => {}
                        Wake::PanelChanged => {}
                    }
                }
            }
        }
    }

    fn deadline(&self) -> Option<SystemTime> {
        match (self.table.next_deadline(), self.retry_at) {
            (Some(lease), Some(retry)) => Some(lease.min(retry)),
            (lease, retry) => lease.or(retry),
        }
    }

    async fn on_request(
        &mut self,
        ctx: &ModuleCtx<Self>,
        wait: &WaitHandle,
        req: Request<AwakeAction>,
    ) {
        let now = wait.now();
        let started = match &req.action {
            AwakeAction::Status => Ok(None),
            AwakeAction::HoldFor { seconds } => self
                .hold_for(now, *seconds)
                .map(|()| Some(Started::Duration(*seconds))),
            AwakeAction::HoldUntilOff => {
                self.hold_until_off(now);
                Ok(Some(Started::UntilOff))
            }
            AwakeAction::Display { on } => Ok(self.display(*on, now)),
            AwakeAction::HoldCommand { pid } => self.hold_command(ctx, now, *pid),
            AwakeAction::HoldSession {
                session_id,
                pid,
                ppid,
            } => {
                self.hold_session(ctx, now, session_id.clone(), *pid, *ppid)
                    .await
            }
            AwakeAction::StopSession { session_id } => {
                self.stop_session(ctx, session_id, truncate_us(now))
            }
            AwakeAction::EndSession { session_id } => self.end_session(ctx, session_id, now),
            AwakeAction::ReleaseAll => self.release_all(ctx),
        };
        match started {
            Ok(started) => {
                if let Some(started) = started {
                    self.emit_started(ctx, started);
                }
                let state = self.turn(ctx, now).await;
                req.respond(Ok(state));
            }
            Err(err) => req.respond::<AwakeState>(Err(err)),
        }
    }

    fn hold_until_off(&mut self, now: SystemTime) {
        let started = truncate_us(now);
        // Replacement keeps the assertion; no locked end sentence is true.
        let _replaced = self.table.hold_until_off(started);
    }

    fn display(&mut self, on: bool, now: SystemTime) -> Option<Started> {
        let started = truncate_us(now);
        self.table
            .set_display(on, started)
            .then_some(Started::Display)
    }

    fn hold_for(&mut self, now: SystemTime, seconds: u64) -> Result<(), ActionError> {
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
        // Replacement keeps the assertion; no locked end sentence is true.
        let _replaced = self.table.hold_for(started, deadline);
        Ok(())
    }

    fn hold_command(
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
            ProcessClass::Finished | ProcessClass::Unreadable => return Ok(None),
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

    async fn hold_session(
        &mut self,
        ctx: &ModuleCtx<Self>,
        now: SystemTime,
        session_id: String,
        pid: i32,
        ppid: i32,
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
        let found = match self.platform.resolve_session(pid, ppid).await {
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
        let (harness, process, label) = found;
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
            label,
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
            name: domain::harness_name(harness).to_owned(),
        }))
    }

    fn end_session(
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
            // No locked end sentence is true for EndSession.
            self.emit_ended(ctx, &[ended]);
        }
        Ok(None)
    }

    fn stop_session(
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
        self.table.stop_session(session_id, now);
        self.save_session(ctx, now, session_id);
        Ok(None)
    }

    fn release_all(&mut self, ctx: &ModuleCtx<Self>) -> Result<Option<Started>, ActionError> {
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

    fn arm(&mut self, id: ProcId) -> Result<(), ActionError> {
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

    fn on_process_exit(&mut self, ctx: &ModuleCtx<Self>, id: ProcId, now: SystemTime) {
        let ended = self.table.on_exit(id);
        self.unwatch_if_idle(id);
        self.forget_ended(ctx, now, &ended);
        self.emit_ended(ctx, &ended);
    }

    fn unwatch_if_idle(&mut self, id: ProcId) {
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

    fn note_exits_dead(&mut self, now: SystemTime) {
        self.watcher = None;
        if self.table.process_ids().is_empty() {
            self.problems.exits = None;
            return;
        }
        tracing::warn!("exit watches stopped; a command lease has no end signal until the retry");
        self.problems.exits = Some(EXITS_PROBLEM.to_owned());
        self.schedule_retry(now);
    }

    fn rebuild_exits_now(&mut self, now: SystemTime) {
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

    fn rebuild_watcher(&mut self) -> Result<(), String> {
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

    async fn turn(&mut self, ctx: &ModuleCtx<Self>, now: SystemTime) -> AwakeState {
        let settled = self.table.settle(now);
        for end in &settled.ended {
            if let Some(id) = process_of(&end.lease) {
                self.unwatch_if_idle(id);
            }
        }
        self.forget_ended(ctx, now, &settled.ended);
        self.emit_ended(ctx, &settled.ended);
        for (key, process, first_hold) in settled.to_check {
            let reading = self.platform.read_descendants(process, first_hold).await;
            if let Some(ended) = self.table.resolve_check(&key, reading, now) {
                if let Some(id) = process_of(&ended.lease) {
                    self.unwatch_if_idle(id);
                }
                self.forget_ended(ctx, now, std::slice::from_ref(&ended));
                self.emit_ended(ctx, &[ended]);
            }
        }
        let held = self.reconciler.apply(self.table.wants());
        self.note_assertions(&held);
        self.flush_dirty(ctx, now);
        self.sync_retry(now, &held);
        self.publish(ctx, &held)
    }

    fn note_assertions(&mut self, held: &Held) {
        let wants = self.table.wants();
        self.problems.system = (wants.system && !held.system).then(|| SYSTEM_PROBLEM.to_owned());
        self.problems.display =
            (wants.display && !held.display).then(|| DISPLAY_PROBLEM.to_owned());
        if let Some(code) = held.release_failed {
            tracing::warn!(code, "releasing a power assertion failed");
            self.problems.release = Some(RELEASE_PROBLEM.to_owned());
        } else if held.released {
            self.problems.release = None;
        }
    }

    fn sync_retry(&mut self, now: SystemTime, held: &Held) {
        if !self.needs_retry(held) {
            self.retry_at = None;
            return;
        }
        if self.retry_at.is_none() {
            self.schedule_retry(now);
        }
    }

    fn needs_retry(&self, held: &Held) -> bool {
        let wants = self.table.wants();
        (wants.system && !held.system)
            || (wants.display && !held.display)
            || !self.dirty.is_empty()
            || self.problems.platform.is_some()
            || self.problems.exits.is_some()
            || !self.restored
    }

    fn publish(&mut self, ctx: &ModuleCtx<Self>, held: &Held) -> AwakeState {
        let mut state = self.table.snapshot(held.system, held.display);
        state.problem = self.problems.first();
        if state != self.published {
            self.published = state.clone();
            let next = state.clone();
            ctx.update_state(|slot| *slot = next);
        }
        state
    }

    fn emit_started(&self, ctx: &ModuleCtx<Self>, started: Started) {
        let (key, source, kind) = match started {
            Started::Duration(seconds) => (
                "manual".to_owned(),
                event::duration_source(seconds),
                "duration",
            ),
            Started::UntilOff => (
                "manual".to_owned(),
                event::until_off_source().to_owned(),
                "until_off",
            ),
            Started::Display => (
                "display".to_owned(),
                event::display_source().to_owned(),
                "display",
            ),
            Started::Command { key, name } => (key, name, "command"),
            Started::Session { key, name } => (key, name, "session"),
        };
        ctx.emit(
            LEASE_STARTED,
            Severity::Info,
            event::started(&source),
            payload(&key, kind),
        );
    }

    fn emit_ended(&self, ctx: &ModuleCtx<Self>, ended: &[Ended]) {
        let mut released = false;
        for end in ended {
            match end.reason {
                EndReason::Deadline => ctx.emit(
                    LEASE_ENDED,
                    Severity::Info,
                    event::ended_deadline(&source_text(&end.lease)),
                    payload(&end.lease.key.text(), kind_of(&end.lease)),
                ),
                EndReason::ProcessExited => ctx.emit(
                    LEASE_ENDED,
                    Severity::Info,
                    event::ended_exited(&source_text(&end.lease)),
                    payload(&end.lease.key.text(), kind_of(&end.lease)),
                ),
                EndReason::GraceClean => ctx.emit(
                    LEASE_ENDED,
                    Severity::Info,
                    event::ended_grace(&source_text(&end.lease)),
                    payload(&end.lease.key.text(), kind_of(&end.lease)),
                ),
                EndReason::Cap => ctx.emit(
                    LEASE_ENDED,
                    Severity::Warn,
                    event::ended_cap(&source_text(&end.lease)),
                    payload(&end.lease.key.text(), kind_of(&end.lease)),
                ),
                EndReason::ReleaseAll if !released => {
                    released = true;
                    ctx.emit(
                        LEASE_ENDED,
                        Severity::Info,
                        event::ended_release_all(),
                        payload(&end.lease.key.text(), kind_of(&end.lease)),
                    );
                }
                EndReason::EndSession | EndReason::ReleaseAll | EndReason::Replaced => {}
            }
        }
    }

    fn take_due_retry(&mut self, now: SystemTime) -> bool {
        match self.retry_at {
            Some(due) if now >= due => {
                self.retry_at = None;
                true
            }
            _ => false,
        }
    }

    fn schedule_retry(&mut self, now: SystemTime) {
        let Some(at) = now.checked_add(RETRY) else {
            return;
        };
        self.retry_at = Some(match self.retry_at {
            Some(existing) => existing.min(at),
            None => at,
        });
    }

    fn note_store(&mut self, now: SystemTime) {
        self.problems.store = Some(STORE_PROBLEM.to_owned());
        self.schedule_retry(now);
    }

    fn note_platform(&mut self, now: SystemTime, err: impl std::fmt::Display) {
        tracing::warn!(error = %err, "a stored lease could not be watched");
        self.problems.platform = Some(PLATFORM_PROBLEM.to_owned());
        self.schedule_retry(now);
    }

    fn save_session(&mut self, ctx: &ModuleCtx<Self>, now: SystemTime, session_id: &str) {
        let Some(saved) = self.table.session_saved(session_id) else {
            return;
        };
        self.upsert_row(ctx, now, stored_session(&saved));
    }

    fn save_command(&mut self, ctx: &ModuleCtx<Self>, now: SystemTime, process: ProcId) {
        if !self.table.has_command(process) {
            return;
        }
        self.upsert_row(ctx, now, stored_command(process));
    }

    fn upsert_row(&mut self, ctx: &ModuleCtx<Self>, now: SystemTime, row: StoredLease) {
        match stored::upsert(&ctx.store, &row) {
            Ok(()) => self.clear_dirty(&row.key),
            Err(err) => {
                tracing::warn!(error = %err, key = %row.key, "lease row was not written");
                self.dirty.insert(row.key);
                self.note_store(now);
            }
        }
    }

    fn delete_stored(&mut self, ctx: &ModuleCtx<Self>, now: SystemTime, key: &str) {
        match stored::delete(&ctx.store, key) {
            Ok(()) => self.clear_dirty(key),
            Err(err) => {
                tracing::warn!(error = %err, key, "lease row was not deleted");
                self.dirty.insert(key.to_owned());
                self.note_store(now);
            }
        }
    }

    fn clear_dirty(&mut self, key: &str) {
        self.dirty.remove(key);
        if self.dirty.is_empty() {
            self.problems.store = None;
        }
    }

    fn forget_ended(&mut self, ctx: &ModuleCtx<Self>, now: SystemTime, ended: &[Ended]) {
        for end in ended {
            if let Some(key) = stored_key(&end.lease) {
                self.delete_stored(ctx, now, &key);
            }
        }
    }

    fn flush_dirty(&mut self, ctx: &ModuleCtx<Self>, now: SystemTime) {
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

    fn row_for(&self, key: &str) -> Option<StoredLease> {
        self.table
            .saved_sessions()
            .into_iter()
            .map(|saved| stored_session(&saved))
            .chain(self.table.saved_commands().into_iter().map(stored_command))
            .find(|row| row.key == key)
    }

    async fn restore_if_needed(&mut self, ctx: &ModuleCtx<Self>, now: SystemTime) {
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

    fn installed_row_remains(&self, row: &StoredLease, id: ProcId) -> bool {
        match row.kind {
            StoredKind::Command => self.table.has_command(id),
            StoredKind::Session => row
                .session_id
                .as_deref()
                .is_some_and(|session_id| self.table.session_process(session_id).is_some()),
        }
    }

    /// One snapshot for every row that is still installed after settle.
    async fn label_installed(&mut self, rows: &[(StoredLease, ProcId)]) {
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
            let label = match row.kind {
                StoredKind::Command => {
                    let label = label_at(
                        labels.as_ref().map(|pair| pair.commands.as_slice()),
                        command_at,
                    );
                    command_at += 1;
                    label
                }
                StoredKind::Session => {
                    let label = label_at(
                        labels.as_ref().map(|pair| pair.sessions.as_slice()),
                        session_at,
                    );
                    session_at += 1;
                    label
                }
            };
            match row.kind {
                StoredKind::Command => self.table.set_command_label(*id, label),
                StoredKind::Session => {
                    if let Some(session_id) = row.session_id.as_deref() {
                        self.table.set_session_label(session_id, label);
                    }
                }
            }
        }
    }

    fn classify_row(
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

    fn install_row(&mut self, row: &StoredLease, id: ProcId, label: Option<String>) -> bool {
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
                    label,
                );
                if let Some(mark) = row.stop_mark {
                    self.table.stop_session(&session_id, mark);
                }
                true
            }
        }
    }

    fn disarm_unheld(&mut self, armed: &[ProcId]) {
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

    fn finish_restore(&mut self) {
        self.restored = true;
        self.problems.platform = None;
        if self.dirty.is_empty() {
            self.problems.store = None;
        }
    }
}

enum RowFate {
    Keep(ProcId),
    Drop,
    /// This row stays stored. Other rows in the pass are still finished.
    Retry,
    Halt,
}

enum Started {
    Duration(u64),
    UntilOff,
    Display,
    Command { key: String, name: String },
    Session { key: String, name: String },
}

async fn watcher_next(watcher: &mut Option<ProcWatcher>) -> Option<Exited> {
    match watcher.as_mut() {
        Some(watcher) => watcher.next().await,
        None => std::future::pending().await,
    }
}

fn platform_failed(err: impl std::fmt::Display) -> ActionError {
    ActionError::Failed(format!(
        "the process list could not be read ({err}); retry the hold"
    ))
}

fn watch_failed(err: ProcsError) -> ActionError {
    match err {
        ProcsError::Watch { errno } => ActionError::Failed(format!(
            "kqueue failed with errno {errno}; no command lease was held"
        )),
        ProcsError::Platform { source } => ActionError::Failed(format!(
            "exit watches could not be started ({source}); no command lease was held"
        )),
    }
}

fn source_text(lease: &Lease) -> String {
    match &lease.source {
        LeaseSource::Duration { started } => {
            let seconds = lease
                .deadline
                .and_then(|deadline| deadline.duration_since(*started).ok())
                .map_or(0, |span| span.as_secs());
            event::duration_source(seconds)
        }
        LeaseSource::UntilOff => event::until_off_source().to_owned(),
        LeaseSource::Display => event::display_source().to_owned(),
        LeaseSource::Command { .. } => lease.label.clone().unwrap_or_else(|| "command".to_owned()),
        LeaseSource::Session { harness, .. } => domain::harness_name(*harness).to_owned(),
    }
}

fn kind_of(lease: &Lease) -> &'static str {
    match lease.source {
        LeaseSource::Duration { .. } => "duration",
        LeaseSource::UntilOff => "until_off",
        LeaseSource::Display => "display",
        LeaseSource::Command { .. } => "command",
        LeaseSource::Session { .. } => "session",
    }
}

fn process_of(lease: &Lease) -> Option<ProcId> {
    match lease.source {
        LeaseSource::Command { process } | LeaseSource::Session { process, .. } => Some(process),
        LeaseSource::Duration { .. } | LeaseSource::UntilOff | LeaseSource::Display => None,
    }
}

fn valid_session_id(session_id: &str) -> bool {
    let len = session_id.len();
    (1..=128).contains(&len)
        && session_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b':' | b'-'))
}

fn rejected_hold(rejected: Rejected, pid: i32, ppid: i32) -> ActionError {
    match rejected {
        Rejected::NotRunning => ActionError::Invalid(format!(
            "pid {pid} and parent {ppid} are not running; no session lease was held"
        )),
        Rejected::NotAHarness => ActionError::Invalid(format!(
            "pid {pid} is not inside a harness; no session lease was held"
        )),
        Rejected::Platform(err) => platform_failed(err),
    }
}

fn payload(key: &str, kind: &str) -> Value {
    json!({ "key": key, "kind": kind })
}

fn stored_key(lease: &Lease) -> Option<String> {
    match lease.source {
        LeaseSource::Command { .. } | LeaseSource::Session { .. } => Some(lease.key.text()),
        LeaseSource::Duration { .. } | LeaseSource::UntilOff | LeaseSource::Display => None,
    }
}

fn stored_session(saved: &domain::SessionSaved) -> StoredLease {
    StoredLease {
        key: LeaseKey::Session {
            session_id: saved.session_id.clone(),
        }
        .text(),
        kind: StoredKind::Session,
        session_id: Some(saved.session_id.clone()),
        harness: Some(saved.harness),
        pid: saved.process.pid,
        process_start_us: saved.process.start.0,
        first_hold: Some(saved.first_hold),
        stop_mark: saved.stop_mark,
        deadline: Some(saved.deadline),
    }
}

fn stored_command(process: ProcId) -> StoredLease {
    StoredLease {
        key: LeaseKey::Command {
            pid: process.pid,
            start_us: process.start.0,
        }
        .text(),
        kind: StoredKind::Command,
        session_id: None,
        harness: None,
        pid: process.pid,
        process_start_us: process.start.0,
        first_hold: None,
        stop_mark: None,
        deadline: None,
    }
}

fn label_at(labels: Option<&[Option<String>]>, index: usize) -> Option<String> {
    labels
        .and_then(|labels| labels.get(index).cloned())
        .flatten()
}

fn truncate_us(time: SystemTime) -> SystemTime {
    let Ok(duration) = time.duration_since(UNIX_EPOCH) else {
        return UNIX_EPOCH;
    };
    match u64::try_from(duration.as_micros()) {
        Ok(micros) => UNIX_EPOCH + Duration::from_micros(micros),
        Err(_) => time,
    }
}

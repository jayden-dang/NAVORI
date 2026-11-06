use std::collections::BTreeSet;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use navori_kernel::{ActionError, ModuleCtx, Request, Severity, WaitHandle, Wake};
use navori_macos::{LidGuard, PowerKind, PowerWatch, StartTime};
use navori_procs::{Exited, ProcId, ProcWatcher, ProcsError};
use serde_json::{Value, json};
use tokio::io::unix::AsyncFd;

use crate::action::AwakeAction;
use crate::domain::{
    self, DisplayChange, EndReason, Ended, Held, Lease, LeaseKey, LeaseSource, LeaseTable,
    Reconciler, Timing,
};
use crate::event::{self, LEASE_ENDED, LEASE_STARTED, LID_CHANGED, TRACKING_CHANGED};
use crate::infra::stored::{self, StoredKind, StoredLease};
use crate::infra::{ExecutableName, Platform, ProcessClass, Rejected};
use crate::state::AwakeState;

const MAX_SECONDS: u64 = 7 * 24 * 60 * 60;
const RETRY: Duration = Duration::from_secs(60);
pub(crate) const SYSTEM_PROBLEM: &str = "macOS refused the request. Navori keeps trying.";
const DISPLAY_PROBLEM: &str = "macOS refused to keep the display on. Navori keeps trying.";
const RELEASE_PROBLEM: &str = "Releasing a power assertion failed.";
const EXITS_PROBLEM: &str =
    "Exit watches stopped, so a command lease has no end signal. Navori retries in 60 seconds.";
const LID_READ_PROBLEM: &str = "Awake could not read the Lid closed setting, so it reads as off.";
const POWER_PROBLEM: &str = "Awake could not watch the power source, so it reads as on battery. Navori retries in 60 seconds.";
const LID_MARKER_PROBLEM: &str = "Awake could not save its lid marker, so it did not hold the lid. Navori retries in 60 seconds.";
const LID_MARKER_READ_PROBLEM: &str =
    "Awake could not read its lid marker, so it did not give the lid back.";
const LID_REFUSED_PROBLEM: &str = "macOS refused to keep the lid awake, so Lid closed is off.";
const LID_CLEAR_PROBLEM: &str =
    "Awake could not give the lid setting back. Navori retries in 60 seconds.";
const LAST_SLEEP_PROBLEM: &str = "Awake could not save when the Mac last slept.";
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
    lid_on: bool,
    lid_problem: Option<String>,
    power: Option<AsyncFd<PowerWatch>>,
    on_adapter: Option<bool>,
    lid: Option<LidGuard>,
    lid_marker: bool,
    lid_restored: bool,
    last_sleep_ms: Option<i64>,
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
            lid_on: false,
            lid_problem: None,
            power: None,
            on_adapter: None,
            lid: None,
            lid_marker: false,
            lid_restored: false,
            last_sleep_ms: None,
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
        self.load_tracking(ctx);
        self.load_lid(ctx);
        self.load_last_sleep(ctx);
        if self.lid_on {
            self.arm_power(wait.now());
        }
        self.restore_lid_once(ctx, wait.now());
        self.restore_if_needed(ctx, wait.now()).await;
        loop {
            let now = wait.now();
            let due = self.take_due_retry(now);
            if due && self.problems.exits.is_some() {
                self.rebuild_exits_now(now);
            }
            if due && self.lid_on && self.power.is_none() {
                self.rebuild_power_now(now);
            }
            if due {
                self.restore_lid_once(ctx, now);
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
                event = power_next(&mut self.power) => {
                    match event {
                        PowerEvent::Changed => self.read_power(wait.now()),
                        PowerEvent::Dead => self.note_power_dead(wait.now()),
                    }
                }
                req = ctx.recv() => {
                    let Some(req) = req else {
                        self.release_lid_on_quit(ctx);
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

    /// Runs on every entry, so a restart repairs itself. A read failure keeps the current
    /// value: on at the first entry, which leaves the Mac awake rather than asleep mid-run.
    fn load_tracking(&mut self, ctx: &ModuleCtx<Self>) {
        match stored::load_tracking(&ctx.store) {
            Ok(Some(on)) => self.table.set_tracking(on),
            Ok(None) => {}
            Err(err) => tracing::warn!(error = %err, "the tracking setting could not be read"),
        }
    }

    fn load_last_sleep(&mut self, ctx: &ModuleCtx<Self>) {
        match stored::load_last_sleep(&ctx.store) {
            Ok(value) => self.last_sleep_ms = value,
            Err(err) => tracing::warn!(error = %err, "the last sleep could not be read"),
        }
    }

    fn load_lid(&mut self, ctx: &ModuleCtx<Self>) {
        match stored::load_lid(&ctx.store) {
            Ok(value) => {
                self.lid_on = value.unwrap_or(false);
                self.lid_problem = None;
            }
            Err(err) => {
                tracing::warn!(error = %err, "the Lid closed setting could not be read");
                self.lid_on = false;
                self.lid_problem = Some(LID_READ_PROBLEM.to_owned());
            }
        }
    }

    /// Registers the watch before the read, so a change in between is not lost.
    fn arm_power(&mut self, now: SystemTime) {
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
            self.lid_problem = None;
        }
    }

    fn disarm_power(&mut self) {
        self.power = None;
        self.on_adapter = None;
    }

    fn read_power(&mut self, now: SystemTime) {
        match self.platform.power_kind() {
            Ok(kind) => self.on_adapter = Some(kind == PowerKind::Adapter),
            Err(err) => {
                tracing::warn!(error = %err, "the power source could not be read");
                self.note_power_dead(now);
            }
        }
    }

    fn note_power_dead(&mut self, now: SystemTime) {
        self.power = None;
        if !self.lid_on {
            return;
        }
        tracing::warn!("the power-source watch stopped; it reads as battery until the retry");
        self.on_adapter = Some(false);
        self.lid_problem = Some(POWER_PROBLEM.to_owned());
        self.schedule_retry(now);
    }

    fn rebuild_power_now(&mut self, now: SystemTime) {
        self.arm_power(now);
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
            AwakeAction::HoldFor { seconds } => match self.hold_for(now, *seconds) {
                Ok(replaced) => {
                    if let Some(ended) = replaced {
                        self.emit_ended(ctx, std::slice::from_ref(&ended));
                    }
                    Ok(Some(Started::Duration(*seconds)))
                }
                Err(err) => Err(err),
            },
            AwakeAction::HoldUntilOff => {
                if let Some(ended) = self.hold_until_off(now) {
                    self.emit_ended(ctx, std::slice::from_ref(&ended));
                }
                Ok(Some(Started::UntilOff))
            }
            AwakeAction::Display { on } => Ok(self.display(ctx, *on, now)),
            AwakeAction::HoldCommand { pid } => self.hold_command(ctx, now, *pid),
            AwakeAction::HoldSession {
                session_id,
                pid,
                ppid,
                title,
                cwd,
            } => {
                let started = self
                    .hold_session(ctx, now, session_id.clone(), *pid, *ppid, cwd.as_deref())
                    .await;
                self.table.set_session_title(session_id, title.as_deref());
                started
            }
            AwakeAction::StopSession {
                session_id,
                background,
                title,
            } => {
                let stopped = self.stop_session(ctx, session_id, truncate_us(now), *background);
                self.table.set_session_title(session_id, title.as_deref());
                stopped
            }
            AwakeAction::EndSession { session_id } => self.end_session(ctx, session_id, now),
            AwakeAction::ReleaseAll => self.release_all(ctx),
            AwakeAction::KeepFor { seconds } => self.keep_for(ctx, now, *seconds),
            AwakeAction::KeepIndefinitely => Ok(self.keep_indefinitely(ctx, now)),
            AwakeAction::KeepOff => {
                let ended = self.table.keep_off();
                self.emit_ended(ctx, &ended);
                Ok(None)
            }
            AwakeAction::Tracking { on } => self.set_tracking(ctx, *on),
            AwakeAction::Lid { on } => self.set_lid(ctx, *on, now),
            AwakeAction::Slept { at_ms } => {
                self.record_sleep(ctx, *at_ms);
                Ok(None)
            }
            AwakeAction::Woke => {
                self.on_woke(now);
                Ok(None)
            }
            AwakeAction::SubagentStart {
                session_id,
                agent_id,
                pid,
                ppid,
            } => {
                self.subagent_start(ctx, now, session_id, agent_id, *pid, *ppid)
                    .await
            }
            AwakeAction::SubagentStop {
                session_id,
                agent_id,
                background,
            } => self.subagent_stop(now, session_id, agent_id, *background),
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

    fn keep_for(
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

    fn keep_indefinitely(&mut self, ctx: &ModuleCtx<Self>, now: SystemTime) -> Option<Started> {
        if let Some(ended) = self.table.end_manual() {
            self.emit_ended(ctx, std::slice::from_ref(&ended));
        }
        self.display(ctx, true, now)
    }

    /// Stores first, so a failure changes nothing, as Release all does.
    fn set_tracking(
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

    /// Stores first, so a failure changes nothing, as Tracking agents does.
    fn set_lid(
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
        self.lid_problem = None;
        if on {
            self.arm_power(now);
        } else {
            self.disarm_power();
        }
        ctx.emit(
            LID_CHANGED,
            Severity::Info,
            event::lid_changed(on).to_owned(),
            json!({ "on": on }),
        );
        Ok(None)
    }

    fn record_sleep(&mut self, ctx: &ModuleCtx<Self>, at_ms: i64) {
        let reason = self.platform.sleep_reason();
        if let Err(err) = stored::save_last_sleep(&ctx.store, at_ms) {
            tracing::warn!(error = %err, "the last sleep could not be stored");
            self.lid_problem = Some(LAST_SLEEP_PROBLEM.to_owned());
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
    fn on_woke(&mut self, now: SystemTime) {
        if self.power.is_some() {
            self.read_power(now);
        }
    }

    fn hold_until_off(&mut self, now: SystemTime) -> Option<Ended> {
        let started = truncate_us(now);
        self.table.hold_until_off(started)
    }

    fn display(&mut self, ctx: &ModuleCtx<Self>, on: bool, now: SystemTime) -> Option<Started> {
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

    fn hold_for(&mut self, now: SystemTime, seconds: u64) -> Result<Option<Ended>, ActionError> {
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

    async fn hold_session(
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

    async fn subagent_start(
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

    fn subagent_stop(
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
            self.emit_ended(ctx, &[ended]);
        }
        Ok(None)
    }

    fn stop_session(
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
        self.reconcile_lid(ctx, now);
        self.flush_dirty(ctx, now);
        self.sync_retry(now, &held);
        self.publish(ctx, &held)
    }

    fn lid_wanted(&self) -> bool {
        self.lid_restored
            && self.lid_on
            && self.table.wants().system
            && self.on_adapter == Some(true)
    }

    fn pending_lid_clear(&self) -> bool {
        self.lid_restored && self.lid_marker && self.lid.is_none()
    }

    fn reconcile_lid(&mut self, ctx: &ModuleCtx<Self>, now: SystemTime) {
        let want = self.lid_wanted();
        if let Some(guard) = self.lid.take() {
            if want {
                self.lid = Some(guard);
            } else {
                match guard.release() {
                    Ok(()) => self.finish_lid_clear(&ctx.store),
                    Err(err) => self.note_lid_clear_failed(&err, now),
                }
            }
            return;
        }
        if self.pending_lid_clear() && self.retry_at.is_none() {
            match self.platform.give_back_lid() {
                Ok(()) => self.finish_lid_clear(&ctx.store),
                Err(err) => self.note_lid_clear_failed(&err, now),
            }
        }
        if want && self.lid.is_none() && !self.lid_marker {
            self.hold_lid(ctx);
        }
    }

    /// Runs until it succeeds or the marker proves unreadable, so a restart never repeats it.
    fn restore_lid_once(&mut self, ctx: &ModuleCtx<Self>, now: SystemTime) {
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
                    self.lid_problem = Some(LID_MARKER_READ_PROBLEM.to_owned());
                    self.lid_restored = true;
                    return;
                }
            }
        }
        match self.platform.give_back_lid() {
            Ok(()) => {
                self.lid_restored = true;
                self.finish_lid_clear(&ctx.store);
            }
            Err(err) => self.note_lid_clear_failed(&err, now),
        }
    }

    fn release_lid_on_quit(&mut self, ctx: &ModuleCtx<Self>) {
        if let Some(guard) = self.lid.take() {
            match guard.release() {
                Ok(()) => self.finish_lid_clear(&ctx.store),
                Err(err) => tracing::warn!(error = %err, "the lid could not be given back on quit"),
            }
        }
    }

    fn hold_lid(&mut self, ctx: &ModuleCtx<Self>) {
        // The marker goes to disk before the lid changes, so a crash cannot leave the lid set
        // with nothing recording it.
        if let Err(err) = stored::save_lid_held(&ctx.store) {
            tracing::warn!(error = %err, "the lid marker could not be stored");
            self.lid_problem = Some(LID_MARKER_PROBLEM.to_owned());
            return;
        }
        self.lid_marker = true;
        self.clear_lid_hold_problem();
        match self.platform.hold_lid() {
            Ok(guard) => {
                self.lid = Some(guard);
                self.clear_lid_hold_problem();
            }
            Err(err) => {
                tracing::warn!(error = %err, "the lid could not be held");
                self.lid_on = false;
                self.disarm_power();
                if let Err(err) = stored::save_lid(&ctx.store, false) {
                    tracing::warn!(error = %err, "the Lid closed setting could not be stored");
                }
                ctx.emit(
                    LID_CHANGED,
                    Severity::Info,
                    event::lid_changed(false).to_owned(),
                    json!({ "on": false }),
                );
                self.finish_lid_clear(&ctx.store);
                self.lid_problem = Some(LID_REFUSED_PROBLEM.to_owned());
            }
        }
    }

    /// The lid is clear, so the marker goes; a failed delete leaves the mirror set and retries.
    fn finish_lid_clear(&mut self, store: &navori_store::Store) {
        match stored::delete_lid_held(store) {
            Ok(()) => {
                self.lid_marker = false;
                self.clear_lid_hold_problem();
            }
            Err(err) => tracing::warn!(error = %err, "the lid marker could not be deleted"),
        }
    }

    fn note_lid_clear_failed(&mut self, err: &navori_macos::LidError, now: SystemTime) {
        tracing::warn!(error = %err, "the lid setting could not be given back");
        self.lid_problem = Some(LID_CLEAR_PROBLEM.to_owned());
        self.schedule_retry(now);
    }

    fn clear_lid_hold_problem(&mut self) {
        if matches!(
            self.lid_problem.as_deref(),
            Some(LID_MARKER_PROBLEM | LID_REFUSED_PROBLEM | LID_CLEAR_PROBLEM)
        ) {
            self.lid_problem = None;
        }
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
            || (self.lid_on && self.power.is_none())
            || !self.lid_restored
            || self.pending_lid_clear()
            || (self.lid_wanted() && self.lid.is_none())
            || !self.restored
    }

    fn publish(&mut self, ctx: &ModuleCtx<Self>, held: &Held) -> AwakeState {
        let mut state = self.table.snapshot(held.system, held.display);
        state.problem = self.problems.first();
        state.lid = self.lid_on;
        state.on_adapter = self.on_adapter;
        state.lid_held = self.lid.is_some();
        state.lid_marker = self.lid_marker;
        state.last_sleep_ms = self.last_sleep_ms;
        state.lid_problem.clone_from(&self.lid_problem);
        if state != self.published {
            self.published = state.clone();
            let next = state.clone();
            ctx.update_state(|slot| *slot = next);
        }
        state
    }

    fn emit_started(&self, ctx: &ModuleCtx<Self>, started: Started) {
        if !self.table.tracking() && matches!(started, Started::Session { .. }) {
            return;
        }
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
        // A session followed while tracking is off holds nothing, so its end is no news.
        let visible = ended.iter().filter(|end| {
            self.table.tracking() || !matches!(end.lease.source, LeaseSource::Session { .. })
        });
        for end in visible {
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
                EndReason::ReleaseAll => {}
                EndReason::EndSession | EndReason::Replaced => ctx.emit(
                    LEASE_ENDED,
                    Severity::Info,
                    event::ended_stopped(&source_text(&end.lease)),
                    payload(&end.lease.key.text(), kind_of(&end.lease)),
                ),
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
                    self.table.stop_session(&session_id, mark, false);
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

enum PowerEvent {
    Changed,
    Dead,
}

async fn power_next(power: &mut Option<AsyncFd<PowerWatch>>) -> PowerEvent {
    let Some(power) = power.as_ref() else {
        return std::future::pending().await;
    };
    loop {
        let Ok(mut guard) = power.readable().await else {
            return PowerEvent::Dead;
        };
        match guard.try_io(|watch| watch.get_ref().read_tokens()) {
            Ok(Ok(0) | Err(_)) => return PowerEvent::Dead,
            Ok(Ok(_)) => return PowerEvent::Changed,
            Err(_would_block) => {}
        }
    }
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

fn check_subagent_ids(session_id: &str, agent_id: &str) -> Result<(), ActionError> {
    if valid_session_id(session_id) && valid_session_id(agent_id) {
        return Ok(());
    }
    Err(ActionError::Invalid(
        "session_id and agent_id must be 1 to 128 bytes of letters, digits, '.', '_', ':', or '-'"
            .to_owned(),
    ))
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

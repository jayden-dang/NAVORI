use std::collections::BTreeSet;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use navori_kernel::{ActionError, ModuleCtx, Request, Severity, WaitHandle, Wake};
use navori_macos::{LidGuard, PowerWatch};
use navori_procs::{Exited, ProcId, ProcWatcher, ProcsError};
use serde_json::{Value, json};
use tokio::io::unix::AsyncFd;

use crate::action::AwakeAction;
use crate::domain::{
    self, EndReason, Ended, Held, Lease, LeaseKey, LeaseSource, LeaseTable, Reconciler, Timing,
};
use crate::event::{self, LEASE_ENDED, LEASE_STARTED};
use crate::infra::stored::{self, StoredKind, StoredLease};
use crate::infra::{Platform, Rejected};
use crate::state::AwakeState;

mod leases;
mod lid;
mod persist;
mod problems;

pub(super) use problems::*;
pub struct Awake {
    pub(in crate::actor) platform: Platform,
    pub(in crate::actor) table: LeaseTable,
    pub(in crate::actor) reconciler: Reconciler,
    pub(in crate::actor) published: AwakeState,
    pub(in crate::actor) lid_on: bool,
    pub(in crate::actor) lid_problems: LidProblems,
    pub(in crate::actor) power: Option<AsyncFd<PowerWatch>>,
    pub(in crate::actor) on_adapter: Option<bool>,
    pub(in crate::actor) lid: Option<LidGuard>,
    pub(in crate::actor) lid_marker: bool,
    pub(in crate::actor) lid_restored: bool,
    pub(in crate::actor) last_sleep_ms: Option<i64>,
    pub(in crate::actor) watcher: Option<ProcWatcher>,
    pub(in crate::actor) problems: Problems,
    pub(in crate::actor) retry_at: Option<SystemTime>,
    pub(in crate::actor) restored: bool,
    pub(in crate::actor) dirty: BTreeSet<String>,
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
            lid_problems: LidProblems::default(),
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
                self.arm_power(now);
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
        state.lid_problem = self.lid_problems.first();
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
}

pub(super) enum RowFate {
    Keep(ProcId),
    Drop,
    /// This row stays stored. Other rows in the pass are still finished.
    Retry,
    Halt,
}

pub(super) enum Started {
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

pub(super) fn platform_failed(err: impl std::fmt::Display) -> ActionError {
    ActionError::Failed(format!(
        "the process list could not be read ({err}); retry the hold"
    ))
}

pub(super) fn watch_failed(err: ProcsError) -> ActionError {
    match err {
        ProcsError::Watch { errno } => ActionError::Failed(format!(
            "kqueue failed with errno {errno}; no command lease was held"
        )),
        ProcsError::Platform { source } => ActionError::Failed(format!(
            "exit watches could not be started ({source}); no command lease was held"
        )),
    }
}

pub(super) fn source_text(lease: &Lease) -> String {
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
        LeaseSource::Session { harness, .. } => harness.display().to_owned(),
    }
}

pub(super) fn kind_of(lease: &Lease) -> &'static str {
    match lease.source {
        LeaseSource::Duration { .. } => crate::state::LeaseKind::Duration.as_str(),
        LeaseSource::UntilOff => crate::state::LeaseKind::UntilOff.as_str(),
        LeaseSource::Display => crate::state::LeaseKind::Display.as_str(),
        LeaseSource::Command { .. } => crate::state::LeaseKind::Command.as_str(),
        LeaseSource::Session { .. } => crate::state::LeaseKind::Session.as_str(),
    }
}

pub(super) fn process_of(lease: &Lease) -> Option<ProcId> {
    match lease.source {
        LeaseSource::Command { process } | LeaseSource::Session { process, .. } => Some(process),
        LeaseSource::Duration { .. } | LeaseSource::UntilOff | LeaseSource::Display => None,
    }
}

pub(super) fn check_subagent_ids(session_id: &str, agent_id: &str) -> Result<(), ActionError> {
    if valid_session_id(session_id) && valid_session_id(agent_id) {
        return Ok(());
    }
    Err(ActionError::Invalid(
        "session_id and agent_id must be 1 to 128 bytes of letters, digits, '.', '_', ':', or '-'"
            .to_owned(),
    ))
}

pub(super) fn valid_session_id(session_id: &str) -> bool {
    let len = session_id.len();
    (1..=128).contains(&len)
        && session_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b':' | b'-'))
}

pub(super) fn rejected_hold(rejected: Rejected, pid: i32, ppid: i32) -> ActionError {
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

pub(super) fn payload(key: &str, kind: &str) -> Value {
    json!({ "key": key, "kind": kind })
}

pub(super) fn stored_key(lease: &Lease) -> Option<String> {
    match lease.source {
        LeaseSource::Command { .. } | LeaseSource::Session { .. } => Some(lease.key.text()),
        LeaseSource::Duration { .. } | LeaseSource::UntilOff | LeaseSource::Display => None,
    }
}

pub(super) fn stored_session(saved: &domain::SessionSaved) -> StoredLease {
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

pub(super) fn stored_command(process: ProcId) -> StoredLease {
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

pub(super) fn at<T: Clone>(items: Option<&[Option<T>]>, index: usize) -> Option<T> {
    items.and_then(|items| items.get(index).cloned()).flatten()
}

pub(super) fn truncate_us(time: SystemTime) -> SystemTime {
    let Ok(duration) = time.duration_since(UNIX_EPOCH) else {
        return UNIX_EPOCH;
    };
    match u64::try_from(duration.as_micros()) {
        Ok(micros) => UNIX_EPOCH + Duration::from_micros(micros),
        Err(_) => time,
    }
}

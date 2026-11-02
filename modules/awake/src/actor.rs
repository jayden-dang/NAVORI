use std::time::{Duration, SystemTime, UNIX_EPOCH};

use navori_kernel::{ActionError, ModuleCtx, Request, Severity, WaitHandle, Wake};
use navori_procs::{Exited, ProcId, ProcWatcher, ProcsError};
use serde_json::{Value, json};

use crate::action::AwakeAction;
use crate::domain::{
    EndReason, Ended, Held, Lease, LeaseKey, LeaseSource, LeaseTable, Reconciler, Timing,
};
use crate::event::{self, LEASE_ENDED, LEASE_STARTED};
use crate::infra::{ExecutableName, Platform, ProcessClass};
use crate::state::AwakeState;

const MAX_SECONDS: u64 = 7 * 24 * 60 * 60;
const EXIT_RETRY: Duration = Duration::from_secs(60);
const EXITS_PROBLEM: &str =
    "Exit watches stopped, so a command lease has no end signal. Navori retries in 60 seconds.";

pub struct Awake {
    platform: Platform,
    table: LeaseTable,
    reconciler: Reconciler,
    published: AwakeState,
    watcher: Option<ProcWatcher>,
    exits_problem: Option<String>,
    retry_at: Option<SystemTime>,
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
            exits_problem: None,
            retry_at: None,
        }
    }

    pub(crate) async fn run_loop(&mut self, ctx: &mut ModuleCtx<Self>) -> anyhow::Result<()> {
        let mut wait = ctx.wait_handle();
        loop {
            let now = wait.now();
            self.rebuild_exits_if_due(now);
            self.turn(ctx, now);
            let deadline = self.deadline();
            tokio::select! {
                biased;
                exit = watcher_next(&mut self.watcher) => {
                    match exit {
                        Some(exited) => self.on_process_exit(ctx, exited.id),
                        None => self.note_exits_dead(wait.now()),
                    }
                }
                req = ctx.recv() => {
                    let Some(req) = req else {
                        return Ok(());
                    };
                    self.on_request(ctx, &wait, req);
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

    fn on_request(&mut self, ctx: &ModuleCtx<Self>, wait: &WaitHandle, req: Request<AwakeAction>) {
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
            AwakeAction::HoldCommand { pid } => self.hold_command(now, *pid),
            AwakeAction::ReleaseAll => {
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
        };
        match started {
            Ok(started) => {
                if let Some(started) = started {
                    self.emit_started(ctx, started);
                }
                let state = self.turn(ctx, now);
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

    fn hold_command(&mut self, now: SystemTime, pid: i32) -> Result<Option<Started>, ActionError> {
        if pid <= 0 {
            return Err(ActionError::Invalid(format!(
                "pid must be positive, not {pid}"
            )));
        }
        let id = match self.platform.identify(pid).map_err(platform_failed)? {
            Some(id) => id,
            None => {
                return match self.platform.classify(pid).map_err(platform_failed)? {
                    ProcessClass::OtherUser => Err(ActionError::Invalid(format!(
                        "pid {pid} belongs to another user"
                    ))),
                    ProcessClass::Finished | ProcessClass::Live(_) => Ok(None),
                };
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
        self.arm(id)?;
        self.table.hold_command(id, truncate_us(now), label.clone());
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

    fn arm(&mut self, id: ProcId) -> Result<(), ActionError> {
        if self.table.leases_on(id) > 0 {
            return Ok(());
        }
        if self.watcher.is_none() {
            let watcher = self.platform.build_watcher().map_err(platform_failed)?;
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

    fn on_process_exit(&mut self, ctx: &ModuleCtx<Self>, id: ProcId) {
        let ended = self.table.on_exit(id);
        self.unwatch_if_idle(id);
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
            self.exits_problem = None;
            self.retry_at = None;
        }
    }

    fn note_exits_dead(&mut self, now: SystemTime) {
        self.watcher = None;
        if self.table.process_ids().is_empty() {
            self.exits_problem = None;
            self.retry_at = None;
            return;
        }
        tracing::warn!("exit watches stopped; a command lease has no end signal until the retry");
        self.exits_problem = Some(EXITS_PROBLEM.to_owned());
        self.retry_at = now.checked_add(EXIT_RETRY);
    }

    fn rebuild_exits_if_due(&mut self, now: SystemTime) {
        let Some(due) = self.retry_at else {
            return;
        };
        if now < due {
            return;
        }
        self.retry_at = None;
        if self.table.process_ids().is_empty() {
            self.exits_problem = None;
            return;
        }
        match self.rebuild_watcher() {
            Ok(()) => self.exits_problem = None,
            Err(err) => {
                tracing::warn!(error = %err, "exit watches could not be rebuilt");
                self.exits_problem = Some(EXITS_PROBLEM.to_owned());
                self.retry_at = now.checked_add(EXIT_RETRY);
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

    fn turn(&mut self, ctx: &ModuleCtx<Self>, now: SystemTime) -> AwakeState {
        let settled = self.table.settle(now);
        self.emit_ended(ctx, &settled.ended);
        let held = self.reconciler.apply(self.table.wants());
        if let Some(code) = held.release_failed {
            tracing::warn!(code, "releasing a power assertion failed");
        }
        self.publish(ctx, &held)
    }

    fn publish(&mut self, ctx: &ModuleCtx<Self>, held: &Held) -> AwakeState {
        let mut state = self.table.snapshot(held.system, held.display);
        state.problem = self.exits_problem.clone();
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
            Started::Command { key, name } => (key, name, "command"),
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
                EndReason::ReleaseAll if !released => {
                    released = true;
                    ctx.emit(
                        LEASE_ENDED,
                        Severity::Info,
                        event::ended_release_all(),
                        payload(&end.lease.key.text(), kind_of(&end.lease)),
                    );
                }
                EndReason::ReleaseAll | EndReason::Replaced => {}
            }
        }
    }
}

enum Started {
    Duration(u64),
    UntilOff,
    Command { key: String, name: String },
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
        LeaseSource::Command { .. } => lease.label.clone().unwrap_or_else(|| "command".to_owned()),
    }
}

fn kind_of(lease: &Lease) -> &'static str {
    match lease.source {
        LeaseSource::Duration { .. } => "duration",
        LeaseSource::UntilOff => "until_off",
        LeaseSource::Command { .. } => "command",
    }
}

fn process_of(lease: &Lease) -> Option<ProcId> {
    match lease.source {
        LeaseSource::Command { process } => Some(process),
        LeaseSource::Duration { .. } | LeaseSource::UntilOff => None,
    }
}

fn payload(key: &str, kind: &str) -> Value {
    json!({ "key": key, "kind": kind })
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

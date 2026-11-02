use std::time::{Duration, SystemTime, UNIX_EPOCH};

use navori_kernel::{ActionError, ModuleCtx, Request, Severity, WaitHandle, Wake};
use serde_json::{Value, json};

use crate::action::AwakeAction;
use crate::domain::{EndReason, Ended, Held, Lease, LeaseSource, LeaseTable, Reconciler, Timing};
use crate::event::{self, LEASE_ENDED, LEASE_STARTED};
use crate::infra::Platform;
use crate::state::AwakeState;

const MAX_SECONDS: u64 = 7 * 24 * 60 * 60;

pub struct Awake {
    platform: Platform,
    table: LeaseTable,
    reconciler: Reconciler,
    published: AwakeState,
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
        }
    }

    pub(crate) async fn run_loop(&mut self, ctx: &mut ModuleCtx<Self>) -> anyhow::Result<()> {
        let mut wait = ctx.wait_handle();
        loop {
            let now = wait.now();
            self.turn(ctx, now);
            let deadline = self.deadline();
            tokio::select! {
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
        self.table.next_deadline()
    }

    fn on_request(&mut self, ctx: &ModuleCtx<Self>, wait: &WaitHandle, req: Request<AwakeAction>) {
        let now = wait.now();
        let started = match &req.action {
            AwakeAction::Status => Ok(None),
            AwakeAction::HoldFor { seconds } => {
                self.hold_for(now, *seconds).map(|()| Some(*seconds))
            }
            AwakeAction::ReleaseAll => {
                let ended = self.table.release_all();
                self.emit_ended(ctx, &ended);
                Ok(None)
            }
        };
        match started {
            Ok(started) => {
                if let Some(seconds) = started {
                    self.emit_started(ctx, seconds);
                }
                let state = self.turn(ctx, now);
                req.respond(Ok(state));
            }
            Err(err) => req.respond::<AwakeState>(Err(err)),
        }
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
        let state = self.table.snapshot(held.system, held.display);
        if state != self.published {
            self.published = state.clone();
            let next = state.clone();
            ctx.update_state(|slot| *slot = next);
        }
        state
    }

    fn emit_started(&self, ctx: &ModuleCtx<Self>, seconds: u64) {
        ctx.emit(
            LEASE_STARTED,
            Severity::Info,
            event::started(&event::duration_source(seconds)),
            payload("manual"),
        );
    }

    fn emit_ended(&self, ctx: &ModuleCtx<Self>, ended: &[Ended]) {
        let mut released = false;
        for end in ended {
            match end.reason {
                EndReason::Deadline => ctx.emit(
                    LEASE_ENDED,
                    Severity::Info,
                    event::ended_deadline(&duration_source_for(&end.lease)),
                    payload(end.lease.key.as_str()),
                ),
                EndReason::ReleaseAll if !released => {
                    released = true;
                    ctx.emit(
                        LEASE_ENDED,
                        Severity::Info,
                        event::ended_release_all(),
                        payload(end.lease.key.as_str()),
                    );
                }
                EndReason::ReleaseAll | EndReason::Replaced => {}
            }
        }
    }
}

fn duration_source_for(lease: &Lease) -> String {
    let LeaseSource::Duration { started } = lease.source;
    let seconds = lease
        .deadline
        .and_then(|deadline| deadline.duration_since(started).ok())
        .map_or(0, |span| span.as_secs());
    event::duration_source(seconds)
}

fn payload(key: &str) -> Value {
    json!({ "key": key, "kind": "duration" })
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

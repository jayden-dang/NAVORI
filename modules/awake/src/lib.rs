//! Keeps the Mac awake for a lease.

mod action;
mod actor;
mod domain;
mod event;
mod infra;
mod state;

pub use action::AwakeAction;
pub use actor::Awake;
pub use domain::{
    EndReason, Ended, Held, Lease, LeaseKey, LeaseSource, LeaseTable, Reconciler, Settled, Timing,
    Wants,
};
pub use event::{LEASE_ENDED, LEASE_STARTED};
pub use infra::{ExitsFactory, Platform, ProcessFactory};
pub use state::{AwakeState, LeaseState};

use navori_kernel::{Module, ModuleCtx, NoConfig};

impl Module for Awake {
    const ID: &'static str = "awake";
    type Action = AwakeAction;
    type Config = NoConfig;
    type State = AwakeState;

    fn initial_state(&self) -> AwakeState {
        AwakeState::idle()
    }

    async fn run(&mut self, ctx: &mut ModuleCtx<Self>) -> anyhow::Result<()> {
        self.run_loop(ctx).await
    }
}

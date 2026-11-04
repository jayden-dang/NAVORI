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
pub use event::{LEASE_ENDED, LEASE_STARTED, TRACKING_CHANGED};
pub use infra::{ExitsFactory, Platform, ProcessFactory};
pub use state::{AwakeState, LeaseState};

use navori_kernel::{Module, ModuleCtx, NoConfig, TrayContribution, TrayGlyph};

use crate::actor::SYSTEM_PROBLEM;

impl Module for Awake {
    const ID: &'static str = "awake";
    type Action = AwakeAction;
    type Config = NoConfig;
    type State = AwakeState;

    fn initial_state(&self) -> AwakeState {
        AwakeState::idle()
    }

    fn tray(state: &AwakeState) -> TrayContribution {
        // Reads only `leases` and `system_held`: the problem glyph is for a Lease macOS
        // will not hold, not for the other things that can go wrong in this Module.
        let glyph = if state.leases.is_empty() {
            None
        } else if state.system_held {
            Some((TrayGlyph::Lease, "Keeping this Mac awake".to_owned()))
        } else {
            Some((
                TrayGlyph::Problem,
                format!("Can't hold this Mac awake. {SYSTEM_PROBLEM}"),
            ))
        };
        TrayContribution {
            glyph,
            attention: None,
        }
    }

    async fn run(&mut self, ctx: &mut ModuleCtx<Self>) -> anyhow::Result<()> {
        self.run_loop(ctx).await
    }
}

//! Lists the TCP ports the user's own processes listen on.

mod action;
mod actor;
mod docker;
mod image;
mod infra;
mod kind;
mod place;
mod scan;
mod state;

use std::path::PathBuf;
use std::sync::Arc;

use navori_kernel::{Module, ModuleCtx, NoConfig, TrayContribution, TrayGlyph};
use navori_macos::{MacProcesses, MacSignals, PlatformError, ProcessSource, Signals};

pub use action::{KillReply, Outcome, PortsAction, Selector, Target, TargetResult};
pub use docker::attribute;
pub use image::database_image;
pub use infra::docker::{Container, Containers, DockerError, DockerSocket};
pub use kind::kind;
pub use place::{place, place_listener};
pub use scan::{BackendPort, HarnessMemo, Scan, Scanner};
pub use state::*;

/// Opens the process source, once, when the first scan needs it.
pub type OpenProcesses = Box<dyn FnMut() -> Result<Arc<dyn ProcessSource>, PlatformError> + Send>;

/// What the Module reads from outside.
pub struct PortsEnv {
    pub home: PathBuf,
    pub processes: OpenProcesses,
    pub containers: Arc<dyn Containers>,
    pub signals: Arc<dyn Signals>,
    /// The Navori helper binary, which a stop must never signal.
    pub helper: Option<PathBuf>,
}

impl std::fmt::Debug for PortsEnv {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PortsEnv")
            .field("home", &self.home)
            .finish_non_exhaustive()
    }
}

impl PortsEnv {
    /// The real machine. `home` is canonicalized once.
    pub fn mac(home: PathBuf) -> Self {
        let home = std::fs::canonicalize(&home).unwrap_or(home);
        let socket = home.join(".docker/run/docker.sock");
        Self {
            home,
            containers: Arc::new(DockerSocket::new(socket)),
            signals: Arc::new(MacSignals::new()),
            helper: None,
            processes: Box::new(|| {
                MacProcesses::new().map(|source| Arc::new(source) as Arc<dyn ProcessSource>)
            }),
        }
    }

    /// Names the helper binary a stop must leave alone.
    #[must_use]
    pub fn with_helper(mut self, helper: PathBuf) -> Self {
        self.helper = Some(helper);
        self
    }
}

/// The Ports Module.
pub struct Ports {
    env: PortsEnv,
    source: Option<Arc<dyn ProcessSource>>,
    memo: HarnessMemo,
}

impl std::fmt::Debug for Ports {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Ports")
            .field("env", &self.env)
            .finish_non_exhaustive()
    }
}

impl Module for Ports {
    const ID: &'static str = "ports";
    type Action = PortsAction;
    type Config = NoConfig;
    type State = PortsState;

    fn initial_state(&self) -> PortsState {
        PortsState::default()
    }

    fn tray(state: &PortsState) -> TrayContribution {
        TrayContribution {
            glyph: state
                .problem
                .as_ref()
                .map(|problem| (TrayGlyph::Problem, format!("Can't scan ports. {problem}"))),
            attention: None,
        }
    }

    async fn run(&mut self, ctx: &mut ModuleCtx<Self>) -> anyhow::Result<()> {
        self.run_loop(ctx).await
    }
}

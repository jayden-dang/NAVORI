//! Navori kernel: the module runtime every feature plugs into.
//!
//! - [`Module`]: an actor with a typed action enum and a state snapshot.
//! - [`Kernel`]: registry + router. Callers (UI, CLI, AI) send a type-erased [`Envelope`];
//!   the kernel deserializes it into the module's action type and awaits the answer.
//! - [`EventBus`]: every event is redacted, journaled and broadcast.
//! - Supervision: a crashed module is restarted with backoff and keeps its inbox.

mod bus;
mod module;
mod runtime;
mod scheduler;
mod system;

pub use bus::EventBus;
pub use module::{ActionDoc, ActionError, Module, ModuleAction, ModuleCtx, Request};
pub use navori_protocol::{
    ActionSpec, CoreStatus, Envelope, Event, ModuleInfo, ModuleState, Risk, Severity,
};
pub use runtime::{DispatchError, Kernel, KernelBuilder, RestartPolicy};
pub use scheduler::{Scheduler, SystemClock, WaitHandle, Wake, WallClock};
pub use system::{System, SystemAction, SystemState};

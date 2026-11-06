//! In-memory stand-ins for process, exit and power reads.

mod exits;
mod lid;
mod power;
mod power_source;
mod processes;
mod signals;

pub use exits::*;
pub use lid::*;
pub use power::*;
pub use power_source::*;
pub use processes::*;
pub use signals::*;

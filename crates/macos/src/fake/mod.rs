//! In-memory stand-ins for process, exit and power reads.

mod exits;
mod lid;
mod power;
mod processes;
mod signals;

pub use exits::*;
pub use lid::*;
pub use power::*;
pub use processes::*;
pub use signals::*;

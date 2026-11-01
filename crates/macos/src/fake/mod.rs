//! In-memory stand-ins for process, exit and power reads.

mod exits;
mod power;
mod processes;

// Empty globs become used when the modules gain items; this file is not edited then.
#[allow(unused_imports)]
pub use exits::*;
#[allow(unused_imports)]
pub use power::*;
#[allow(unused_imports)]
pub use processes::*;

//! macOS process, exit and power reads.

mod error;
mod exits;
mod power;
mod procargs;
mod process;

// Empty globs become used when the modules gain items; this file is not edited then.
#[allow(unused_imports)]
pub use error::*;
#[allow(unused_imports)]
pub use exits::*;
#[allow(unused_imports)]
pub use power::*;
#[allow(unused_imports)]
pub use procargs::*;
#[allow(unused_imports)]
pub use process::*;

#[cfg(feature = "fake")]
pub mod fake;

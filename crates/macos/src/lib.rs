//! macOS process, exit and power reads.

// Empty globs become used when the modules gain items; this file is not edited then.
#![allow(unused_imports)]

mod error;
mod exits;
mod power;
mod procargs;
mod process;

pub use error::*;
pub use exits::*;
pub use power::*;
pub use procargs::*;
pub use process::*;

#[cfg(feature = "fake")]
pub mod fake;

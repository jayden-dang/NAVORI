//! macOS process, exit and power reads.

mod error;
mod exits;
mod lid;
mod power;
mod procargs;
mod process;
mod signals;
mod sockets;

pub use error::*;
pub use exits::*;
pub use lid::*;
pub use power::*;
pub use procargs::*;
pub use process::*;
pub use signals::*;
pub use sockets::*;

#[cfg(feature = "fake")]
pub mod fake;

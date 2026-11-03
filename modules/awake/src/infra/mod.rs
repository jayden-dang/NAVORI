mod platform;
pub(crate) mod stored;

pub(crate) use platform::{ExecutableName, ProcessClass, Rejected};
pub use platform::{ExitsFactory, Platform, ProcessFactory};

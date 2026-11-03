mod hookfile;
mod identity;

pub(crate) use hookfile::{Changed, converge, strip};
pub use hookfile::{Classification, classify};

//! Navori's own secrets (tokens and API keys), kept only in the macOS Keychain (ARCH-8).
//!
//! A value never appears in `Debug`, `Display` or an error from this crate, and the crate
//! contains no `unsafe` code (ARCH-9).

mod key;
pub use key::*;

mod secret;
pub use secret::*;

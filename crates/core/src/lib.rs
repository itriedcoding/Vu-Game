//! Volt performance engine.
//!
//! Everything that inspects Windows, edits reversible settings, cleans temporary
//! files or measures performance lives here. The GUI crate only renders what this
//! engine reports, which keeps the UI free of system calls and keeps the engine
//! unit-testable.
//!
//! Design rules this crate follows:
//! * Volt never touches Roblox game files, never injects code and never talks to
//!   Roblox's anti-cheat.
//! * Every system change is backed up before it is applied and can be undone by
//!   [`optimize::restore`].
//! * Nothing is estimated: values are either measured or reported as unavailable.

pub mod cleanup;
pub mod format;
pub mod fps;
pub mod model;
pub mod monitor;
pub mod optimize;
pub mod platform;
pub mod recommend;
pub mod report;
pub mod roblox;
pub mod state;

pub const APP_NAME: &str = "Volt";
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Seconds since the Unix epoch, or `0` if the clock predates it.
pub fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

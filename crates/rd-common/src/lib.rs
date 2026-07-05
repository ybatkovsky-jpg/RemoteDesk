//! Shared types for RemoteDesk.
//!
//! Re-exports the core types from RustDesk's `hbb_common` crate
//! and extends with application-specific types.

// Re-export the real hbb_common crate
pub use hbb_common as core;

pub mod config;
pub mod error;
pub mod proto;

/// Crate version
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Re-export commonly used types
pub use error::{Error, Result};

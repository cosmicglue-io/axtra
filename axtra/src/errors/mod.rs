//! Error types, macros, and helpers for Axtra.
//!
//! This module provides:
//! - The [`AppError`] enum for unified error handling
//! - Error construction macros ([`app_error!`])
//! - TypeScript type generation for error codes
//! - Notification integration (Slack, Discord, ntfy, PostHog, custom)
//! - Automatic error location tracking
//!
//! See crate-level docs for usage examples.

mod macros;
mod response;
mod types;

#[cfg(feature = "notifier")]
pub mod notifiers;

// Re-export everything users need
pub use types::*;

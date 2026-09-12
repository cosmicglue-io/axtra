//! # Axtra
//!
//! Opinionated helpers for Axum + Astro projects.
//!
//! ## Features
//!
//! - **AppError**: Unified error type for Axum APIs.
//! - **Error Macros**: Ergonomic error construction with `app_error!`.
//! - **TypeScript Type Generation**: Rust error types exported via `ts-rs`.
//! - **Error Notifications**: Pluggable notification system with Slack, Discord, ntfy, and PostHog support.
//! - **Wrapped JSON Responses**: `WrappedJson<T>` and `ResponseKey` derive macro.
//! - **Health Check Endpoint**: Built-in Axum route for Postgres connectivity.
//! - **Static File Serving**: SPA and static file helpers for Axum.
//! - **Bouncer** (optional): Reject and ban IP's hitting invalid endpoints.
//!
//! ## Notification System
//!
//! The notification system uses a trait-based architecture allowing you to:
//! - Use built-in providers (Slack, Discord, ntfy, PostHog)
//! - Create custom providers by implementing [`notifier::ErrorNotifier`]
//! - Configure via builder pattern or environment variables
//!
//! ```rust,ignore
//! use axtra::notifier::{NotificationManager, SlackConfig};
//! use axtra::errors::notifiers::init_notification_manager;
//!
//! // Auto-configure from environment variables
//! init_notification_manager(NotificationManager::from_env());
//!
//! // Or use builder pattern
//! let manager = NotificationManager::builder()
//!     .with_slack(SlackConfig::new("https://hooks.slack.com/..."))
//!     .build();
//! init_notification_manager(manager);
//! ```
//!
//! ## See Also
//! - [README](https://github.com/cosmicglue-io/axtra)
//! - [API Docs (docs.rs)](https://docs.rs/axtra)
//! - [Changelog](./CHANGELOG.md)

pub use axtra_macros::*;

#[cfg(feature = "bouncer")]
pub mod bouncer;
pub mod errors;
#[cfg(feature = "notifier")]
pub mod notifier;
pub mod response;
pub mod routes;

//! # Notifier
//!
//! Pluggable notification system for error alerts and incident tracking.
//!
//! ## Feature Flags
//!
//! This module requires the `notifier` feature to be enabled. Individual providers
//! are gated by their own features:
//!
//! - `notify-error-slack` - Slack webhook notifications
//! - `notify-error-discord` - Discord webhook notifications
//! - `notify-error-ntfy` - ntfy push notifications
//! - `notify-error-posthog` - PostHog error tracking
//!
//! ## Architecture
//!
//! The notification system uses a trait-based approach where providers implement
//! [`ErrorNotifier`] to handle error events. The [`NotificationManager`] coordinates
//! multiple providers, sending events to all of them concurrently.
//!
//! ## Usage
//!
//! ### Simple setup with environment variables
//!
//! ```rust,ignore
//! use axtra::notifier::NotificationManager;
//! use axtra::errors::notifiers::init_notification_manager;
//!
//! // Auto-detects providers from env vars:
//! // - SLACK_ERROR_WEBHOOK_URL (+ optional SLACK_ERROR_MENTION)
//! // - DISCORD_ERROR_WEBHOOK_URL (+ optional DISCORD_ERROR_MENTION)
//! // - NTFY_TOPIC (+ optional NTFY_SERVER_URL, NTFY_ACCESS_TOKEN)
//! // - POSTHOG_PROJECT_TOKEN (+ optional POSTHOG_HOST, ENVIRONMENT/ENV,
//! //   SERVICE/APP_NAME, RELEASE/VERSION)
//! init_notification_manager(NotificationManager::from_env());
//! ```
//!
//! ### Builder pattern (recommended)
//!
//! ```rust,ignore
//! use axtra::notifier::{NotificationManager, SlackConfig, NtfyConfig, PosthogConfig};
//! use axtra::errors::notifiers::init_notification_manager;
//!
//! let manager = NotificationManager::builder()
//!     .with_slack(SlackConfig::new("https://hooks.slack.com/services/...")
//!         .with_mention("@oncall"))
//!     .with_ntfy(NtfyConfig::new("my-app-errors"))
//!     .with_posthog(PosthogConfig::new(std::env::var("POSTHOG_PROJECT_TOKEN").unwrap())
//!         .with_environment("production")
//!         .with_service("my-app"))
//!     .unwrap()
//!     .build();
//!
//! init_notification_manager(manager);
//! ```
//!
//! ### Custom provider
//!
//! ```rust,ignore
//! use axtra::notifier::{ErrorNotifier, ErrorEvent, NotifyError, NotifyFuture, NotificationManager};
//! use reqwest::Client;
//! use std::sync::Arc;
//!
//! struct PagerDutyProvider {
//!     api_key: String,
//! }
//!
//! impl ErrorNotifier for PagerDutyProvider {
//!     fn notify<'a>(&'a self, client: &'a Client, event: &'a ErrorEvent) -> NotifyFuture<'a> {
//!         Box::pin(async move {
//!             // POST to PagerDuty API
//!             Ok(())
//!         })
//!     }
//!
//!     fn name(&self) -> &'static str {
//!         "pagerduty"
//!     }
//! }
//!
//! let manager = NotificationManager::builder()
//!     .with_provider(Arc::new(PagerDutyProvider { api_key: "...".into() }))
//!     .build();
//! ```
//!
//! ## See Also
//! - [README](https://github.com/cosmicglue-io/axtra)
//! - [docs.rs/axtra](https://docs.rs/axtra)

#[cfg(feature = "notifier")]
mod event;
#[cfg(feature = "notifier")]
mod manager;
#[cfg(feature = "notifier")]
pub mod providers;
#[cfg(feature = "notifier")]
mod traits;

#[cfg(feature = "notifier")]
pub use event::ErrorEvent;
#[cfg(feature = "notifier")]
pub use manager::{NotificationManager, NotificationManagerBuilder};
#[cfg(feature = "notifier")]
pub use traits::{ErrorNotifier, NotifyError, NotifyFuture};

// Re-export provider configs at top level for convenience
#[cfg(feature = "notify-error-discord")]
pub use providers::{DiscordConfig, DiscordProvider};
#[cfg(feature = "notify-error-ntfy")]
pub use providers::{NtfyConfig, NtfyProvider};
#[cfg(feature = "notify-error-posthog")]
pub use providers::{PosthogConfig, PosthogProvider};
#[cfg(feature = "notify-error-slack")]
pub use providers::{SlackConfig, SlackProvider};

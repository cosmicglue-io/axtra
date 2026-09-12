//! Error notification handlers using the pluggable notification system.
//!
//! This module provides a global [`NotificationManager`] that can be configured
//! at application startup to send error notifications to multiple providers.
//!
//! ## Usage
//!
//! ```rust,ignore
//! use axtra::notifier::{NotificationManager, SlackConfig};
//! use axtra::errors::notifiers::init_notification_manager;
//!
//! // Option 1: Auto-configure from environment variables
//! init_notification_manager(NotificationManager::from_env());
//!
//! // Option 2: Manual configuration
//! let manager = NotificationManager::builder()
//!     .with_slack(SlackConfig::new("https://hooks.slack.com/services/..."))
//!     .build();
//! init_notification_manager(manager);
//! ```

#[cfg(feature = "notifier")]
use crate::notifier::NotificationManager;

#[cfg(feature = "notifier")]
use std::sync::OnceLock;

#[cfg(feature = "notifier")]
static NOTIFICATION_MANAGER: OnceLock<NotificationManager> = OnceLock::new();

/// Initialize the global notification manager.
///
/// This should be called once at application startup. If called multiple times,
/// subsequent calls are ignored and the original manager is retained.
///
/// # Example
///
/// ```rust,ignore
/// use axtra::notifier::NotificationManager;
/// use axtra::errors::notifiers::init_notification_manager;
///
/// init_notification_manager(NotificationManager::from_env());
/// ```
#[cfg(feature = "notifier")]
pub fn init_notification_manager(manager: NotificationManager) {
    let _ = NOTIFICATION_MANAGER.set(manager);
}

/// Get a reference to the global notification manager.
///
/// Returns `None` if the manager hasn't been initialized. In that case,
/// a default manager will be created from environment variables on first
/// notification attempt.
#[cfg(feature = "notifier")]
pub fn notification_manager() -> &'static NotificationManager {
    NOTIFICATION_MANAGER.get_or_init(NotificationManager::from_env)
}

/// Drain in-flight notifications and flush provider buffers.
#[cfg(feature = "notifier")]
pub async fn shutdown_notification_manager() {
    notification_manager().shutdown().await;
}

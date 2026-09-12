//! Trait definitions for notification providers.

use reqwest::Client;
use std::future::Future;
use std::pin::Pin;

use super::ErrorEvent;

/// Error type for notification operations.
#[derive(Debug, thiserror::Error)]
pub enum NotifyError {
    /// Transient error - may succeed on retry (network issues, rate limits)
    #[error("transient error: {0}")]
    Transient(String),
    /// Permanent error - will not succeed on retry (invalid config, auth failure)
    #[error("permanent error: {0}")]
    Permanent(String),
}

impl NotifyError {
    /// Create a transient error from any error type.
    pub fn transient(err: impl std::fmt::Display) -> Self {
        Self::Transient(err.to_string())
    }

    /// Create a permanent error from any error type.
    pub fn permanent(err: impl std::fmt::Display) -> Self {
        Self::Permanent(err.to_string())
    }
}

impl From<reqwest::Error> for NotifyError {
    fn from(err: reqwest::Error) -> Self {
        if err.is_timeout() || err.is_connect() {
            Self::Transient(err.to_string())
        } else if err.is_status() {
            // 4xx errors are permanent, 5xx are transient
            if err.status().is_some_and(|s| s.is_server_error()) {
                Self::Transient(err.to_string())
            } else {
                Self::Permanent(err.to_string())
            }
        } else {
            Self::Transient(err.to_string())
        }
    }
}

/// Boxed future type for dyn-compatible async trait methods.
pub type NotifyFuture<'a> = Pin<Box<dyn Future<Output = Result<(), NotifyError>> + Send + 'a>>;

/// Trait for notification providers.
///
/// Implement this trait to create custom notification providers.
/// Providers receive error events and decide how to handle them:
/// - **Webhook providers** (Slack, Discord, ntfy): Format and send messages
/// - **Capture providers** (PostHog, Sentry): Create incidents/issues
///
/// # Example
///
/// ```rust,ignore
/// use axtra::notifier::{ErrorNotifier, ErrorEvent, NotifyError, NotifyFuture};
/// use reqwest::Client;
///
/// struct MyProvider {
///     api_key: String,
/// }
///
/// impl ErrorNotifier for MyProvider {
///     fn notify<'a>(&'a self, client: &'a Client, event: &'a ErrorEvent) -> NotifyFuture<'a> {
///         Box::pin(async move {
///             // Send to your notification service
///             client.post("https://api.example.com/notify")
///                 .header("Authorization", &self.api_key)
///                 .json(&serde_json::json!({
///                     "message": event.message,
///                     "severity": event.severity(),
///                 }))
///                 .send()
///                 .await?
///                 .error_for_status()?;
///             Ok(())
///         })
///     }
///
///     fn name(&self) -> &'static str {
///         "my-provider"
///     }
/// }
/// ```
pub trait ErrorNotifier: Send + Sync {
    /// Send a notification for the given error event.
    ///
    /// The `client` is a shared HTTP client for making requests.
    /// Returns `Ok(())` on success, or a `NotifyError` on failure.
    fn notify<'a>(&'a self, client: &'a Client, event: &'a ErrorEvent) -> NotifyFuture<'a>;

    /// Flush buffered notifications and stop provider workers.
    ///
    /// Providers without buffering can use this default no-op.
    fn shutdown(&self) -> NotifyFuture<'_> {
        Box::pin(async { Ok(()) })
    }

    /// Human-readable name for this provider (used in logs).
    fn name(&self) -> &'static str;
}

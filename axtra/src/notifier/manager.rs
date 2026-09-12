//! Notification manager for coordinating multiple providers.

use reqwest::Client;
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};
use std::time::Duration;
use tokio::sync::{Notify, Semaphore, TryAcquireError};

use super::{ErrorEvent, ErrorNotifier};

#[cfg(not(test))]
const NOTIFICATION_DRAIN_TIMEOUT: Duration = Duration::from_secs(10);
#[cfg(test)]
const NOTIFICATION_DRAIN_TIMEOUT: Duration = Duration::from_millis(50);
#[cfg(not(test))]
const PROVIDER_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(5);
#[cfg(test)]
const PROVIDER_SHUTDOWN_TIMEOUT: Duration = Duration::from_millis(50);
const NOTIFICATION_REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_IN_FLIGHT_NOTIFICATIONS: usize = 64;

#[cfg(feature = "notify-error-discord")]
use super::providers::DiscordProvider;
#[cfg(feature = "notify-error-ntfy")]
use super::providers::NtfyProvider;
#[cfg(feature = "notify-error-posthog")]
use super::providers::PosthogProvider;
#[cfg(feature = "notify-error-slack")]
use super::providers::SlackProvider;

/// Manages multiple notification providers and dispatches error events to all of them.
///
/// # Example
///
/// ```rust,ignore
/// use axtra::notifier::{NotificationManager, NtfyConfig, SlackConfig};
///
/// let manager = NotificationManager::builder()
///     .with_slack(SlackConfig::new("https://hooks.slack.com/services/..."))
///     .with_ntfy(NtfyConfig {
///         server_url: "https://ntfy.sh".into(),
///         topic: "my-app-errors".into(),
///         access_token: None,
///     })
///     .build();
///
/// // Or auto-configure from environment variables:
/// let manager = NotificationManager::from_env();
/// ```
pub struct NotificationManager {
    providers: Vec<Arc<dyn ErrorNotifier>>,
    client: Client,
    active_notifications: AtomicUsize,
    idle: Notify,
    notification_permits: Arc<Semaphore>,
    shutting_down: AtomicBool,
}

struct ActiveNotificationGuard<'a>(&'a NotificationManager);

impl Drop for ActiveNotificationGuard<'_> {
    fn drop(&mut self) {
        if self.0.active_notifications.fetch_sub(1, Ordering::AcqRel) == 1 {
            self.0.idle.notify_waiters();
        }
    }
}

impl NotificationManager {
    /// Create a new notification manager with no providers.
    pub fn new() -> Self {
        Self {
            providers: Vec::new(),
            client: notification_client(),
            active_notifications: AtomicUsize::new(0),
            idle: Notify::new(),
            notification_permits: Arc::new(Semaphore::new(MAX_IN_FLIGHT_NOTIFICATIONS)),
            shutting_down: AtomicBool::new(false),
        }
    }

    /// Create a builder for configuring the notification manager.
    pub fn builder() -> NotificationManagerBuilder {
        NotificationManagerBuilder::new()
    }

    /// Create a notification manager from environment variables.
    ///
    /// This will automatically configure providers based on the following env vars:
    /// - `SLACK_ERROR_WEBHOOK_URL` - Slack webhook URL
    /// - `DISCORD_ERROR_WEBHOOK_URL` - Discord webhook URL
    /// - `NTFY_TOPIC` - ntfy topic (uses `NTFY_SERVER_URL` for custom server, `NTFY_ACCESS_TOKEN` for auth)
    /// - `POSTHOG_PROJECT_TOKEN` - PostHog project token (required)
    /// - `POSTHOG_HOST` - PostHog ingestion host (optional)
    /// - `ENVIRONMENT`, `APP_ENV`, or `ENV` - environment name like "production" (optional)
    /// - `SERVICE` or `APP_NAME` - service name like "api" (optional)
    /// - `RELEASE` or `VERSION` - release version (optional)
    pub fn from_env() -> Self {
        #[allow(unused_mut)]
        let mut builder = Self::builder();

        #[cfg(feature = "notify-error-slack")]
        if let Ok(url) = std::env::var("SLACK_ERROR_WEBHOOK_URL") {
            use super::providers::SlackConfig;
            let mut config = SlackConfig::new(url);
            if let Ok(mention) = std::env::var("SLACK_ERROR_MENTION") {
                config = config.with_mention(mention);
            }
            builder = builder.with_slack(config);
        }

        #[cfg(feature = "notify-error-discord")]
        if let Ok(url) = std::env::var("DISCORD_ERROR_WEBHOOK_URL") {
            use super::providers::DiscordConfig;
            let mut config = DiscordConfig::new(url);
            if let Ok(mention) = std::env::var("DISCORD_ERROR_MENTION") {
                config = config.with_mention(mention);
            }
            builder = builder.with_discord(config);
        }

        #[cfg(feature = "notify-error-ntfy")]
        if let Ok(topic) = std::env::var("NTFY_TOPIC") {
            use super::providers::NtfyConfig;
            builder = builder.with_ntfy(NtfyConfig {
                server_url: std::env::var("NTFY_SERVER_URL")
                    .unwrap_or_else(|_| "https://ntfy.sh".into()),
                topic,
                access_token: std::env::var("NTFY_ACCESS_TOKEN").ok(),
            });
        }

        #[cfg(feature = "notify-error-posthog")]
        if let Ok(project_token) = std::env::var("POSTHOG_PROJECT_TOKEN")
            && !project_token.trim().is_empty()
        {
            use super::providers::PosthogConfig;
            let mut config = PosthogConfig::new(project_token);
            if let Ok(host) = std::env::var("POSTHOG_HOST") {
                config = config.with_host(host);
            }
            if let Ok(env) = std::env::var("ENVIRONMENT")
                .or_else(|_| std::env::var("APP_ENV"))
                .or_else(|_| std::env::var("ENV"))
            {
                config = config.with_environment(env);
            }
            if let Ok(service) = std::env::var("SERVICE").or_else(|_| std::env::var("APP_NAME")) {
                config = config.with_service(service);
            }
            if let Ok(release) = std::env::var("RELEASE").or_else(|_| std::env::var("VERSION")) {
                config = config.with_release(release);
            }
            match PosthogProvider::new(config) {
                Ok(provider) => builder = builder.with_provider(Arc::new(provider)),
                Err(error) => tracing::warn!(%error, "invalid PostHog notifier configuration"),
            }
        }

        builder.build()
    }

    /// Check if any providers are configured.
    pub fn has_providers(&self) -> bool {
        !self.providers.is_empty()
    }

    /// Get the number of configured providers.
    pub fn provider_count(&self) -> usize {
        self.providers.len()
    }

    /// Send a notification to all configured providers within the manager's
    /// concurrency and lifecycle bounds.
    ///
    /// This method sends the event to all providers concurrently and logs any failures.
    /// It does not fail if individual providers fail - errors are logged and execution continues.
    pub async fn notify(&self, event: &ErrorEvent) {
        if self.providers.is_empty() || self.shutting_down.load(Ordering::Acquire) {
            return;
        }
        let Ok(permit) = Arc::clone(&self.notification_permits).acquire_owned().await else {
            return;
        };
        let Some((_permit, _guard)) = self.activate_notification(permit) else {
            return;
        };

        self.notify_providers(event).await;
    }

    async fn notify_providers(&self, event: &ErrorEvent) {
        let futures: Vec<_> = self
            .providers
            .iter()
            .map(|provider| {
                let client = self.client.clone();
                let provider = Arc::clone(provider);
                let event = event.clone();
                async move {
                    let name = provider.name();
                    if let Err(err) = provider.notify(&client, &event).await {
                        tracing::warn!(provider = name, error = %err, "notification failed");
                    }
                }
            })
            .collect();

        futures::future::join_all(futures).await;
    }

    fn activate_notification(
        &self,
        permit: tokio::sync::OwnedSemaphorePermit,
    ) -> Option<(
        tokio::sync::OwnedSemaphorePermit,
        ActiveNotificationGuard<'_>,
    )> {
        if self.providers.is_empty() || self.shutting_down.load(Ordering::Acquire) {
            return None;
        }
        self.active_notifications.fetch_add(1, Ordering::AcqRel);
        let guard = ActiveNotificationGuard(self);
        if self.shutting_down.load(Ordering::Acquire) {
            return None;
        }

        Some((permit, guard))
    }

    /// Dispatch a notification without blocking the response path while
    /// retaining it for an orderly shutdown.
    pub(crate) fn dispatch(&'static self, event: ErrorEvent) {
        if self.providers.is_empty() || self.shutting_down.load(Ordering::Acquire) {
            return;
        }
        let permit = match Arc::clone(&self.notification_permits).try_acquire_owned() {
            Ok(permit) => permit,
            Err(TryAcquireError::NoPermits) => {
                tracing::warn!("notification capacity exhausted; dropping error notification");
                return;
            }
            Err(TryAcquireError::Closed) => return,
        };
        let Some((permit, guard)) = self.activate_notification(permit) else {
            return;
        };
        tokio::spawn(async move {
            let _guard = guard;
            let _permit = permit;
            self.notify_providers(&event).await;
        });
    }

    /// Wait for dispatched notifications, then flush provider buffers.
    pub async fn shutdown(&self) {
        self.shutting_down.store(true, Ordering::Release);
        self.notification_permits.close();
        let drain = async {
            loop {
                let notified = self.idle.notified();
                if self.active_notifications.load(Ordering::Acquire) == 0 {
                    break;
                }
                notified.await;
            }
        };

        if tokio::time::timeout(NOTIFICATION_DRAIN_TIMEOUT, drain)
            .await
            .is_err()
        {
            tracing::warn!("timed out waiting for error notifications to drain");
            return;
        }

        futures::future::join_all(self.providers.iter().map(|provider| async move {
            match tokio::time::timeout(PROVIDER_SHUTDOWN_TIMEOUT, provider.shutdown()).await {
                Ok(Ok(())) => {}
                Ok(Err(error)) => {
                    tracing::warn!(provider = provider.name(), %error, "provider shutdown failed");
                }
                Err(_) => {
                    tracing::warn!(provider = provider.name(), "provider shutdown timed out");
                }
            }
        }))
        .await;
    }
}

impl Default for NotificationManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Builder for configuring a [`NotificationManager`].
pub struct NotificationManagerBuilder {
    providers: Vec<Arc<dyn ErrorNotifier>>,
}

impl NotificationManagerBuilder {
    /// Create a new builder.
    pub fn new() -> Self {
        Self {
            providers: Vec::new(),
        }
    }

    /// Add a custom provider.
    pub fn with_provider(mut self, provider: Arc<dyn ErrorNotifier>) -> Self {
        self.providers.push(provider);
        self
    }

    /// Add a Slack provider with the given configuration.
    #[cfg(feature = "notify-error-slack")]
    pub fn with_slack(self, config: super::providers::SlackConfig) -> Self {
        self.with_provider(Arc::new(SlackProvider::new(config)))
    }

    /// Add a Discord provider with the given configuration.
    #[cfg(feature = "notify-error-discord")]
    pub fn with_discord(self, config: super::providers::DiscordConfig) -> Self {
        self.with_provider(Arc::new(DiscordProvider::new(config)))
    }

    /// Add an ntfy provider with the given configuration.
    #[cfg(feature = "notify-error-ntfy")]
    pub fn with_ntfy(self, config: super::providers::NtfyConfig) -> Self {
        self.with_provider(Arc::new(NtfyProvider::new(config)))
    }

    /// Add a PostHog error-tracking provider.
    #[cfg(feature = "notify-error-posthog")]
    pub fn with_posthog(
        self,
        config: super::providers::PosthogConfig,
    ) -> Result<Self, super::NotifyError> {
        Ok(self.with_provider(Arc::new(PosthogProvider::new(config)?)))
    }

    /// Build the notification manager.
    pub fn build(self) -> NotificationManager {
        NotificationManager {
            providers: self.providers,
            client: notification_client(),
            active_notifications: AtomicUsize::new(0),
            idle: Notify::new(),
            notification_permits: Arc::new(Semaphore::new(MAX_IN_FLIGHT_NOTIFICATIONS)),
            shutting_down: AtomicBool::new(false),
        }
    }
}

fn notification_client() -> Client {
    Client::builder()
        .timeout(NOTIFICATION_REQUEST_TIMEOUT)
        .build()
        .expect("default notification HTTP client should build")
}

impl Default for NotificationManagerBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicUsize;

    use super::*;
    use crate::errors::ErrorCode;
    use crate::notifier::NotifyFuture;

    struct SlowProvider;

    impl ErrorNotifier for SlowProvider {
        fn notify<'a>(&'a self, _: &'a Client, _: &'a ErrorEvent) -> NotifyFuture<'a> {
            Box::pin(async move {
                tokio::time::sleep(Duration::from_millis(10)).await;
                Ok(())
            })
        }

        fn name(&self) -> &'static str {
            "slow"
        }
    }

    struct PanicProvider;

    impl ErrorNotifier for PanicProvider {
        fn notify<'a>(&'a self, _: &'a Client, _: &'a ErrorEvent) -> NotifyFuture<'a> {
            Box::pin(async move { panic!("provider panic") })
        }

        fn name(&self) -> &'static str {
            "panic"
        }
    }

    struct HangingShutdownProvider;

    impl ErrorNotifier for HangingShutdownProvider {
        fn notify<'a>(&'a self, _: &'a Client, _: &'a ErrorEvent) -> NotifyFuture<'a> {
            Box::pin(async { Ok(()) })
        }

        fn shutdown(&self) -> NotifyFuture<'_> {
            Box::pin(std::future::pending())
        }

        fn name(&self) -> &'static str {
            "hanging-shutdown"
        }
    }

    struct HangingNotifyProvider {
        shutdowns: Arc<AtomicUsize>,
    }

    struct CountingProvider(Arc<AtomicUsize>);

    impl ErrorNotifier for CountingProvider {
        fn notify<'a>(&'a self, _: &'a Client, _: &'a ErrorEvent) -> NotifyFuture<'a> {
            Box::pin(async move {
                self.0.fetch_add(1, Ordering::AcqRel);
                Ok(())
            })
        }

        fn name(&self) -> &'static str {
            "counting"
        }
    }

    impl ErrorNotifier for HangingNotifyProvider {
        fn notify<'a>(&'a self, _: &'a Client, _: &'a ErrorEvent) -> NotifyFuture<'a> {
            Box::pin(std::future::pending())
        }

        fn shutdown(&self) -> NotifyFuture<'_> {
            Box::pin(async move {
                self.shutdowns.fetch_add(1, Ordering::AcqRel);
                Ok(())
            })
        }

        fn name(&self) -> &'static str {
            "hanging-notify"
        }
    }

    fn event() -> ErrorEvent {
        ErrorEvent::new("Acme", ErrorCode::Exception, "failed", "src/main.rs:1")
    }

    #[tokio::test]
    async fn dispatch_is_bounded() {
        let manager = Box::leak(Box::new(
            NotificationManager::builder()
                .with_provider(Arc::new(SlowProvider))
                .build(),
        ));

        for _ in 0..=MAX_IN_FLIGHT_NOTIFICATIONS {
            manager.dispatch(event());
        }

        assert_eq!(
            manager.active_notifications.load(Ordering::Acquire),
            MAX_IN_FLIGHT_NOTIFICATIONS
        );
        manager.shutdown().await;
    }

    #[tokio::test]
    async fn panicking_provider_releases_activity_count() {
        let manager = Box::leak(Box::new(
            NotificationManager::builder()
                .with_provider(Arc::new(PanicProvider))
                .build(),
        ));

        manager.dispatch(event());
        tokio::time::timeout(Duration::from_secs(1), manager.shutdown())
            .await
            .expect("shutdown should not wait on a panicked provider task");

        assert_eq!(manager.active_notifications.load(Ordering::Acquire), 0);

        manager.dispatch(event());
        assert_eq!(manager.active_notifications.load(Ordering::Acquire), 0);
    }

    #[tokio::test]
    async fn provider_shutdown_is_bounded() {
        let manager = NotificationManager::builder()
            .with_provider(Arc::new(HangingShutdownProvider))
            .build();

        tokio::time::timeout(Duration::from_secs(1), manager.shutdown())
            .await
            .expect("provider shutdown should have a deadline");
    }

    #[tokio::test]
    async fn drain_timeout_does_not_overlap_provider_shutdown() {
        let shutdowns = Arc::new(AtomicUsize::new(0));
        let manager = Box::leak(Box::new(
            NotificationManager::builder()
                .with_provider(Arc::new(HangingNotifyProvider {
                    shutdowns: Arc::clone(&shutdowns),
                }))
                .build(),
        ));

        manager.dispatch(event());
        tokio::task::yield_now().await;
        manager.shutdown().await;

        assert_eq!(shutdowns.load(Ordering::Acquire), 0);
    }

    #[tokio::test]
    async fn public_notify_is_ignored_after_shutdown_starts() {
        let calls = Arc::new(AtomicUsize::new(0));
        let manager = NotificationManager::builder()
            .with_provider(Arc::new(CountingProvider(Arc::clone(&calls))))
            .build();

        manager.shutdown().await;
        manager.notify(&event()).await;

        assert_eq!(calls.load(Ordering::Acquire), 0);
    }

    #[tokio::test]
    async fn public_notify_waits_for_capacity() {
        let calls = Arc::new(AtomicUsize::new(0));
        let manager = Arc::new(
            NotificationManager::builder()
                .with_provider(Arc::new(CountingProvider(Arc::clone(&calls))))
                .build(),
        );
        let mut permits = Vec::new();
        for _ in 0..MAX_IN_FLIGHT_NOTIFICATIONS {
            permits.push(
                Arc::clone(&manager.notification_permits)
                    .acquire_owned()
                    .await
                    .unwrap(),
            );
        }
        let notifying_manager = Arc::clone(&manager);
        let task = tokio::spawn(async move {
            notifying_manager.notify(&event()).await;
        });
        tokio::task::yield_now().await;

        assert!(!task.is_finished());
        assert_eq!(calls.load(Ordering::Acquire), 0);

        permits.pop();
        tokio::time::timeout(Duration::from_secs(1), task)
            .await
            .expect("public notify should resume when capacity is available")
            .unwrap();
        assert_eq!(calls.load(Ordering::Acquire), 1);
    }
}

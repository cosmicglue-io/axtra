//! ntfy push notification provider.

use reqwest::Client;

use crate::notifier::{ErrorEvent, ErrorNotifier, NotifyFuture};

/// Configuration for the ntfy provider.
#[derive(Debug, Clone)]
pub struct NtfyConfig {
    /// Server URL (default: `https://ntfy.sh`)
    pub server_url: String,
    /// Topic to publish to
    pub topic: String,
    /// Optional access token for authentication
    pub access_token: Option<String>,
}

impl NtfyConfig {
    /// Create a new config with the default ntfy.sh server.
    pub fn new(topic: impl Into<String>) -> Self {
        Self {
            server_url: "https://ntfy.sh".into(),
            topic: topic.into(),
            access_token: None,
        }
    }

    /// Set a custom server URL.
    pub fn with_server(mut self, url: impl Into<String>) -> Self {
        self.server_url = url.into();
        self
    }

    /// Set an access token for authentication.
    pub fn with_token(mut self, token: impl Into<String>) -> Self {
        self.access_token = Some(token.into());
        self
    }
}

/// ntfy push notification provider.
///
/// Sends push notifications via [ntfy](https://ntfy.sh), a simple HTTP-based
/// pub-sub notification service.
///
/// # Example
///
/// ```rust,ignore
/// use axtra::notifier::{NotificationManager, NtfyConfig};
///
/// let manager = NotificationManager::builder()
///     .with_ntfy(NtfyConfig::new("my-app-errors"))
///     .build();
///
/// // With custom server and authentication:
/// let manager = NotificationManager::builder()
///     .with_ntfy(NtfyConfig::new("alerts")
///         .with_server("https://ntfy.example.com")
///         .with_token("tk_secret"))
///     .build();
/// ```
pub struct NtfyProvider {
    config: NtfyConfig,
}

impl NtfyProvider {
    /// Create a new ntfy provider with the given configuration.
    pub fn new(config: NtfyConfig) -> Self {
        Self { config }
    }
}

impl ErrorNotifier for NtfyProvider {
    fn notify<'a>(&'a self, client: &'a Client, event: &'a ErrorEvent) -> NotifyFuture<'a> {
        Box::pin(async move {
            let url = format!(
                "{}/{}",
                self.config.server_url.trim_end_matches('/'),
                self.config.topic
            );
            let title = format!(
                "🔴 {} Error — {}",
                event.severity().to_uppercase(),
                event.app_name
            );
            let body = format!(
                "[{}][{:?}] {}",
                event.location, event.error_code, event.message
            );

            // Map severity to ntfy priority (1-5, with 5 being max)
            let priority = match event.severity() {
                "critical" => "5",
                "major" => "4",
                "minor" => "3",
                "warning" => "2",
                _ => "3",
            };

            let mut request = client
                .post(&url)
                .header("Title", title)
                .header("Priority", priority)
                .header(
                    "Tags",
                    format!("error,{:?}", event.error_code).to_lowercase(),
                )
                .body(body);

            if let Some(ref token) = self.config.access_token {
                request = request.header("Authorization", format!("Bearer {}", token));
            }

            request.send().await?.error_for_status()?;

            Ok(())
        })
    }

    fn name(&self) -> &'static str {
        "ntfy"
    }
}

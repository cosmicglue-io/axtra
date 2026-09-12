//! PostHog error tracking provider.

use std::fmt;

use posthog_rs::{CaptureExceptionOptions, ClientOptionsBuilder, ErrorTrackingOptionsBuilder};
use reqwest::Client;

use crate::errors::ErrorCode;
use crate::notifier::{ErrorEvent, ErrorNotifier, NotifyError, NotifyFuture};

/// Configuration for PostHog error tracking.
#[derive(Clone)]
pub struct PosthogConfig {
    pub project_token: String,
    pub host: String,
    pub environment: Option<String>,
    pub service: Option<String>,
    pub release: Option<String>,
}

impl PosthogConfig {
    pub fn new(project_token: impl Into<String>) -> Self {
        Self {
            project_token: project_token.into(),
            host: posthog_rs::DEFAULT_HOST.into(),
            environment: None,
            service: None,
            release: None,
        }
    }

    pub fn with_host(mut self, host: impl Into<String>) -> Self {
        self.host = host.into();
        self
    }

    pub fn with_environment(mut self, environment: impl Into<String>) -> Self {
        self.environment = Some(environment.into());
        self
    }

    pub fn with_service(mut self, service: impl Into<String>) -> Self {
        self.service = Some(service.into());
        self
    }

    pub fn with_release(mut self, release: impl Into<String>) -> Self {
        self.release = Some(release.into());
        self
    }
}

/// Captures Axtra server errors as PostHog `$exception` events.
pub struct PosthogProvider {
    client: tokio::sync::OnceCell<posthog_rs::Client>,
    options: posthog_rs::ClientOptions,
    config: PosthogConfig,
}

impl PosthogProvider {
    pub fn new(config: PosthogConfig) -> Result<Self, NotifyError> {
        if config.project_token.trim().is_empty() {
            return Err(NotifyError::permanent(
                "PostHog project token cannot be empty",
            ));
        }

        let error_tracking = ErrorTrackingOptionsBuilder::default()
            .capture_stacktrace(false)
            .build()
            .map_err(NotifyError::permanent)?;
        let options = ClientOptionsBuilder::default()
            .api_key(config.project_token.clone())
            .host(config.host.clone())
            .disable_geoip(true)
            .flush_at(20)
            .flush_interval_ms(5_000)
            .max_queue_size(1_000)
            .shutdown_timeout_ms(5_000)
            .error_tracking(error_tracking)
            .build()
            .map_err(NotifyError::permanent)?;

        Ok(Self {
            client: tokio::sync::OnceCell::new(),
            options,
            config,
        })
    }

    async fn client(&self) -> &posthog_rs::Client {
        self.client
            .get_or_init(|| posthog_rs::client(self.options.clone()))
            .await
    }

    fn exception_options(
        &self,
        event: &ErrorEvent,
    ) -> Result<CaptureExceptionOptions, NotifyError> {
        let service = self.config.service.as_deref().unwrap_or(&event.app_name);
        let error_code = format!("{:?}", event.error_code);
        let fingerprint = event.fingerprint.clone().unwrap_or_else(|| {
            format!(
                "{service}:{error_code}:{}:{}",
                event.location, event.message
            )
        });

        let mut options = CaptureExceptionOptions::new()
            .fingerprint(fingerprint)
            .level(level(event.error_code))
            .property("app", service)
            .map_err(NotifyError::permanent)?
            .property("error_code", error_code)
            .map_err(NotifyError::permanent)?
            .property("source_location", &event.location)
            .map_err(NotifyError::permanent)?;

        if let Some(environment) = &self.config.environment {
            options = options
                .property("environment", environment)
                .map_err(NotifyError::permanent)?;
        }
        if let Some(release) = &self.config.release {
            options = options
                .property("release", release)
                .map_err(NotifyError::permanent)?;
        }

        Ok(options)
    }
}

impl ErrorNotifier for PosthogProvider {
    fn notify<'a>(&'a self, _client: &'a Client, event: &'a ErrorEvent) -> NotifyFuture<'a> {
        Box::pin(async move {
            let error = ReportedError {
                message: event.message.clone(),
            };
            self.client()
                .await
                .capture_exception_with(&error, self.exception_options(event)?)
                .await
                .map_err(NotifyError::transient)
        })
    }

    fn shutdown(&self) -> NotifyFuture<'_> {
        Box::pin(async move {
            if let Some(client) = self.client.get() {
                client.shutdown().await;
            }
            Ok(())
        })
    }

    fn name(&self) -> &'static str {
        "posthog"
    }
}

#[derive(Debug)]
struct ReportedError {
    message: String,
}

impl fmt::Display for ReportedError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for ReportedError {}

fn level(error_code: ErrorCode) -> &'static str {
    match error_code {
        ErrorCode::Database => "fatal",
        ErrorCode::Exception => "error",
        ErrorCode::Authentication
        | ErrorCode::Authorization
        | ErrorCode::BadRequest
        | ErrorCode::NotFound
        | ErrorCode::Validation => "warning",
    }
}

#[cfg(test)]
mod tests {
    use httpmock::prelude::*;

    use super::*;

    #[tokio::test]
    async fn sends_canonical_exception_without_sensitive_event_context() {
        let server = MockServer::start();
        let capture = server.mock(|when, then| {
            when.method(POST)
                .path("/batch/")
                .body_includes(r#""event":"$exception""#)
                .body_includes(r#""app":"Acme""#)
                .body_includes(r#""environment":"production""#)
                .body_includes(r#""release":"abc123""#)
                .body_includes(r#""source_location":"src/checkout.rs:42""#)
                .is_true(|request| {
                    let body = String::from_utf8_lossy(request.body_ref());
                    !body.contains("customer@example.com")
                        && !body.contains("?token=secret")
                        && !body.contains("database password")
                });
            then.status(200);
        });

        let provider = PosthogProvider::new(
            PosthogConfig::new("phc_test")
                .with_host(server.base_url())
                .with_environment("production")
                .with_service("Acme")
                .with_release("abc123"),
        )
        .unwrap();
        let event = ErrorEvent::new(
            "fallback",
            ErrorCode::Exception,
            "Checkout failed",
            "src/checkout.rs:42",
        )
        .with_source_error("database password")
        .with_request("POST", "https://example.com/checkout?token=secret")
        .with_user_email("user-1", "customer@example.com");

        provider
            .notify(&Client::new(), &event)
            .await
            .expect("capture should enqueue");
        provider.shutdown().await.expect("shutdown should flush");

        capture.assert_calls(1);
    }

    #[test]
    fn rejects_empty_project_token() {
        assert!(PosthogProvider::new(PosthogConfig::new("  ")).is_err());
    }
}

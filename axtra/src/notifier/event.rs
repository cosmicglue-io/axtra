//! Error event representation for notification providers.

use crate::errors::ErrorCode;
use time::OffsetDateTime;

/// Represents an error event to be sent to notification providers.
///
/// This struct contains all the context needed for providers to format
/// and send notifications. Providers decide how to use each field based
/// on their capabilities (webhooks format messages, capture services
/// create incidents, etc.).
#[derive(Debug, Clone)]
pub struct ErrorEvent {
    /// Application name (from `APP_NAME` env var or "Rust")
    pub app_name: String,
    /// The error code category
    pub error_code: ErrorCode,
    /// Human-readable error message
    pub message: String,
    /// Source location where the error occurred (e.g., "module::function:42")
    pub location: String,
    /// When the error occurred
    pub timestamp: OffsetDateTime,
    /// Serialized error chain for capture providers (optional)
    pub source_error: Option<String>,

    // Error tracking context for capture providers.
    /// Full stacktrace
    pub stacktrace: Option<String>,
    /// Request URL that triggered the error
    pub request_url: Option<String>,
    /// HTTP method (GET, POST, etc.)
    pub request_method: Option<String>,
    /// User identifier
    pub user_id: Option<String>,
    /// User email
    pub user_email: Option<String>,
    /// Custom indexed tags (searchable)
    pub tags: Option<serde_json::Value>,
    /// Extra context data (not indexed)
    pub extra: Option<serde_json::Value>,
    /// Breadcrumb trail leading to the error
    pub breadcrumbs: Option<serde_json::Value>,
    /// Custom fingerprint for error grouping
    pub fingerprint: Option<String>,
}

impl ErrorEvent {
    /// Create a new error event with the current timestamp.
    pub fn new(
        app_name: impl Into<String>,
        error_code: ErrorCode,
        message: impl Into<String>,
        location: impl Into<String>,
    ) -> Self {
        Self {
            app_name: app_name.into(),
            error_code,
            message: message.into(),
            location: location.into(),
            timestamp: OffsetDateTime::now_utc(),
            source_error: None,
            stacktrace: None,
            request_url: None,
            request_method: None,
            user_id: None,
            user_email: None,
            tags: None,
            extra: None,
            breadcrumbs: None,
            fingerprint: None,
        }
    }

    /// Add source error chain information.
    pub fn with_source_error(mut self, source: impl Into<String>) -> Self {
        self.source_error = Some(source.into());
        self
    }

    /// Add a stacktrace.
    pub fn with_stacktrace(mut self, stacktrace: impl Into<String>) -> Self {
        self.stacktrace = Some(stacktrace.into());
        self
    }

    /// Add request context.
    pub fn with_request(mut self, method: impl Into<String>, url: impl Into<String>) -> Self {
        self.request_method = Some(method.into());
        self.request_url = Some(url.into());
        self
    }

    /// Add user context.
    pub fn with_user(mut self, user_id: impl Into<String>) -> Self {
        self.user_id = Some(user_id.into());
        self
    }

    /// Add user context with email.
    pub fn with_user_email(mut self, user_id: impl Into<String>, email: impl Into<String>) -> Self {
        self.user_id = Some(user_id.into());
        self.user_email = Some(email.into());
        self
    }

    /// Add custom tags (indexed, searchable).
    pub fn with_tags(mut self, tags: serde_json::Value) -> Self {
        self.tags = Some(tags);
        self
    }

    /// Add extra context data (not indexed).
    pub fn with_extra(mut self, extra: serde_json::Value) -> Self {
        self.extra = Some(extra);
        self
    }

    /// Add breadcrumbs.
    pub fn with_breadcrumbs(mut self, breadcrumbs: serde_json::Value) -> Self {
        self.breadcrumbs = Some(breadcrumbs);
        self
    }

    /// Set a custom fingerprint for error grouping.
    pub fn with_fingerprint(mut self, fingerprint: impl Into<String>) -> Self {
        self.fingerprint = Some(fingerprint.into());
        self
    }

    /// Map error code to severity string for incident tracking systems.
    pub fn severity(&self) -> &'static str {
        match self.error_code {
            ErrorCode::Database => "critical",
            ErrorCode::Exception => "major",
            ErrorCode::Authentication | ErrorCode::Authorization => "minor",
            ErrorCode::BadRequest | ErrorCode::NotFound | ErrorCode::Validation => "warning",
        }
    }
}

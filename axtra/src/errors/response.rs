//! Response handling and conversion logic for AppError.

use axum::{
    Json,
    response::{Html, IntoResponse, Response},
};
use std::sync::OnceLock;

use crate::errors::{AppError, ErrorCode, ErrorFormat, ErrorResponse};

#[cfg(feature = "notifier")]
use crate::errors::notifiers::notification_manager;
#[cfg(feature = "notifier")]
use crate::notifier::ErrorEvent;

/// Cached error pages to avoid repeated file I/O.
static ERROR_PAGE_404: OnceLock<Option<String>> = OnceLock::new();
static ERROR_PAGE_500: OnceLock<Option<String>> = OnceLock::new();

/// Load and cache an error page from disk.
fn load_error_page(path: &str, cache: &'static OnceLock<Option<String>>) -> Option<&'static str> {
    cache
        .get_or_init(|| std::fs::read_to_string(path).ok())
        .as_deref()
}

fn escape_html_text(input: &str) -> String {
    input.chars().fold(
        String::with_capacity(input.len()),
        |mut output, character| {
            match character {
                '&' => output.push_str("&amp;"),
                '<' => output.push_str("&lt;"),
                '>' => output.push_str("&gt;"),
                '"' => output.push_str("&quot;"),
                '\'' => output.push_str("&#x27;"),
                _ => output.push(character),
            }
            output
        },
    )
}

impl AppError {
    /// Generates a formatted error message for logging and notifications.
    pub fn formatted_message(&self) -> String {
        let location = self.location();
        let error_code = self.code();
        let message = self.log_message();

        format!("[{location}][{error_code:?}] {message}")
    }

    /// Generates a detailed log message, recursively including sources.
    fn log_message(&self) -> String {
        fn proxy_source(
            source: &Option<Box<dyn std::error::Error + Send + Sync>>,
        ) -> Option<String> {
            source.as_ref().and_then(|src| {
                src.downcast_ref::<AppError>()
                    .map(|app_err| app_err.log_message())
                    .or_else(|| Some(format!("{src:?}")))
            })
        }

        match self {
            AppError::Authentication { .. } => "Authentication failed".to_string(),
            AppError::Authorization {
                resource, action, ..
            } => format!("'{action}' on '{resource}'"),
            AppError::BadRequest { detail, source, .. } => match proxy_source(source) {
                Some(msg) => format!("Bad Request: {detail} | caused by: {msg}"),
                None => detail.to_string(),
            },
            AppError::Database {
                message, source, ..
            } => format!("{message} | sqlx: {source:?}"),
            AppError::Exception { detail, source, .. } => match proxy_source(source) {
                Some(msg) => format!("{detail} | caused by: {msg}"),
                None => detail.to_string(),
            },
            AppError::NotFound { resource, .. } => {
                format!("Resource '{resource}'")
            }
            AppError::Validation { .. } => "Invalid payload".to_string(),
        }
    }

    /// Returns a user-friendly message for the error.
    fn user_message(&self) -> &str {
        match self {
            AppError::Authentication { .. } => {
                "Authentication is required to access this resource."
            }
            AppError::Authorization { .. } => "You are not authorized to perform this action.",
            AppError::BadRequest { detail, .. } => detail,
            AppError::Database { .. } => "A database error occurred.",
            AppError::Exception { .. } => "An internal server error occurred.",
            AppError::NotFound { .. } => "The requested resource was not found.",
            AppError::Validation { .. } => "There was a validation error with your request.",
        }
    }

    /// Returns the stable, developer-provided message without source error details.
    /// Used for error tracking fingerprinting where the message must be consistent
    /// across occurrences of the same logical error.
    #[cfg(feature = "notifier")]
    fn stable_message(&self) -> String {
        match self {
            AppError::Authentication { .. } => "Authentication failed".to_string(),
            AppError::Authorization {
                resource, action, ..
            } => format!("'{action}' on '{resource}'"),
            AppError::BadRequest { detail, .. } => detail.clone(),
            AppError::Database { message, .. } => message.clone(),
            AppError::Exception { detail, .. } => detail.clone(),
            AppError::NotFound { resource, .. } => format!("Resource '{resource}'"),
            AppError::Validation { .. } => "Invalid payload".to_string(),
        }
    }

    /// Send notification to all configured providers.
    #[cfg(feature = "notifier")]
    fn send_notification(&self) {
        let app_name = std::env::var("APP_NAME").unwrap_or_else(|_| "Rust".to_string());
        let error_code = self.code();
        let message = self.stable_message();
        let location = self.location().to_string();

        // Build the error event with stable message for consistent fingerprinting.
        // The varying source error chain is added separately below.
        let mut event = ErrorEvent::new(app_name, error_code, message, location);

        // Add source error chain if available
        let source_error = self.source_chain();
        if !source_error.is_empty() {
            event = event.with_source_error(source_error);
        }

        notification_manager().dispatch(event);
    }

    /// Get the error source chain as a string for capture providers.
    #[cfg(feature = "notifier")]
    fn source_chain(&self) -> String {
        use std::error::Error;

        let mut chain = Vec::new();
        let mut current: Option<&dyn Error> = Some(self);

        while let Some(err) = current {
            chain.push(format!("{err}"));
            current = err.source();
        }

        chain.join("\n  caused by: ")
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let status = self.status_code();
        let format = self.format();
        let error_code = self.code();
        let formatted_message = self.formatted_message();

        // Log the error
        match error_code {
            ErrorCode::Authentication | ErrorCode::Authorization => {
                tracing::info!("{formatted_message}");
            }
            ErrorCode::BadRequest | ErrorCode::NotFound | ErrorCode::Validation => {
                tracing::warn!("{formatted_message}");
            }
            ErrorCode::Database | ErrorCode::Exception => {
                tracing::error!("{formatted_message}");
                #[cfg(feature = "notifier")]
                self.send_notification();
            }
        }

        // Generate response
        match format {
            ErrorFormat::Json => {
                let error_response = ErrorResponse {
                    status: status.canonical_reason().unwrap_or("Unknown").to_string(),
                    message: self.user_message().to_string(),
                    code: self.code(),
                    validation_errors: match &self {
                        AppError::Validation { errors, .. } => Some(errors.clone().into()),
                        _ => None,
                    },
                };
                (status, Json(error_response)).into_response()
            }
            ErrorFormat::Html => {
                let cached_page = match error_code {
                    ErrorCode::NotFound => load_error_page("dist/404.html", &ERROR_PAGE_404),
                    _ => load_error_page("dist/500.html", &ERROR_PAGE_500),
                };

                let html_content = cached_page.map(String::from).unwrap_or_else(|| {
                    format!(
                        r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="utf-8">
    <title>Error</title>
</head>
<body>
    <h1>Error</h1>
    <p>{}</p>
</body>
</html>"#,
                        escape_html_text(self.user_message())
                    )
                });

                (status, Html(html_content)).into_response()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use axum::body::to_bytes;

    use super::*;

    #[tokio::test]
    async fn fallback_html_escapes_user_message() {
        let response = AppError::bad_request(
            "<strong>invalid & unsafe</strong>",
            None,
            "test",
            ErrorFormat::Html,
        )
        .into_response();
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let body = String::from_utf8(body.to_vec()).unwrap();

        assert!(body.contains("&lt;strong&gt;invalid &amp; unsafe&lt;/strong&gt;"));
        assert!(!body.contains("<strong>"));
    }
}

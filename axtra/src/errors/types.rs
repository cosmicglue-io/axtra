//! Error types and enums

use std::collections::HashMap;

use axum::extract::rejection::JsonRejection;
use http::StatusCode;
use serde::Serialize;
use thiserror::Error;
use ts_rs::TS;
use validator::{ValidationError, ValidationErrors, ValidationErrorsKind};

use crate::error_location;

// --- Core Enums ---

/// Supported error output formats.
#[derive(Clone, Debug, Serialize)]
pub enum ErrorFormat {
    Html,
    Json,
}

/// Enum of all possible error codes.
#[derive(Debug, Serialize, TS, Clone, Copy)]
#[ts(export, export_to = "errors.ts")]
#[serde(rename_all = "camelCase")]
pub enum ErrorCode {
    Authentication,
    Authorization,
    BadRequest,
    Database,
    Exception,
    NotFound,
    Validation,
}

// --- Validation Errors ---

/// Represents a single field validation error.
#[derive(Debug, Serialize, TS)]
#[ts(export, export_to = "errors.ts")]
#[serde(rename_all = "camelCase")]
pub struct ValidationFieldError {
    pub field: String,
    pub code: String,
    pub message: String,
    pub params: HashMap<String, String>,
}

/// Represents all validation errors in a serializable form.
#[derive(Debug, Serialize, TS)]
#[ts(export, export_to = "errors.ts")]
pub struct SerializableValidationErrors {
    pub errors: Vec<ValidationFieldError>,
}

/// Convert `ValidationErrors` to `SerializableValidationErrors` for serialization
impl From<ValidationErrors> for SerializableValidationErrors {
    fn from(errors: ValidationErrors) -> Self {
        let mut field_errors = Vec::new();
        collect_validation_errors(&errors, "", &mut field_errors);
        field_errors.sort_by(|left, right| {
            left.field
                .cmp(&right.field)
                .then_with(|| left.code.cmp(&right.code))
        });
        SerializableValidationErrors {
            errors: field_errors,
        }
    }
}

fn collect_validation_errors(
    errors: &ValidationErrors,
    prefix: &str,
    output: &mut Vec<ValidationFieldError>,
) {
    for (field, kind) in errors.errors() {
        let path = if prefix.is_empty() {
            field.to_string()
        } else {
            format!("{prefix}.{field}")
        };

        match kind {
            ValidationErrorsKind::Field(field_errors) => {
                output.extend(
                    field_errors
                        .iter()
                        .map(|error| serialize_validation_error(&path, error)),
                );
            }
            ValidationErrorsKind::Struct(nested) => {
                collect_validation_errors(nested, &path, output);
            }
            ValidationErrorsKind::List(items) => {
                for (index, nested) in items {
                    collect_validation_errors(nested, &format!("{path}[{index}]"), output);
                }
            }
        }
    }
}

fn serialize_validation_error(field: &str, error: &ValidationError) -> ValidationFieldError {
    ValidationFieldError {
        field: field.to_string(),
        code: error.code.to_string(),
        message: error
            .message
            .as_ref()
            .map(ToString::to_string)
            .unwrap_or_else(|| format!("Validation failed for {field}")),
        params: error
            .params
            .iter()
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect(),
    }
}

// --- Core AppError ---

/// Unified error type for Axtra APIs.
///
/// Use [`app_error!`] macro to construct errors ergonomically.
/// See crate-level docs for usage patterns.
#[derive(Debug, Error)]
pub enum AppError {
    #[error("Bad Request: {detail}")]
    BadRequest {
        detail: String,
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
        location: String,
        format: ErrorFormat,
    },
    #[error("Database error: {message}")]
    Database {
        message: String,
        #[source]
        source: Box<sqlx::Error>,
        location: String,
        format: ErrorFormat,
    },
    #[error("Exception: {detail}")]
    Exception {
        detail: String,
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
        location: String,
        format: ErrorFormat,
    },
    #[error("Not Found: {resource}")]
    NotFound {
        resource: String,
        location: String,
        format: ErrorFormat,
    },
    #[error("Unauthorized: {resource} {action}")]
    Authorization {
        resource: String,
        action: String,
        location: String,
        format: ErrorFormat,
    },
    #[error("Authentication required")]
    Authentication {
        location: String,
        format: ErrorFormat,
    },
    #[error("Validation error")]
    Validation {
        errors: ValidationErrors,
        location: String,
        format: ErrorFormat,
    },
}

impl AppError {
    /// Create a BadRequest error.
    pub fn bad_request(
        detail: impl AsRef<str>,
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
        location: impl AsRef<str>,
        format: ErrorFormat,
    ) -> Self {
        Self::BadRequest {
            detail: detail.as_ref().to_string(),
            source,
            location: location.as_ref().to_string(),
            format,
        }
    }

    /// Create a Database error.
    pub fn database(
        message: impl AsRef<str>,
        source: sqlx::Error,
        location: impl AsRef<str>,
        format: ErrorFormat,
    ) -> Self {
        Self::Database {
            message: message.as_ref().to_string(),
            source: Box::new(source),
            location: location.as_ref().to_string(),
            format,
        }
    }

    /// Create an Exception error.
    pub fn exception(
        detail: impl AsRef<str>,
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
        location: impl AsRef<str>,
        format: ErrorFormat,
    ) -> Self {
        Self::Exception {
            detail: detail.as_ref().to_string(),
            source,
            location: location.as_ref().to_string(),
            format,
        }
    }

    /// Create a NotFound error.
    pub fn not_found(
        resource: impl AsRef<str>,
        location: impl AsRef<str>,
        format: ErrorFormat,
    ) -> Self {
        Self::NotFound {
            resource: resource.as_ref().to_string(),
            location: location.as_ref().to_string(),
            format,
        }
    }

    /// Create an Unauthenticated error.
    pub fn unauthenticated(location: impl AsRef<str>, format: ErrorFormat) -> Self {
        Self::Authentication {
            location: location.as_ref().to_string(),
            format,
        }
    }

    /// Create an Unauthorized error.
    pub fn unauthorized(
        resource: impl AsRef<str>,
        action: impl AsRef<str>,
        location: impl AsRef<str>,
        format: ErrorFormat,
    ) -> Self {
        Self::Authorization {
            resource: resource.as_ref().to_string(),
            action: action.as_ref().to_string(),
            location: location.as_ref().to_string(),
            format,
        }
    }

    /// Create a Validation error.
    pub fn validation(
        errors: ValidationErrors,
        location: impl AsRef<str>,
        format: ErrorFormat,
    ) -> Self {
        Self::Validation {
            errors,
            location: location.as_ref().to_string(),
            format,
        }
    }
}

#[cfg(test)]
mod validation_tests {
    use std::{borrow::Cow, collections::BTreeMap};

    use super::*;

    fn error(code: &'static str) -> ValidationError {
        ValidationError::new(code)
    }

    #[test]
    fn serializes_nested_struct_and_list_errors() {
        let mut address = ValidationErrors::new();
        address.add("city", error("required"));

        let mut first_item = ValidationErrors::new();
        first_item.add("sku", error("invalid"));
        let items = BTreeMap::from([(0, Box::new(first_item))]);

        let errors = ValidationErrors(std::collections::HashMap::from([
            (
                Cow::Borrowed("address"),
                ValidationErrorsKind::Struct(Box::new(address)),
            ),
            (Cow::Borrowed("items"), ValidationErrorsKind::List(items)),
        ]));

        let serialized = SerializableValidationErrors::from(errors);
        let fields: Vec<_> = serialized
            .errors
            .iter()
            .map(|error| error.field.as_str())
            .collect();

        assert_eq!(fields, ["address.city", "items[0].sku"]);
    }
}

impl AppError {
    /// Convert the error to a generic error code.
    pub fn code(&self) -> ErrorCode {
        match self {
            Self::BadRequest { .. } => ErrorCode::BadRequest,
            Self::Database { .. } => ErrorCode::Database,
            Self::Exception { .. } => ErrorCode::Exception,
            Self::NotFound { .. } => ErrorCode::NotFound,
            Self::Authorization { .. } => ErrorCode::Authorization,
            Self::Authentication { .. } => ErrorCode::Authentication,
            Self::Validation { .. } => ErrorCode::Validation,
        }
    }

    /// Returns the format from any variant.
    pub fn format(&self) -> &ErrorFormat {
        match self {
            AppError::BadRequest { format, .. } => format,
            AppError::Database { format, .. } => format,
            AppError::Exception { format, .. } => format,
            AppError::NotFound { format, .. } => format,
            AppError::Authorization { format, .. } => format,
            AppError::Authentication { format, .. } => format,
            AppError::Validation { format, .. } => format,
        }
    }

    /// Returns the location from any variant.
    pub fn location(&self) -> &str {
        match self {
            AppError::BadRequest { location, .. } => location,
            AppError::Database { location, .. } => location,
            AppError::Exception { location, .. } => location,
            AppError::NotFound { location, .. } => location,
            AppError::Authorization { location, .. } => location,
            AppError::Authentication { location, .. } => location,
            AppError::Validation { location, .. } => location,
        }
    }

    /// Returns the HTTP status code for the error.
    pub fn status_code(&self) -> StatusCode {
        match self {
            AppError::Authentication { .. } => StatusCode::UNAUTHORIZED,
            AppError::Authorization { .. } => StatusCode::FORBIDDEN,
            AppError::BadRequest { .. } | AppError::Validation { .. } => StatusCode::BAD_REQUEST,
            AppError::Database { .. } | AppError::Exception { .. } => {
                StatusCode::INTERNAL_SERVER_ERROR
            }
            AppError::NotFound { .. } => StatusCode::NOT_FOUND,
        }
    }
}

// --- Error Conversion Implementations ---

/// Converts Axum JSON rejections into AppError.
impl From<JsonRejection> for AppError {
    fn from(err: JsonRejection) -> Self {
        AppError::bad_request(
            "Invalid JSON request",
            Some(Box::new(err)),
            error_location!(),
            ErrorFormat::Json,
        )
    }
}

/// Converts validator errors into AppError.
impl From<ValidationErrors> for AppError {
    fn from(err: ValidationErrors) -> Self {
        AppError::validation(err, error_location!(), ErrorFormat::Json)
    }
}

// --- API Response ---

#[derive(Debug, Serialize, TS)]
#[ts(export, export_to = "errors.ts")]
#[serde(rename_all = "camelCase")]
pub struct ErrorResponse {
    pub status: String,
    pub message: String,
    pub code: ErrorCode,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub validation_errors: Option<SerializableValidationErrors>,
}

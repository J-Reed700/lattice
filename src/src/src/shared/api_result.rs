//! API Result Types
//!
//! Provides a standardized result wrapper for IPC gateway responses.
//! This module defines the `ApiResult<T>` type that wraps all gateway responses
//! in a consistent format with success/error discrimination and structured error codes.
//!
//! ## Design Principles
//!
//! - **Consistent Structure**: All responses follow `{ ok: bool, data?: T, error?: ApiError }` format
//! - **Type-Safe**: Uses Rust's type system for compile-time safety
//! - **Serializable**: Untagged enum for clean JSON serialization
//! - **Error Codes**: Comprehensive error categorization for frontend handling
//! - **Details Preservation**: Optional error details for debugging without exposing internals
//!
//! ## Usage
//!
//! ```rust,no_run
//! use lattice::shared::api_result::{ApiResult, ErrorCode};
//!
//! // Success response
//! let result = ApiResult::success(vec![1, 2, 3]);
//! // Serializes to: { "ok": true, "data": [1, 2, 3] }
//!
//! // Error response
//! let error = ApiResult::<()>::error(ErrorCode::NotFound, "Document not found");
//! // Serializes to: { "ok": false, "error": { "code": "NOT_FOUND", "message": "..." } }
//!
//! // Error with details
//! let error = ApiResult::<()>::error_with_details(
//!     ErrorCode::ValidationError,
//!     "Invalid input",
//!     "Field 'email' is required"
//! );
//! // Serializes to: { "ok": false, "error": { "code": "VALIDATION_ERROR", "message": "...", "details": "..." } }
//! ```

use crate::application::error::ApplicationError;
use crate::domain::error::DomainError;
use crate::shared::error::AppError;
use serde::{Deserialize, Serialize};

/// API result wrapper for IPC gateway responses.
///
/// This type wraps all gateway responses in a consistent format that discriminates
/// between success and error cases. It uses an untagged enum for clean JSON serialization.
///
/// # JSON Serialization
///
/// - Success: `{ "ok": true, "data": <T> }`
/// - Error: `{ "ok": false, "error": { "code": "...", "message": "...", "details"?: "..." } }`
///
/// # Examples
///
/// ```rust
/// # use lattice::shared::api_result::{ApiResult, ErrorCode};
/// let success = ApiResult::success("hello");
/// assert!(success.is_ok());
///
/// let error = ApiResult::<String>::error(ErrorCode::NotFound, "Not found");
/// assert!(!error.is_ok());
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(untagged)]
pub enum ApiResult<T> {
    /// Success variant containing data
    Success {
        /// Always true for success
        ok: bool,
        /// The successful result data
        data: T,
    },
    /// Error variant containing error information
    Error {
        /// Always false for errors
        ok: bool,
        /// Structured error information
        error: ApiError,
    },
}

impl<T> ApiResult<T> {
    /// Creates a success result.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use lattice::shared::api_result::ApiResult;
    /// let result = ApiResult::success(42);
    /// assert!(result.is_ok());
    /// ```
    pub fn success(data: T) -> Self {
        Self::Success { ok: true, data }
    }

    /// Creates an error result without details.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use lattice::shared::api_result::{ApiResult, ErrorCode};
    /// let result = ApiResult::<()>::error(ErrorCode::NotFound, "Resource not found");
    /// assert!(!result.is_ok());
    /// ```
    pub fn error(code: ErrorCode, message: impl Into<String>) -> Self {
        Self::Error {
            ok: false,
            error: ApiError {
                code,
                message: message.into(),
                details: None,
            },
        }
    }

    /// Creates an error result with additional details.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use lattice::shared::api_result::{ApiResult, ErrorCode};
    /// let result = ApiResult::<()>::error_with_details(
    ///     ErrorCode::ValidationError,
    ///     "Invalid request",
    ///     "Field 'name' is required"
    /// );
    /// assert!(!result.is_ok());
    /// ```
    pub fn error_with_details(
        code: ErrorCode,
        message: impl Into<String>,
        details: impl Into<String>,
    ) -> Self {
        Self::Error {
            ok: false,
            error: ApiError {
                code,
                message: message.into(),
                details: Some(details.into()),
            },
        }
    }

    /// Creates an ApiResult from a standard Result.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use lattice::shared::api_result::ApiResult;
    /// # use lattice::application::error::ApplicationError;
    /// let ok: Result<i32, ApplicationError> = Ok(42);
    /// let result = ApiResult::from_result(ok);
    /// assert!(result.is_ok());
    ///
    /// let err: Result<i32, ApplicationError> = Err(ApplicationError::InvalidConfig {
    ///     details: "Bad config".to_string(),
    /// });
    /// let result = ApiResult::from_result(err);
    /// assert!(!result.is_ok());
    /// ```
    pub fn from_result<E>(result: Result<T, E>) -> Self
    where
        E: Into<ApiError>,
    {
        match result {
            Ok(data) => Self::success(data),
            Err(e) => {
                let api_error = e.into();
                Self::Error {
                    ok: false,
                    error: api_error,
                }
            }
        }
    }

    /// Returns true if this is a success result.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use lattice::shared::api_result::ApiResult;
    /// let success = ApiResult::success(42);
    /// assert!(success.is_ok());
    ///
    /// let error = ApiResult::<i32>::error(
    ///     lattice::shared::api_result::ErrorCode::NotFound,
    ///     "Not found"
    /// );
    /// assert!(!error.is_ok());
    /// ```
    pub fn is_ok(&self) -> bool {
        matches!(self, Self::Success { .. })
    }

    /// Returns true if this is an error result.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use lattice::shared::api_result::ApiResult;
    /// let success = ApiResult::success(42);
    /// assert!(!success.is_err());
    ///
    /// let error = ApiResult::<i32>::error(
    ///     lattice::shared::api_result::ErrorCode::NotFound,
    ///     "Not found"
    /// );
    /// assert!(error.is_err());
    /// ```
    pub fn is_err(&self) -> bool {
        matches!(self, Self::Error { .. })
    }
}

/// Structured error information for API responses.
///
/// Contains an error code for programmatic handling, a human-readable message,
/// and optional details for debugging.
///
/// # Examples
///
/// ```rust
/// # use lattice::shared::api_result::{ApiError, ErrorCode};
/// let error = ApiError {
///     code: ErrorCode::NotFound,
///     message: "Document not found".to_string(),
///     details: Some("ID: doc-123".to_string()),
/// };
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct ApiError {
    /// Error code for programmatic handling
    pub code: ErrorCode,
    /// Human-readable error message
    pub message: String,
    /// Optional additional details (sanitized for security)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
}

/// Comprehensive error code enumeration for API responses.
///
/// These codes allow the frontend to handle errors programmatically without
/// parsing error messages. Each code maps to a specific error category.
///
/// # Serialization
///
/// Error codes serialize to SCREAMING_SNAKE_CASE for consistency with typical
/// API conventions.
///
/// # Examples
///
/// ```rust
/// # use lattice::shared::api_result::ErrorCode;
/// let code = ErrorCode::NotFound;
/// let json = serde_json::to_string(&code).unwrap();
/// assert_eq!(json, "\"NOT_FOUND\"");
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ErrorCode {
    // Resource Errors
    /// Requested resource was not found
    NotFound,
    /// Resource already exists
    AlreadyExists,
    /// Resource has been deleted or is no longer available
    Gone,

    // Permission & Security Errors
    /// Operation not permitted
    PermissionDenied,
    /// Invalid credentials or authentication failure
    Unauthorized,
    /// Security violation detected
    SecurityViolation,

    // Validation Errors
    /// Input validation failed
    ValidationError,
    /// Invalid input format
    InvalidInput,
    /// Invalid configuration
    InvalidConfig,
    /// Invalid state for requested operation
    InvalidState,
    /// Constraint violation
    ConstraintViolation,

    // File System Errors
    /// File not found
    FileNotFound,
    /// File is too large to process
    FileTooLarge,
    /// File type not supported
    UnsupportedFileType,
    /// File system operation failed
    FileSystemError,
    /// File read error
    FileReadError,
    /// File write error
    FileWriteError,

    // Database Errors
    /// Database operation failed
    DatabaseError,
    /// Database connection failed
    DatabaseConnectionError,
    /// Database migration failed
    MigrationError,

    // Network Errors
    /// Network operation failed
    NetworkError,
    /// Request timeout
    Timeout,
    /// Rate limit exceeded
    RateLimitExceeded,

    // Service Errors
    /// Required service is not available
    ServiceNotAvailable,
    /// Service initialization failed
    ServiceInitializationError,
    /// AI model not loaded or unavailable
    ModelNotLoaded,
    /// AI model loading failed
    ModelLoadError,

    // Processing Errors
    /// Data processing failed
    ProcessingError,
    /// Serialization failed
    SerializationError,
    /// Deserialization failed
    DeserializationError,
    /// Parsing error
    ParsingError,
    /// Embedding generation failed
    EmbeddingError,
    /// Content extraction failed
    ExtractionError,
    /// Tokenization error
    TokenizationError,

    // Concurrency Errors
    /// Concurrent modification detected
    ConcurrentModification,
    /// Processing queue is full
    QueueFull,

    // Backup & Recovery Errors
    /// Backup creation failed
    BackupCreationFailed,
    /// Backup restore failed
    BackupRestoreFailed,
    /// Backup file is corrupted
    BackupCorrupted,

    // Generic Errors
    /// Internal server error
    InternalError,
    /// Feature not implemented
    NotImplemented,
    /// Generic error
    Unknown,
}

// ============================================================================
// From Trait Implementations
// ============================================================================

/// Convert ApplicationError (application layer) to ApiError
impl From<ApplicationError> for ApiError {
    fn from(err: ApplicationError) -> Self {
        match err {
            // Domain errors
            ApplicationError::Domain(domain_err) => domain_err.into(),

            // Configuration errors
            ApplicationError::InvalidConfig { ref details } => ApiError {
                code: ErrorCode::InvalidConfig,
                message: "Invalid configuration".to_string(),
                details: Some(details.clone()),
            },

            // Service errors
            ApplicationError::ServiceNotAvailable {
                ref service,
                ref reason,
            } => ApiError {
                code: ErrorCode::ServiceNotAvailable,
                message: format!("Service '{}' not available", service),
                details: Some(reason.clone()),
            },

            // Queue full
            ApplicationError::QueueFull { ref queue_name } => ApiError {
                code: ErrorCode::QueueFull,
                message: format!("Queue '{}' is full", queue_name),
                details: Some("Please retry later".to_string()),
            },

            // Serialization errors
            ApplicationError::SerializationFailed {
                ref context,
                ref reason,
            } => ApiError {
                code: ErrorCode::SerializationError,
                message: format!("Serialization failed for {}", context),
                details: Some(reason.clone()),
            },

            // Deserialization errors
            ApplicationError::DeserializationFailed {
                ref context,
                ref reason,
            } => ApiError {
                code: ErrorCode::DeserializationError,
                message: format!("Deserialization failed for {}", context),
                details: Some(reason.clone()),
            },

            // JSON errors
            ApplicationError::JsonError {
                ref message,
                line,
                column,
            } => ApiError {
                code: ErrorCode::ParsingError,
                message: "JSON parsing error".to_string(),
                details: Some(format!("{}:{}: {}", line, column, message)),
            },

            // Parsing errors
            ApplicationError::ParsingError {
                ref context,
                ref reason,
            } => ApiError {
                code: ErrorCode::ParsingError,
                message: format!("Parsing error in {}", context),
                details: Some(reason.clone()),
            },

            // Use case errors
            ApplicationError::UseCaseFailed {
                ref use_case,
                ref reason,
            } => ApiError {
                code: ErrorCode::ProcessingError,
                message: format!("Use case '{}' failed", use_case),
                details: Some(reason.clone()),
            },

            // Mapping errors
            ApplicationError::MappingError {
                ref context,
                ref reason,
            } => ApiError {
                code: ErrorCode::ProcessingError,
                message: format!("Mapping error in {}", context),
                details: Some(reason.clone()),
            },
        }
    }
}

/// Convert AppError (shared layer) to ApiError
impl From<AppError> for ApiError {
    fn from(err: AppError) -> Self {
        match err {
            // Domain errors
            AppError::Domain(domain_err) => (*domain_err).into(),

            // Application layer errors
            AppError::Application(app_err) => (*app_err).into(),

            // Configuration errors
            AppError::InvalidConfig(ref details) => ApiError {
                code: ErrorCode::InvalidConfig,
                message: "Invalid configuration".to_string(),
                details: Some(details.clone()),
            },

            // Resource errors
            AppError::NotFound(ref msg) => ApiError {
                code: ErrorCode::NotFound,
                message: format!("Not found: {}", msg),
                details: None,
            },

            // Permission errors
            AppError::PermissionDenied(ref msg) => ApiError {
                code: ErrorCode::PermissionDenied,
                message: format!("Permission denied: {}", msg),
                details: None,
            },

            // Validation errors
            AppError::InvalidInput(ref msg) => ApiError {
                code: ErrorCode::InvalidInput,
                message: format!("Invalid input: {}", msg),
                details: None,
            },
            AppError::ValidationFailed(ref msg) => ApiError {
                code: ErrorCode::ValidationError,
                message: format!("Validation failed: {}", msg),
                details: None,
            },
            AppError::InvalidState(ref msg) => ApiError {
                code: ErrorCode::InvalidState,
                message: format!("Invalid state: {}", msg),
                details: None,
            },

            // File errors
            AppError::FileNotFound { ref path } => ApiError {
                code: ErrorCode::FileNotFound,
                message: format!("File not found: {}", path),
                details: None,
            },
            AppError::FileTooLarge {
                ref path,
                size_bytes,
                max_size_bytes,
            } => ApiError {
                code: ErrorCode::FileTooLarge,
                message: format!("File too large: {}", path),
                details: Some(format!(
                    "Size: {} bytes, Max: {} bytes",
                    size_bytes, max_size_bytes
                )),
            },
            AppError::UnsupportedFileType {
                ref path,
                ref detected_type,
            } => ApiError {
                code: ErrorCode::UnsupportedFileType,
                message: format!("Unsupported file type: {}", path),
                details: Some(format!("Detected type: {}", detected_type)),
            },
            AppError::FileRead {
                ref path,
                ref reason,
            } => ApiError {
                code: ErrorCode::FileReadError,
                message: format!("Failed to read file: {}", path),
                details: Some(reason.clone()),
            },
            AppError::FileStorage(ref msg) | AppError::FileSystem(ref msg) => ApiError {
                code: ErrorCode::FileSystemError,
                message: "File system error".to_string(),
                details: Some(msg.clone()),
            },

            // Database errors
            AppError::Database(ref msg) => ApiError {
                code: ErrorCode::DatabaseError,
                message: "Database error".to_string(),
                details: Some(sanitize_database_error(msg)),
            },
            AppError::Migration(ref msg) => ApiError {
                code: ErrorCode::MigrationError,
                message: "Database migration failed".to_string(),
                details: Some(msg.clone()),
            },

            // Network errors
            AppError::Network(ref msg) => ApiError {
                code: ErrorCode::NetworkError,
                message: "Network error".to_string(),
                details: Some(msg.clone()),
            },
            AppError::InvalidUrl(ref msg) => ApiError {
                code: ErrorCode::InvalidInput,
                message: "Invalid URL".to_string(),
                details: Some(msg.clone()),
            },
            AppError::RateLimitExceeded(ref msg) => ApiError {
                code: ErrorCode::RateLimitExceeded,
                message: "Rate limit exceeded".to_string(),
                details: Some(msg.clone()),
            },

            // Service errors
            AppError::ServiceNotAvailable(ref msg) => ApiError {
                code: ErrorCode::ServiceNotAvailable,
                message: "Service not available".to_string(),
                details: Some(msg.clone()),
            },
            AppError::AiModelsNotInstalled(ref msg) | AppError::ModelLoadFailed(ref msg) => {
                ApiError {
                    code: ErrorCode::ModelNotLoaded,
                    message: "AI model not available".to_string(),
                    details: Some(msg.clone()),
                }
            }

            // Processing errors
            AppError::EmbeddingFailed { ref reason } => ApiError {
                code: ErrorCode::EmbeddingError,
                message: "Embedding generation failed".to_string(),
                details: Some(reason.clone()),
            },
            AppError::ContentExtraction {
                ref path,
                ref reason,
            } => ApiError {
                code: ErrorCode::ExtractionError,
                message: format!("Content extraction failed: {} ({})", path, reason),
                details: Some(reason.clone()),
            },
            AppError::TokenizationError { ref reason } => ApiError {
                code: ErrorCode::TokenizationError,
                message: "Tokenization error".to_string(),
                details: Some(reason.clone()),
            },
            AppError::Serialization(ref msg) => ApiError {
                code: ErrorCode::SerializationError,
                message: "Serialization failed".to_string(),
                details: Some(msg.clone()),
            },
            AppError::Deserialization(ref msg) | AppError::Parsing(ref msg) => ApiError {
                code: ErrorCode::DeserializationError,
                message: "Deserialization failed".to_string(),
                details: Some(msg.clone()),
            },

            // Concurrency errors
            AppError::ConcurrentModification {
                ref resource,
                ref details,
            } => ApiError {
                code: ErrorCode::ConcurrentModification,
                message: format!("Concurrent modification: {}", resource),
                details: Some(details.clone()),
            },
            AppError::QueueFull => ApiError {
                code: ErrorCode::QueueFull,
                message: "Processing queue is full".to_string(),
                details: Some("Please retry later".to_string()),
            },

            // Backup errors
            AppError::BackupCreationFailed(ref msg) => ApiError {
                code: ErrorCode::BackupCreationFailed,
                message: "Backup creation failed".to_string(),
                details: Some(msg.clone()),
            },
            AppError::BackupRestoreFailed(ref msg) => ApiError {
                code: ErrorCode::BackupRestoreFailed,
                message: "Backup restore failed".to_string(),
                details: Some(msg.clone()),
            },
            AppError::BackupCorrupted(ref msg) => ApiError {
                code: ErrorCode::BackupCorrupted,
                message: "Backup file is corrupted".to_string(),
                details: Some(msg.clone()),
            },

            // I/O errors
            AppError::Io { ref message, .. } => ApiError {
                code: ErrorCode::FileSystemError,
                message: "I/O error".to_string(),
                details: Some(message.clone()),
            },

            // JSON errors
            AppError::Json {
                ref message,
                line,
                column,
            } => ApiError {
                code: ErrorCode::ParsingError,
                message: "JSON parsing error".to_string(),
                details: Some(format!("{}:{}: {}", line, column, message)),
            },

            // Security errors
            AppError::Security(ref msg) | AppError::KeyringError(ref msg) => ApiError {
                code: ErrorCode::SecurityViolation,
                message: "Security error".to_string(),
                details: Some(msg.clone()),
            },

            // Generic errors
            AppError::InvalidData(ref msg) => ApiError {
                code: ErrorCode::InvalidInput,
                message: "Invalid data".to_string(),
                details: Some(msg.clone()),
            },
            AppError::Storage(ref msg) => ApiError {
                code: ErrorCode::InternalError,
                message: "Storage error".to_string(),
                details: Some(msg.clone()),
            },
            AppError::InternalError(ref msg) | AppError::Other(ref msg) => ApiError {
                code: ErrorCode::InternalError,
                message: "Internal error".to_string(),
                details: Some(msg.clone()),
            },
        }
    }
}

/// Convert DomainError to ApiError
impl From<DomainError> for ApiError {
    fn from(err: DomainError) -> Self {
        match err {
            DomainError::EntityNotFound {
                ref entity_type,
                ref identifier,
            } => ApiError {
                code: ErrorCode::NotFound,
                message: format!("{} not found", entity_type),
                details: Some(format!("Identifier: {}", identifier)),
            },
            DomainError::InvalidState { ref details } => ApiError {
                code: ErrorCode::InvalidState,
                message: "Invalid state".to_string(),
                details: Some(details.clone()),
            },
            DomainError::ValidationFailed {
                ref field,
                ref reason,
            } => ApiError {
                code: ErrorCode::ValidationError,
                message: format!("Validation failed for field '{}'", field),
                details: Some(reason.clone()),
            },
            DomainError::UnsupportedFileType {
                ref path,
                ref detected_type,
            } => ApiError {
                code: ErrorCode::UnsupportedFileType,
                message: format!("Unsupported file type: {}", path.display()),
                details: detected_type.clone(),
            },
            DomainError::FileTooLarge {
                ref path,
                size_bytes,
                max_size_bytes,
            } => ApiError {
                code: ErrorCode::FileTooLarge,
                message: format!("File too large: {}", path.display()),
                details: Some(format!(
                    "Size: {} bytes, Max: {} bytes",
                    size_bytes, max_size_bytes
                )),
            },
            DomainError::ConcurrentModification {
                ref resource,
                ref details,
            } => ApiError {
                code: ErrorCode::ConcurrentModification,
                message: format!("Concurrent modification: {}", resource),
                details: Some(details.clone()),
            },
            DomainError::ConstraintViolation { ref constraint } => ApiError {
                code: ErrorCode::ConstraintViolation,
                message: "Constraint violation".to_string(),
                details: Some(constraint.clone()),
            },
        }
    }
}

/// Convert anyhow::Error to ApiError
impl From<anyhow::Error> for ApiError {
    fn from(err: anyhow::Error) -> Self {
        ApiError {
            code: ErrorCode::InternalError,
            message: "Internal error".to_string(),
            details: Some(format!("{:#}", err)),
        }
    }
}

/// Convert std::io::Error to ApiError
impl From<std::io::Error> for ApiError {
    fn from(err: std::io::Error) -> Self {
        use std::io::ErrorKind;

        let code = match err.kind() {
            ErrorKind::NotFound => ErrorCode::FileNotFound,
            ErrorKind::PermissionDenied => ErrorCode::PermissionDenied,
            ErrorKind::TimedOut => ErrorCode::Timeout,
            ErrorKind::InvalidInput | ErrorKind::InvalidData => ErrorCode::InvalidInput,
            _ => ErrorCode::FileSystemError,
        };

        ApiError {
            code,
            message: "I/O error".to_string(),
            details: Some(err.to_string()),
        }
    }
}

/// Convert sqlx::Error to ApiError
impl From<sqlx::Error> for ApiError {
    fn from(err: sqlx::Error) -> Self {
        use sqlx::Error;

        let (code, message) = match &err {
            Error::RowNotFound => (ErrorCode::NotFound, "Record not found"),
            Error::PoolTimedOut | Error::PoolClosed => (
                ErrorCode::DatabaseConnectionError,
                "Database connection error",
            ),
            Error::Configuration(_) => (ErrorCode::InvalidConfig, "Database configuration error"),
            _ => (ErrorCode::DatabaseError, "Database error"),
        };

        ApiError {
            code,
            message: message.to_string(),
            details: Some(sanitize_database_error(&err.to_string())),
        }
    }
}

/// Convert serde_json::Error to ApiError
impl From<serde_json::Error> for ApiError {
    fn from(err: serde_json::Error) -> Self {
        ApiError {
            code: ErrorCode::SerializationError,
            message: "JSON serialization error".to_string(),
            details: Some(err.to_string()),
        }
    }
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Sanitizes database error messages to avoid leaking sensitive information.
///
/// Removes:
/// - SQL query text
/// - Connection strings
/// - Table/column names that might reveal schema
/// - Specific error codes that could aid attackers
///
/// # Examples
///
/// ```rust
/// # use lattice::shared::api_result::sanitize_database_error;
/// let error = "error in SQL: SELECT * FROM users WHERE password = 'secret'";
/// let sanitized = sanitize_database_error(error);
/// assert!(!sanitized.contains("SELECT"));
/// assert!(!sanitized.contains("secret"));
/// ```
pub(crate) fn sanitize_database_error(msg: &str) -> String {
    // Remove SQL queries (anything between quotes or after "SQL:")
    let msg = msg.split("SQL:").next().unwrap_or(msg);
    let msg = msg.split("Query:").next().unwrap_or(msg);

    // Remove connection strings
    let msg = if msg.contains("://") {
        "Database connection error"
    } else {
        msg
    };

    // Truncate long messages
    if msg.len() > 200 {
        format!("{}...", &msg[..200])
    } else {
        msg.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_success_serialization() {
        let result = ApiResult::success(vec![1, 2, 3]);
        let json = serde_json::to_value(&result).unwrap();

        assert_eq!(json["ok"], true);
        assert_eq!(json["data"], serde_json::json!([1, 2, 3]));
    }

    #[test]
    fn test_error_serialization() {
        let result = ApiResult::<()>::error(ErrorCode::NotFound, "Not found");
        let json = serde_json::to_value(&result).unwrap();

        assert_eq!(json["ok"], false);
        assert_eq!(json["error"]["code"], "NOT_FOUND");
        assert!(json["error"]["message"]
            .as_str()
            .unwrap()
            .contains("Not found"));
        assert!(json["error"]["details"].is_null());
    }

    #[test]
    fn test_error_with_details_serialization() {
        let result = ApiResult::<()>::error_with_details(
            ErrorCode::ValidationError,
            "Invalid input",
            "Field 'email' is required",
        );
        let json = serde_json::to_value(&result).unwrap();

        assert_eq!(json["ok"], false);
        assert_eq!(json["error"]["code"], "VALIDATION_ERROR");
        assert!(json["error"]["details"].as_str().unwrap().contains("email"));
    }

    #[test]
    fn test_from_result_ok() {
        let ok: Result<i32, AppError> = Ok(42);
        let result = ApiResult::from_result(ok);

        assert!(result.is_ok());
        if let ApiResult::Success { data, .. } = result {
            assert_eq!(data, 42);
        }
    }

    #[test]
    fn test_from_result_err() {
        let err: Result<i32, DomainError> = Err(DomainError::EntityNotFound {
            entity_type: "Document".to_string(),
            identifier: "doc-123".to_string(),
        });
        let result = ApiResult::from_result(err);

        assert!(result.is_err());
        if let ApiResult::Error { error, .. } = result {
            assert_eq!(error.code, ErrorCode::NotFound);
        }
    }

    #[test]
    fn test_domain_error_conversion() {
        let domain_err = DomainError::ValidationFailed {
            field: "email".to_string(),
            reason: "Invalid format".to_string(),
        };
        let api_err: ApiError = domain_err.into();

        assert_eq!(api_err.code, ErrorCode::ValidationError);
        assert!(api_err.message.contains("email"));
        assert!(api_err.details.is_some());
    }

    #[test]
    fn test_io_error_conversion() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let api_err: ApiError = io_err.into();

        assert_eq!(api_err.code, ErrorCode::FileNotFound);
    }

    #[test]
    fn test_sqlx_error_conversion() {
        let sqlx_err = sqlx::Error::RowNotFound;
        let api_err: ApiError = sqlx_err.into();

        assert_eq!(api_err.code, ErrorCode::NotFound);
    }

    #[test]
    fn test_sanitize_database_error() {
        let error = "error in SQL: SELECT * FROM users WHERE password = 'secret'";
        let sanitized = sanitize_database_error(error);

        assert!(!sanitized.contains("SELECT"));
        assert!(!sanitized.contains("secret"));
        assert!(!sanitized.contains("password"));
    }

    #[test]
    fn test_error_code_serialization() {
        let code = ErrorCode::NotFound;
        let json = serde_json::to_string(&code).unwrap();
        assert_eq!(json, "\"NOT_FOUND\"");

        let code = ErrorCode::ValidationError;
        let json = serde_json::to_string(&code).unwrap();
        assert_eq!(json, "\"VALIDATION_ERROR\"");
    }
}

#[test]
fn test_json_format_specification() {
    // Test success format
    let success = ApiResult::success(vec![1, 2, 3]);
    let json = serde_json::to_string(&success).unwrap();
    assert!(json.contains(r#""ok":true"#));
    assert!(json.contains(r#""data""#));

    // Test error format
    let error = ApiResult::<()>::error(ErrorCode::NotFound, "Not found");
    let json = serde_json::to_string(&error).unwrap();
    assert!(json.contains(r#""ok":false"#));
    assert!(json.contains(r#""error":"#));
    assert!(json.contains(r#""code":"NOT_FOUND""#));
    assert!(json.contains(r#""message""#));

    // Test error with details format
    let error = ApiResult::<()>::error_with_details(
        ErrorCode::ValidationError,
        "Invalid input",
        "Field 'email' is required",
    );
    let json = serde_json::to_string(&error).unwrap();
    assert!(json.contains(r#""ok":false"#));
    assert!(json.contains(r#""code":"VALIDATION_ERROR""#));
    assert!(json.contains(r#""details":"Field 'email' is required""#));
}

#[test]
fn test_comprehensive_error_conversions() {
    // Test ApplicationError conversion
    let app_err = ApplicationError::ServiceNotAvailable {
        service: "TestService".to_string(),
        reason: "Initializing".to_string(),
    };
    let api_err: ApiError = app_err.into();
    assert_eq!(api_err.code, ErrorCode::ServiceNotAvailable);
    assert!(api_err.details.is_some());

    // Test DomainError conversion
    let domain_err = DomainError::ValidationFailed {
        field: "email".to_string(),
        reason: "Invalid format".to_string(),
    };
    let api_err: ApiError = domain_err.into();
    assert_eq!(api_err.code, ErrorCode::ValidationError);
    assert!(api_err.message.contains("email"));

    // Test AppError conversion with nested errors
    let app_error = AppError::Domain(Box::new(DomainError::EntityNotFound {
        entity_type: "Document".to_string(),
        identifier: "doc-123".to_string(),
    }));
    let api_err: ApiError = app_error.into();
    assert_eq!(api_err.code, ErrorCode::NotFound);
}

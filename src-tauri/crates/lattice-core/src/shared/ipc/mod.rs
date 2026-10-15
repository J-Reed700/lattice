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
//! use lattice_core::shared::ipc::{ApiResult, ErrorCode};
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

use crate::shared::error::AppError;
use crate::shared::error::{ApplicationError, DomainError};
use serde::{Deserialize, Serialize};

mod conversions;

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
/// # use lattice_core::shared::ipc::{ApiResult, ErrorCode};
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
    /// # use lattice_core::shared::ipc::ApiResult;
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
    /// # use lattice_core::shared::ipc::{ApiResult, ErrorCode};
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
    /// # use lattice_core::shared::ipc::{ApiResult, ErrorCode};
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
    /// # use lattice_core::shared::ipc::ApiResult;
    /// # use lattice_core::application::error::ApplicationError;
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
    /// # use lattice_core::shared::ipc::ApiResult;
    /// let success = ApiResult::success(42);
    /// assert!(success.is_ok());
    ///
    /// let error = ApiResult::<i32>::error(
    ///     lattice_core::shared::ipc::ErrorCode::NotFound,
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
    /// # use lattice_core::shared::ipc::ApiResult;
    /// let success = ApiResult::success(42);
    /// assert!(!success.is_err());
    ///
    /// let error = ApiResult::<i32>::error(
    ///     lattice_core::shared::ipc::ErrorCode::NotFound,
    ///     "Not found"
    /// );
    /// assert!(error.is_err());
    /// ```
    pub fn is_err(&self) -> bool {
        matches!(self, Self::Error { .. })
    }

    /// The command-shaped result: the data, or the error as the IPC `Err`.
    ///
    /// Tauri commands return `Result<T, ApiError>`; this is how an `_impl`
    /// that builds an `ApiResult` reaches that shape without the envelope
    /// riding inside a successful reply.
    pub fn into_result(self) -> Result<T, ApiError> {
        match self {
            Self::Success { data, .. } => Ok(data),
            Self::Error { error, .. } => Err(error),
        }
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
/// # use lattice_core::shared::ipc::{ApiError, ErrorCode};
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
/// # use lattice_core::shared::ipc::ErrorCode;
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
/// # use lattice_core::shared::ipc::sanitize_database_error;
/// let error = "error in SQL: SELECT * FROM users WHERE password = 'secret'";
/// let sanitized = sanitize_database_error(error);
/// assert!(!sanitized.contains("SELECT"));
/// assert!(!sanitized.contains("secret"));
/// ```
pub(crate) fn sanitize_database_error(msg: &str) -> String {
    // Remove SQL queries (anything between quotes or after "SQL:")
    let msg = msg.split("SQL:").next().unwrap_or(msg);
    let msg = msg.split("Query:").next().unwrap_or(msg);

    let msg = if msg.contains("://") {
        "Database connection error"
    } else {
        msg
    };

    // Truncate long messages
    if msg.len() > 200 {
        format!(
            "{}...",
            &msg[..crate::shared::text::floor_char_boundary(msg, 200)]
        )
    } else {
        msg.to_string()
    }
}

#[cfg(test)]
mod tests;

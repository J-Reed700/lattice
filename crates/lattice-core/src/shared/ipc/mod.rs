//! Standardized result wrapper for IPC gateway responses.

use crate::shared::error::AppError;
use crate::shared::error::{ApplicationError, DomainError};
use serde::{Deserialize, Serialize};

mod conversions;

/// API result wrapper for IPC gateway responses.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(untagged)]
pub enum ApiResult<T> {
    Success {
        ok: bool,
        data: T,
    },
    Error {
        ok: bool,
        error: ApiError,
    },
}

impl<T> ApiResult<T> {
    /// Creates a success result.
    pub fn success(data: T) -> Self {
        Self::Success { ok: true, data }
    }

    /// Creates an error result without details.
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
    pub fn is_ok(&self) -> bool {
        matches!(self, Self::Success { .. })
    }

    /// Returns true if this is an error result.
    pub fn is_err(&self) -> bool {
        matches!(self, Self::Error { .. })
    }

    /// The command-shaped result: the data, or the error as the IPC `Err`.
    pub fn into_result(self) -> Result<T, ApiError> {
        match self {
            Self::Success { data, .. } => Ok(data),
            Self::Error { error, .. } => Err(error),
        }
    }
}

/// Structured error information for API responses.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct ApiError {
    pub code: ErrorCode,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
}

/// Comprehensive error code enumeration for API responses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ErrorCode {
    NotFound,
    AlreadyExists,
    Gone,

    PermissionDenied,
    Unauthorized,
    SecurityViolation,

    ValidationError,
    InvalidInput,
    InvalidConfig,
    InvalidState,
    ConstraintViolation,

    FileNotFound,
    FileTooLarge,
    UnsupportedFileType,
    FileSystemError,
    FileReadError,
    FileWriteError,

    DatabaseError,
    DatabaseConnectionError,
    MigrationError,

    NetworkError,
    Timeout,
    RateLimitExceeded,

    ServiceNotAvailable,
    ServiceInitializationError,
    ModelNotLoaded,
    ModelLoadError,

    ProcessingError,
    SerializationError,
    DeserializationError,
    ParsingError,
    EmbeddingError,
    ExtractionError,
    TokenizationError,

    ConcurrentModification,
    QueueFull,

    BackupCreationFailed,
    BackupRestoreFailed,
    BackupCorrupted,

    InternalError,
    NotImplemented,
    Unknown,
}

/// Sanitizes database error messages to avoid leaking sensitive schema or query details.
pub(crate) fn sanitize_database_error(msg: &str) -> String {
    let msg = msg.split("SQL:").next().unwrap_or(msg);
    let msg = msg.split("Query:").next().unwrap_or(msg);

    let msg = if msg.contains("://") {
        "Database connection error"
    } else {
        msg
    };

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

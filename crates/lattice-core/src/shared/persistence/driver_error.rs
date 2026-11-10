//! The database driver's error, carried into the shared error contracts
//! (`AppError`, `ApiError`) so `?` works on sqlx results in every layer.
use crate::shared::error::AppError;
use crate::shared::ipc::{sanitize_database_error, ApiError, ErrorCode};

/// Auto-convert from sqlx::Error
impl From<sqlx::Error> for AppError {
    fn from(err: sqlx::Error) -> Self {
        AppError::Database(err.to_string())
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

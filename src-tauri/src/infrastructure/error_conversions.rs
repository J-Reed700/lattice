//! Driver error conversions live outside the shared error contract.
use crate::shared::error::AppError;

/// Auto-convert from sqlx::Error
impl From<sqlx::Error> for AppError {
    fn from(err: sqlx::Error) -> Self {
        AppError::Database(err.to_string())
    }
}

/// Auto-convert from ndarray::ShapeError
impl From<ndarray::ShapeError> for AppError {
    fn from(err: ndarray::ShapeError) -> Self {
        AppError::Other(format!("Array shape error: {}", err))
    }
}

/// Auto-convert from keyring::Error
impl From<keyring::Error> for AppError {
    fn from(err: keyring::Error) -> Self {
        AppError::KeyringError(err.to_string())
    }
}

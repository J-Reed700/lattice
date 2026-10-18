//! The database driver's error, carried into the shared error contract so
//! `?` works on sqlx results in every layer.
use crate::shared::error::AppError;

/// Auto-convert from sqlx::Error
impl From<sqlx::Error> for AppError {
    fn from(err: sqlx::Error) -> Self {
        AppError::Database(err.to_string())
    }
}

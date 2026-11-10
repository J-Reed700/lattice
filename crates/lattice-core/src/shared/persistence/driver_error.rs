//! The database driver's error, carried into `AppError` so `?` works on sqlx
//! results in data modules.
use crate::shared::error::AppError;

impl From<sqlx::Error> for AppError {
    fn from(err: sqlx::Error) -> Self {
        AppError::Database(err.to_string())
    }
}

use crate::shared::error::AppError;

pub type IndexingError = AppError;
pub type Result<T> = std::result::Result<T, AppError>;

pub mod db;
pub mod sync_repository;

impl From<sqlx::Error> for crate::error::AppError {
    fn from(error: sqlx::Error) -> Self {
        Self::Db(Box::new(error))
    }
}

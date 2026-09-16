use crate::shared::error::AppError;
use async_trait::async_trait;

#[async_trait]
pub trait SystemRepository: Send + Sync {
    async fn vacuum(&self) -> Result<(), AppError>;

    async fn analyze(&self) -> Result<(), AppError>;

    async fn get_database_size(&self) -> Result<i64, AppError>;

    async fn integrity_check(&self) -> Result<bool, AppError>;
}

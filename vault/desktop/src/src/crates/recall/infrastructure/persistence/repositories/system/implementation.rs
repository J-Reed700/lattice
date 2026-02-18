use crate::domain::repositories::system_repository::SystemRepository;
use crate::error::AppError;
use async_trait::async_trait;
use sqlx::SqlitePool;

#[derive(Clone)]
pub struct SqliteSystemRepository {
    pool: SqlitePool,
}

impl SqliteSystemRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl SystemRepository for SqliteSystemRepository {
    async fn vacuum(&self) -> Result<(), AppError> {
        let mut conn = self.pool.acquire().await?;
        super::ops::vacuum(&mut conn).await
    }

    async fn analyze(&self) -> Result<(), AppError> {
        let mut conn = self.pool.acquire().await?;
        super::ops::analyze(&mut conn).await
    }

    async fn get_database_size(&self) -> Result<i64, AppError> {
        let mut conn = self.pool.acquire().await?;
        super::ops::get_database_size(&mut conn).await
    }

    async fn integrity_check(&self) -> Result<bool, AppError> {
        let mut conn = self.pool.acquire().await?;
        super::ops::integrity_check(&mut conn).await
    }
}

use async_trait::async_trait;
use lattice_core::application::ports::DatabaseStatsPort;
use lattice_core::shared::error::Result;
use sqlx::SqlitePool;

pub struct DatabaseStatsAdapter {
    pool: SqlitePool,
}

impl DatabaseStatsAdapter {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl DatabaseStatsPort for DatabaseStatsAdapter {
    async fn get_database_size_bytes(&self) -> Result<i64> {
        super::repository::database_size_bytes(&self.pool).await
    }
}

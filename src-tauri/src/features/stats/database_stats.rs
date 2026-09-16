use crate::application::ports::DatabaseStatsPort;
use crate::shared::error::{Result, ResultExt};
use async_trait::async_trait;
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
        let (page_count, page_size): (i64, i64) = sqlx::query_as(
            "SELECT page_count, page_size FROM pragma_page_count(), pragma_page_size()",
        )
        .fetch_one(&self.pool)
        .await
        .context("Failed to read database size")?;

        Ok(page_count * page_size)
    }
}

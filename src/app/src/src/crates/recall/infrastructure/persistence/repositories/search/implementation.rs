use crate::domain::repositories::search_repository::{SearchRepository, SearchResult};
use crate::error::AppError;
use async_trait::async_trait;
use sqlx::SqlitePool;

use super::ops;

#[derive(Clone, Debug)]
pub struct SqliteSearchRepository {
    pool: SqlitePool,
}

impl SqliteSearchRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl SearchRepository for SqliteSearchRepository {
    async fn search_bm25(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>, AppError> {
        let mut conn = self.pool.acquire().await?;
        ops::search_bm25(&mut conn, query, limit).await
    }

    async fn count_searchable_chunks(&self) -> Result<i64, AppError> {
        let mut conn = self.pool.acquire().await?;
        ops::count_searchable_chunks(&mut conn).await
    }

    async fn optimize_index(&self) -> Result<(), AppError> {
        let mut conn = self.pool.acquire().await?;
        ops::optimize_index(&mut conn).await
    }

    async fn rebuild_index(&self) -> Result<(), AppError> {
        let mut conn = self.pool.acquire().await?;
        ops::rebuild_index(&mut conn).await
    }
}

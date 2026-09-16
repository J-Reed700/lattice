//! Chunk Repository Implementation
//!
//! SQLite pool-based implementation of ChunkRepository that delegates to ops.rs.

use crate::application::ports::{ChunkRepositoryPort, Filter, RepositoryPort};
use crate::domain::entities::chunk::Chunk as ChunkEntity;
use crate::shared::error::{AppError, Result};
use async_trait::async_trait;
use sqlx::SqlitePool;

use super::ops;

#[derive(Debug, Clone)]
pub struct ChunkFilter {
    pub document_id: Option<String>,
    pub limit: Option<usize>,
}

impl Filter for ChunkFilter {
    fn validate(&self) -> Result<()> {
        if let Some(limit) = self.limit {
            if limit == 0 || limit > 10000 {
                return Err(AppError::InvalidInput(
                    "Limit must be between 1 and 10000".to_string(),
                ));
            }
        }
        Ok(())
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

#[derive(Debug, Clone)]
pub struct SqliteChunkRepository {
    pool: SqlitePool,
}

impl SqliteChunkRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn find_by_document(&self, document_id: &str) -> Result<Vec<ChunkEntity>> {
        let mut conn = self.pool.acquire().await?;
        ops::find_by_document(&mut conn, document_id).await
    }

    pub async fn delete_by_document(&self, document_id: &str) -> Result<()> {
        let mut conn = self.pool.acquire().await?;
        ops::delete_by_document(&mut conn, document_id).await
    }

    pub async fn count_by_document(&self, document_id: &str) -> Result<usize> {
        let mut conn = self.pool.acquire().await?;
        ops::count_by_document(&mut conn, document_id).await
    }
}

#[async_trait]
impl RepositoryPort<ChunkEntity> for SqliteChunkRepository {
    async fn find_by_id(&self, id: &str) -> Result<Option<ChunkEntity>> {
        let mut conn = self.pool.acquire().await?;
        ops::find_by_id(&mut conn, id).await
    }

    async fn find_by_filter(&self, filter: &dyn Filter) -> Result<Vec<ChunkEntity>> {
        filter.validate()?;

        let filter_any = filter.as_any();
        if let Some(chunk_filter) = filter_any.downcast_ref::<ChunkFilter>() {
            if let Some(document_id) = &chunk_filter.document_id {
                return self.find_by_document(document_id).await;
            }
        }

        Ok(vec![])
    }

    async fn find_all(&self) -> Result<Vec<ChunkEntity>> {
        let mut conn = self.pool.acquire().await?;
        ops::find_all(&mut conn).await
    }

    async fn save(&self, entity: &ChunkEntity) -> Result<()> {
        let mut conn = self.pool.acquire().await?;
        ops::save(&mut conn, entity).await
    }

    async fn save_batch(&self, entities: &[ChunkEntity]) -> Result<()> {
        if entities.is_empty() {
            return Ok(());
        }

        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| AppError::Database(format!("Failed to begin transaction: {}", e)))?;

        for entity in entities {
            ops::save(&mut tx, entity).await?;
        }

        tx.commit()
            .await
            .map_err(|e| AppError::Database(format!("Failed to commit transaction: {}", e)))?;

        Ok(())
    }

    async fn delete(&self, id: &str) -> Result<()> {
        let mut conn = self.pool.acquire().await?;
        ops::delete(&mut conn, id).await
    }

    async fn delete_batch(&self, ids: &[&str]) -> Result<()> {
        if ids.is_empty() {
            return Ok(());
        }

        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| AppError::Database(format!("Failed to begin transaction: {}", e)))?;

        for id in ids {
            ops::delete(&mut tx, id).await?;
        }

        tx.commit()
            .await
            .map_err(|e| AppError::Database(format!("Failed to commit transaction: {}", e)))?;

        Ok(())
    }

    async fn count(&self) -> Result<usize> {
        let mut conn = self.pool.acquire().await?;
        ops::count(&mut conn).await
    }

    async fn exists(&self, id: &str) -> Result<bool> {
        let mut conn = self.pool.acquire().await?;
        ops::exists(&mut conn, id).await
    }
}

#[async_trait]
impl ChunkRepositoryPort for SqliteChunkRepository {
    async fn create(
        &self,
        document_id: &str,
        content: &str,
        _context_prefix: Option<&str>,
        _contextualized_content: Option<&str>,
        index: usize,
        _start_char: Option<i64>,
        _end_char: Option<i64>,
    ) -> Result<ChunkEntity> {
        use crate::shared::domain_types::DocumentId;
        let doc_id = DocumentId::from(document_id.to_string());
        let chunk = ChunkEntity::new(doc_id, content.to_string(), index);
        RepositoryPort::save(self, &chunk).await?;
        Ok(chunk)
    }

    async fn find_by_document(&self, document_id: &str) -> Result<Vec<ChunkEntity>> {
        let mut conn = self.pool.acquire().await?;
        ops::find_by_document(&mut conn, document_id).await
    }

    async fn find_by_ids(&self, chunk_ids: &[String]) -> Result<Vec<ChunkEntity>> {
        let mut conn = self.pool.acquire().await?;
        ops::find_by_ids(&mut conn, chunk_ids).await
    }

    async fn delete_by_document(&self, document_id: &str) -> Result<()> {
        let mut conn = self.pool.acquire().await?;
        ops::delete_by_document(&mut conn, document_id).await
    }

    async fn count_all(&self) -> Result<i64> {
        let mut conn = self.pool.acquire().await?;
        ops::count_all(&mut conn).await
    }

    async fn count_indexed_documents(&self) -> Result<i64> {
        let mut conn = self.pool.acquire().await?;
        ops::count_indexed_documents(&mut conn).await
    }

    async fn count_by_document(&self, document_id: &str) -> Result<i64> {
        let mut conn = self.pool.acquire().await?;
        ops::count_by_document(&mut conn, document_id)
            .await
            .map(|c| c as i64)
    }

    async fn create_batch(&self, chunks: Vec<ChunkEntity>) -> Result<Vec<ChunkEntity>> {
        RepositoryPort::save_batch(self, &chunks).await?;
        Ok(chunks)
    }
}

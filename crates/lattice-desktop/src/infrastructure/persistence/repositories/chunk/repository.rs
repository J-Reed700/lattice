//! Pool-based chunk repository: each method acquires a connection or begins a
//! unit of work and calls the SQL in `ops`.

use async_trait::async_trait;
use lattice_core::application::ports::{ChunkRepositoryPort, RepositoryPort};
use lattice_core::domain::entities::chunk::Chunk as ChunkEntity;
use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::persistence::driver::DriverPool;
use lattice_core::shared::persistence::Transactions;
use sqlx::SqlitePool;
use std::collections::HashMap;

use super::ops;

/// SQLite chunk persistence behind `RepositoryPort<ChunkEntity>` and `ChunkRepositoryPort`.
#[derive(Debug, Clone)]
pub struct ChunkRepository {
    pool: SqlitePool,
}

impl ChunkRepository {
    pub fn new(pool: impl DriverPool) -> Self {
        let pool = pool.sqlx_pool().clone();
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
impl RepositoryPort<ChunkEntity> for ChunkRepository {
    async fn find_by_id(&self, id: &str) -> Result<Option<ChunkEntity>> {
        let mut conn = self.pool.acquire().await?;
        ops::find_by_id(&mut conn, id).await
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
            .begin_write()
            .await
            .map_err(|e| AppError::Database(format!("Failed to begin transaction: {}", e)))?;

        ops::save_batch_optimized(&mut tx, entities).await?;

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
            .begin_write()
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
impl ChunkRepositoryPort for ChunkRepository {
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
        use lattice_core::shared::types::DocumentId;
        let doc_id = DocumentId::from(document_id.to_string());
        let chunk = ChunkEntity::new(doc_id, content.to_string(), index);
        RepositoryPort::save(self, &chunk).await?;
        Ok(chunk)
    }

    async fn find_by_document(&self, document_id: &str) -> Result<Vec<ChunkEntity>> {
        ChunkRepository::find_by_document(self, document_id).await
    }

    async fn find_first_by_document(
        &self,
        document_id: &str,
        limit: usize,
    ) -> Result<Vec<ChunkEntity>> {
        let mut conn = self.pool.acquire().await?;
        ops::find_by_document_limit(&mut conn, document_id, Some(limit)).await
    }

    async fn find_by_ids(&self, chunk_ids: &[String]) -> Result<Vec<ChunkEntity>> {
        let mut conn = self.pool.acquire().await?;
        ops::find_by_ids(&mut conn, chunk_ids).await
    }

    async fn find_content_by_ids(&self, chunk_ids: &[String]) -> Result<HashMap<String, String>> {
        let mut conn = self.pool.acquire().await?;
        ops::find_content_by_ids(&mut conn, chunk_ids).await
    }

    async fn find_conversation_attached_chunk_ids(&self) -> Result<Vec<String>> {
        let mut conn = self.pool.acquire().await?;
        ops::find_conversation_attached_chunk_ids(&mut conn).await
    }

    async fn delete_by_document(&self, document_id: &str) -> Result<()> {
        ChunkRepository::delete_by_document(self, document_id).await
    }

    async fn count_all(&self) -> Result<i64> {
        let mut conn = self.pool.acquire().await?;
        ops::count_all(&mut conn).await
    }

    async fn count_indexed_documents(&self) -> Result<i64> {
        let mut conn = self.pool.acquire().await?;
        ops::count_documents_with_chunks(&mut conn).await
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

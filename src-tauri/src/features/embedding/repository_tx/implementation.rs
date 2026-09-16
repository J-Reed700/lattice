//! Embedding Repository Implementation
//!
//! SQLite pool-based implementation of EmbeddingRepository that delegates to ops.rs.

use crate::application::ports::EmbeddingRepositoryPort;
use crate::features::embedding::entity::Embedding as DomainEmbedding;
use crate::shared::error::Result;
use async_trait::async_trait;
use sqlx::SqlitePool;

use super::ops;

#[derive(Debug, Clone)]
pub struct SqliteEmbeddingRepository {
    pool: SqlitePool,
}

impl SqliteEmbeddingRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl EmbeddingRepositoryPort for SqliteEmbeddingRepository {
    async fn create(&self, chunk_id: &str, vector: &[f32], model: &str) -> Result<String> {
        use crate::shared::domain_types::ChunkId;
        let chunk_id_typed = ChunkId::from(chunk_id.to_string());
        let embedding = DomainEmbedding::new(chunk_id_typed, model.to_string(), vector.len());
        self.save(&embedding, vector.to_vec()).await?;
        Ok(embedding.id().to_string())
    }

    async fn find_by_chunk(&self, chunk_id: &str) -> Result<Option<DomainEmbedding>> {
        let result = self.find_by_chunk_id(chunk_id).await?;
        Ok(result.map(|(entity, _vector)| entity))
    }

    async fn save(&self, entity: &DomainEmbedding, vector: Vec<f32>) -> Result<()> {
        let mut conn = self.pool.acquire().await?;
        ops::save(&mut conn, entity, vector).await
    }

    async fn save_batch(&self, entries: Vec<(DomainEmbedding, Vec<f32>)>) -> Result<()> {
        if entries.is_empty() {
            return Ok(());
        }

        let mut tx = self.pool.begin().await.map_err(|e| {
            crate::shared::error::AppError::Database(format!("Failed to begin transaction: {}", e))
        })?;

        ops::save_batch(&mut tx, entries).await?;

        tx.commit().await.map_err(|e| {
            crate::shared::error::AppError::Database(format!("Failed to commit transaction: {}", e))
        })?;

        Ok(())
    }

    async fn find_by_chunk_id(
        &self,
        chunk_id: &str,
    ) -> Result<Option<(DomainEmbedding, Vec<f32>)>> {
        let mut conn = self.pool.acquire().await?;
        ops::find_by_chunk_id(&mut conn, chunk_id).await
    }

    async fn find_by_document_id(
        &self,
        document_id: &str,
    ) -> Result<Vec<(DomainEmbedding, Vec<f32>)>> {
        let mut conn = self.pool.acquire().await?;
        ops::find_by_document_id(&mut conn, document_id).await
    }

    async fn delete_by_chunk_id(&self, chunk_id: &str) -> Result<()> {
        let mut conn = self.pool.acquire().await?;
        ops::delete_by_chunk_id(&mut conn, chunk_id).await
    }

    async fn delete_by_document_id(&self, document_id: &str) -> Result<()> {
        let mut conn = self.pool.acquire().await?;
        ops::delete_by_document_id(&mut conn, document_id).await
    }

    async fn count(&self) -> Result<i64> {
        let mut conn = self.pool.acquire().await?;
        ops::count(&mut conn).await
    }

    async fn create_batch(&self, entries: Vec<(DomainEmbedding, Vec<f32>)>) -> Result<Vec<String>> {
        let ids: Vec<String> = entries.iter().map(|(e, _)| e.id().to_string()).collect();
        self.save_batch(entries).await?;
        Ok(ids)
    }
}

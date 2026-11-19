//! SQLite implementation of the embedding repository port.

use crate::infrastructure::persistence::repositories::embedding::ops;
use async_trait::async_trait;
use lattice_core::application::ports::EmbeddingRepositoryPort;
use lattice_core::domain::entities::Embedding as DomainEmbedding;
use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::persistence::Database;

/// Embedding entity from database
#[derive(Debug, Clone)]
pub struct Embedding {
    pub id: String,
    pub chunk_id: String,
    pub embedding: Vec<f32>,
    pub model_name: String,
    pub created_at: String,
}

/// Production embedding repository backed by SQLite
pub struct EmbeddingRepository {
    db: Database,
}

impl EmbeddingRepository {
    pub fn new(db: Database) -> Self {
        Self { db }
    }
}

#[async_trait]
impl EmbeddingRepositoryPort for EmbeddingRepository {
    async fn create(&self, chunk_id: &str, vector: &[f32], model: &str) -> Result<String> {
        use lattice_core::shared::types::ChunkId;
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
        let mut tx = self.db.begin_write().await?;
        ops::save(&mut tx, entity, vector).await?;
        tx.commit().await?;
        Ok(())
    }

    async fn save_batch(&self, entries: Vec<(DomainEmbedding, Vec<f32>)>) -> Result<()> {
        if entries.is_empty() {
            return Ok(());
        }
        let mut tx = self
            .db
            .begin_write()
            .await
            .map_err(|e| AppError::Database(format!("Failed to begin transaction: {}", e)))?;

        ops::save_batch(&mut tx, entries).await?;

        tx.commit()
            .await
            .map_err(|e| AppError::Database(format!("Failed to commit transaction: {}", e)))?;

        Ok(())
    }

    async fn find_by_chunk_id(
        &self,
        chunk_id: &str,
    ) -> Result<Option<(DomainEmbedding, Vec<f32>)>> {
        let mut read = self.db.begin_read().await?;
        let found = ops::find_by_chunk_id(&mut read, chunk_id).await?;
        read.commit().await?;
        Ok(found)
    }

    async fn find_by_document_id(
        &self,
        document_id: &str,
    ) -> Result<Vec<(DomainEmbedding, Vec<f32>)>> {
        let mut read = self.db.begin_read().await?;
        let found = ops::find_by_document_id(&mut read, document_id).await?;
        read.commit().await?;
        Ok(found)
    }

    async fn vectors_by_document_ids(
        &self,
        document_ids: &[String],
    ) -> Result<lattice_core::application::ports::embedding_repository_port::DocumentVectors> {
        let mut read = self.db.begin_read().await?;
        let vectors =
            crate::infrastructure::persistence::repositories::embedding::ops::vectors_by_document_ids(
                &mut read,
                document_ids,
            )
            .await?;
        read.commit().await?;
        Ok(vectors)
    }

    async fn delete_by_chunk_id(&self, chunk_id: &str) -> Result<()> {
        let mut tx = self.db.begin_write().await?;
        sqlx::query!(
            r#"DELETE FROM text_embeddings WHERE chunk_id = ?"#,
            chunk_id
        )
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::Database(format!("Failed to delete embedding: {}", e)))?;
        tx.commit().await?;

        Ok(())
    }

    async fn delete_by_document_id(&self, document_id: &str) -> Result<()> {
        let mut tx = self.db.begin_write().await?;
        sqlx::query(
            r#"
            DELETE FROM text_embeddings
            WHERE chunk_id IN (
                SELECT id FROM text_chunks WHERE document_id = ?
            )
            "#,
        )
        .bind(document_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::Database(format!("Failed to delete embeddings: {}", e)))?;
        tx.commit().await?;

        Ok(())
    }

    async fn count(&self) -> Result<i64> {
        let mut read = self.db.begin_read().await?;
        let count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM text_embeddings")
            .fetch_one(&mut *read)
            .await
            .map_err(|e| AppError::Database(format!("Failed to count embeddings: {}", e)))?;
        read.commit().await?;
        Ok(count)
    }

    async fn create_batch(&self, entries: Vec<(DomainEmbedding, Vec<f32>)>) -> Result<Vec<String>> {
        let ids: Vec<String> = entries.iter().map(|(e, _)| e.id().to_string()).collect();
        self.save_batch(entries).await?;
        Ok(ids)
    }
}

//! Embedding Repository Implementation
//!
//! This module implements the embedding repository port.
//!
//! # Migration Status
//! - [x] Basic implementation added for build compatibility
//! - [x] Migrated to DDD EmbeddingRepositoryPort
//!
//! # Implements
//! - EmbeddingRepositoryTrait (legacy)
//! - EmbeddingRepositoryPort (DDD)

use crate::application::ports::EmbeddingRepositoryPort;
use crate::features::embedding::entity::Embedding as DomainEmbedding;
use crate::features::embedding::persistence_mapper::{
    EmbeddingDTO, EmbeddingMapper,
};
use crate::shared::error::{AppError, Result};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::{Row, SqlitePool};

// Database row struct for embedding queries
#[derive(Debug, sqlx::FromRow)]
struct EmbeddingRow {
    chunk_id: String,
    embedding: Vec<u8>,
    model_name: String,
    dimension: i64,
    created_at: String,
}

// Legacy trait import removed - migrated to DDD
// use super::traits::EmbeddingRepositoryTrait;

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
    pool: SqlitePool,
}

impl EmbeddingRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

// ============================================================================
// Legacy Trait Implementation - COMMENTED OUT (migrated to DDD)
// ============================================================================
// The old EmbeddingRepositoryTrait implementation has been replaced with
// EmbeddingRepositoryPort. See the implementation at the end of this file.

/*
#[async_trait]
impl EmbeddingRepositoryTrait for EmbeddingRepository {
    async fn create(&self, chunk_id: &str, embedding: &[f32], model_name: &str) -> Result<String> {
        let id = Uuid::new_v4().to_string();
        let created_at = Utc::now().to_rfc3339();

        // Serialize embedding as bytes
        let embedding_bytes: Vec<u8> = embedding
            .iter()
            .flat_map(|f| f.to_le_bytes())
            .collect();

        sqlx::query(
            r#"
            INSERT INTO embeddings (id, chunk_id, embedding, model_name, created_at)
            VALUES (?, ?, ?, ?, ?)
            "#,
        )
        .bind(&id)
        .bind(chunk_id)
        .bind(&embedding_bytes)
        .bind(model_name)
        .bind(&created_at)
        .execute(&self.pool)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;

        Ok(id)
    }

    async fn create_batch(
        &self,
        chunk_embeddings: Vec<(&str, Vec<f32>)>,
        model_name: &str,
    ) -> Result<Vec<String>> {
        let mut ids = Vec::new();

        for (chunk_id, embedding) in chunk_embeddings {
            let id = self.create(chunk_id, &embedding, model_name).await?;
            ids.push(id);
        }

        Ok(ids)
    }

    async fn find_by_chunk(&self, chunk_id: &str) -> Result<Option<Embedding>> {
        let row = sqlx::query(
            r#"
            SELECT id, chunk_id, embedding, model_name, created_at
            FROM embeddings
            WHERE chunk_id = ?
            "#,
        )
        .bind(chunk_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;

        Ok(row.map(|r| {
            let embedding_bytes: Vec<u8> = r.try_get("embedding").map_err(|e| AppError::Database(e.to_string()))?;
            let embedding: Vec<f32> = embedding_bytes
                .chunks_exact(4)
                .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
                .collect();

            Ok::<_, AppError>(Embedding {
                id: r.try_get("id").map_err(|e| AppError::Database(e.to_string()))?,
                chunk_id: r.try_get("chunk_id").map_err(|e| AppError::Database(e.to_string()))?,
                embedding,
                model_name: r.try_get("model_name").map_err(|e| AppError::Database(e.to_string()))?,
                created_at: r.try_get("created_at").map_err(|e| AppError::Database(e.to_string()))?,
            })
        }).transpose()?.flatten())
    }

    async fn find_all_for_document(&self, document_id: &str) -> Result<Vec<Embedding>> {
        let rows = sqlx::query(
            r#"
            SELECT e.id, e.chunk_id, e.embedding, e.model_name, e.created_at
            FROM embeddings e
            JOIN text_chunks c ON e.chunk_id = c.id
            WHERE c.document_id = ?
            "#,
        )
        .bind(document_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;

        rows.into_iter().map(|r| {
            let embedding_bytes: Vec<u8> = r.try_get("embedding").map_err(|e| AppError::Database(e.to_string()))?;
            let embedding: Vec<f32> = embedding_bytes
                .chunks_exact(4)
                .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
                .collect();

            Ok(Embedding {
                id: r.try_get("id").map_err(|e| AppError::Database(e.to_string()))?,
                chunk_id: r.try_get("chunk_id").map_err(|e| AppError::Database(e.to_string()))?,
                embedding,
                model_name: r.try_get("model_name").map_err(|e| AppError::Database(e.to_string()))?,
                created_at: r.try_get("created_at").map_err(|e| AppError::Database(e.to_string()))?,
            })
        }).collect()
    }

    async fn delete_by_chunk(&self, chunk_id: &str) -> Result<()> {
        sqlx::query("DELETE FROM embeddings WHERE chunk_id = ?")
            .bind(chunk_id)
            .execute(&self.pool)
            .await
            .map_err(|e| AppError::Database(e.to_string()))?;
        Ok(())
    }

    async fn delete_by_document(&self, document_id: &str) -> Result<()> {
        sqlx::query(
            r#"
            DELETE FROM embeddings
            WHERE chunk_id IN (
                SELECT id FROM text_chunks WHERE document_id = ?
            )
            "#,
        )
        .bind(document_id)
        .execute(&self.pool)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;
        Ok(())
    }

    async fn count(&self) -> Result<i64> {
        let row = sqlx::query("SELECT COUNT(*) as count FROM embeddings")
            .fetch_one(&self.pool)
            .await
            .map_err(|e| AppError::Database(e.to_string()))?;
        Ok(row.try_get("count").map_err(|e| AppError::Database(e.to_string()))?)
    }
}
*/

// ============================================================================
// DDD Port Implementation
// ============================================================================

#[async_trait]
impl EmbeddingRepositoryPort for EmbeddingRepository {
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
        // Validate dimension
        EmbeddingMapper::validate_dimension(entity, &vector)?;

        // Convert to DTO
        let dto = EmbeddingMapper::to_dto(entity, vector);

        // Serialize vector to bytes for SQLite BLOB storage
        let embedding_bytes = bincode::serialize(&dto.embedding)
            .map_err(|e| AppError::Serialization(e.to_string()))?;

        // Generate a deterministic ID based on chunk_id for upserts
        let id = format!("emb_{}", dto.chunk_id);

        let dimension = dto.dimension as i32;
        let created_at = dto.computed_at.to_rfc3339();

        sqlx::query!(
            r#"
            INSERT INTO text_embeddings (id, chunk_id, embedding, model_name, dimension, created_at)
            VALUES (?, ?, ?, ?, ?, ?)
            ON CONFLICT(id) DO UPDATE SET
                chunk_id = excluded.chunk_id,
                embedding = excluded.embedding,
                model_name = excluded.model_name,
                dimension = excluded.dimension,
                created_at = excluded.created_at
            "#,
            id,
            dto.chunk_id,
            embedding_bytes,
            dto.model_name,
            dimension,
            created_at
        )
        .execute(&self.pool)
        .await
        .map_err(|e| AppError::Database(format!("Failed to save embedding: {}", e)))?;

        Ok(())
    }

    async fn save_batch(&self, entries: Vec<(DomainEmbedding, Vec<f32>)>) -> Result<()> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| AppError::Database(format!("Failed to begin transaction: {}", e)))?;

        for (entity, vector) in entries {
            let dto = EmbeddingMapper::to_dto(&entity, vector);
            let embedding_bytes = bincode::serialize(&dto.embedding)
                .map_err(|e| AppError::Serialization(e.to_string()))?;

            // Generate a deterministic ID based on chunk_id for upserts
            let id = format!("emb_{}", dto.chunk_id);

            let dimension = dto.dimension as i32;
            let created_at = dto.computed_at.to_rfc3339();

            sqlx::query!(
                r#"
                INSERT INTO text_embeddings (id, chunk_id, embedding, model_name, dimension, created_at)
                VALUES (?, ?, ?, ?, ?, ?)
                ON CONFLICT(id) DO UPDATE SET
                    chunk_id = excluded.chunk_id,
                    embedding = excluded.embedding,
                    model_name = excluded.model_name,
                    dimension = excluded.dimension,
                    created_at = excluded.created_at
                "#,
                id,
                dto.chunk_id,
                embedding_bytes,
                dto.model_name,
                dimension,
                created_at
            )
            .execute(&mut *tx)
            .await
            .map_err(|e| AppError::Database(format!("Failed to save embedding: {}", e)))?;
        }

        tx.commit()
            .await
            .map_err(|e| AppError::Database(format!("Failed to commit transaction: {}", e)))?;

        Ok(())
    }

    async fn find_by_chunk_id(
        &self,
        chunk_id: &str,
    ) -> Result<Option<(DomainEmbedding, Vec<f32>)>> {
        let record = sqlx::query_as::<_, EmbeddingRow>(
            r#"
            SELECT chunk_id, embedding, model_name, dimension, created_at
            FROM text_embeddings
            WHERE chunk_id = ?
            "#,
        )
        .bind(chunk_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AppError::Database(format!("Failed to find embedding: {}", e)))?;

        match record {
            Some(rec) => {
                let vector: Vec<f32> = bincode::deserialize(&rec.embedding)
                    .map_err(|e| AppError::Deserialization(e.to_string()))?;

                let computed_at = DateTime::parse_from_rfc3339(&rec.created_at)
                    .map_err(|e| AppError::Parsing(e.to_string()))?
                    .with_timezone(&Utc);

                let dto = EmbeddingDTO {
                    chunk_id: rec.chunk_id,
                    embedding: vector.clone(),
                    model_name: rec.model_name,
                    dimension: rec.dimension as usize,
                    computed_at,
                    model_version: None,
                };

                let entity = EmbeddingMapper::from_dto(&dto)?;
                Ok(Some((entity, vector)))
            }
            None => Ok(None),
        }
    }

    async fn find_by_document_id(
        &self,
        document_id: &str,
    ) -> Result<Vec<(DomainEmbedding, Vec<f32>)>> {
        let records = sqlx::query_as::<_, EmbeddingRow>(
            r#"
            SELECT e.chunk_id, e.embedding, e.model_name, e.dimension, e.created_at
            FROM text_embeddings e
            INNER JOIN text_chunks c ON e.chunk_id = c.id
            WHERE c.document_id = ?
            ORDER BY c.chunk_index ASC
            "#,
        )
        .bind(document_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AppError::Database(format!("Failed to find embeddings: {}", e)))?;

        let mut results = Vec::new();
        for rec in records {
            let vector: Vec<f32> = bincode::deserialize(&rec.embedding)
                .map_err(|e| AppError::Deserialization(e.to_string()))?;

            let computed_at = DateTime::parse_from_rfc3339(&rec.created_at)
                .map_err(|e| AppError::Parsing(e.to_string()))?
                .with_timezone(&Utc);

            let dto = EmbeddingDTO {
                chunk_id: rec.chunk_id,
                embedding: vector.clone(),
                model_name: rec.model_name,
                dimension: rec.dimension as usize,
                computed_at,
                model_version: None,
            };

            let entity = EmbeddingMapper::from_dto(&dto)?;
            results.push((entity, vector));
        }

        Ok(results)
    }

    async fn delete_by_chunk_id(&self, chunk_id: &str) -> Result<()> {
        sqlx::query!(
            r#"DELETE FROM text_embeddings WHERE chunk_id = ?"#,
            chunk_id
        )
        .execute(&self.pool)
        .await
        .map_err(|e| AppError::Database(format!("Failed to delete embedding: {}", e)))?;

        Ok(())
    }

    async fn delete_by_document_id(&self, document_id: &str) -> Result<()> {
        sqlx::query(
            r#"
            DELETE FROM text_embeddings
            WHERE chunk_id IN (
                SELECT id FROM text_chunks WHERE document_id = ?
            )
            "#,
        )
        .bind(document_id)
        .execute(&self.pool)
        .await
        .map_err(|e| AppError::Database(format!("Failed to delete embeddings: {}", e)))?;

        Ok(())
    }

    async fn count(&self) -> Result<i64> {
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM text_embeddings")
            .fetch_one(&self.pool)
            .await
            .map_err(|e| AppError::Database(format!("Failed to count embeddings: {}", e)))
    }

    async fn create_batch(&self, entries: Vec<(DomainEmbedding, Vec<f32>)>) -> Result<Vec<String>> {
        let ids: Vec<String> = entries.iter().map(|(e, _)| e.id().to_string()).collect();
        self.save_batch(entries).await?;
        Ok(ids)
    }
}

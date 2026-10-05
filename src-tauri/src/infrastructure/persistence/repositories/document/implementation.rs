//! Document Repository Implementation
//!
//! SQLite pool-based implementation of DocumentRepository that delegates to ops.rs.

use crate::application::ports::{DocumentRepositoryPort, Filter, RepositoryPort};
use crate::domain::entities::Document as DocumentEntity;
use crate::shared::error::{AppError, Result};
use async_trait::async_trait;
use sqlx::SqlitePool;

use super::ops;

#[derive(Debug, Clone)]
pub struct SqliteDocumentRepository {
    pub(super) pool: SqlitePool,
}

impl SqliteDocumentRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn find_by_path(&self, file_path: &str) -> Result<Option<DocumentEntity>> {
        let mut conn = self.pool.acquire().await?;
        ops::find_by_path(&mut conn, file_path).await
    }

    pub async fn find_aggregate_by_path_exact(
        &self,
        file_path: &str,
    ) -> Result<Option<DocumentEntity>> {
        let mut tx = self.pool.begin().await?;
        let entity = ops::find_by_path(&mut tx, file_path).await?;
        let aggregate = match entity {
            Some(doc_entity) => {
                let chunks =
                    ops::fetch_chunks_for_document(&mut tx, doc_entity.id().as_str()).await?;
                let tags = ops::fetch_tags_for_document(&mut tx, doc_entity.id().as_str()).await?;
                Some(doc_entity.from_parts(chunks, tags)?)
            }
            None => None,
        };
        tx.commit().await?;
        Ok(aggregate)
    }

    pub async fn find_by_path_pattern(&self, pattern: &str) -> Result<Vec<DocumentEntity>> {
        let mut conn = self.pool.acquire().await?;
        ops::find_by_path_pattern(&mut conn, pattern).await
    }

    pub async fn exists_by_path(&self, file_path: &str) -> Result<bool> {
        let mut conn = self.pool.acquire().await?;
        ops::exists_by_path(&mut conn, file_path).await
    }

    pub async fn count_documents(&self) -> Result<i64> {
        let mut conn = self.pool.acquire().await?;
        ops::count_documents(&mut conn).await
    }

    pub async fn count_chunks(&self) -> Result<i64> {
        let mut conn = self.pool.acquire().await?;
        ops::count_chunks(&mut conn).await
    }
}

#[async_trait]
impl RepositoryPort<DocumentEntity> for SqliteDocumentRepository {
    async fn find_by_id(&self, id: &str) -> Result<Option<DocumentEntity>> {
        let mut tx = self.pool.begin().await?;
        let result = ops::find_by_id(&mut tx, id).await?;
        tx.commit().await?;
        Ok(result)
    }

    async fn find_by_filter(&self, filter: &dyn Filter) -> Result<Vec<DocumentEntity>> {
        let filter = filter
            .as_any()
            .downcast_ref::<crate::application::ports::DocumentFilter>()
            .ok_or_else(|| AppError::InvalidInput("Expected DocumentFilter".into()))?;
        let mut tx = self.pool.begin().await?;
        let documents = ops::find_by_filter(&mut tx, filter).await?;
        tx.commit().await?;
        Ok(documents)
    }

    async fn find_all(&self) -> Result<Vec<DocumentEntity>> {
        let mut tx = self.pool.begin().await?;
        let result = ops::find_all(&mut tx).await?;
        tx.commit().await?;
        Ok(result)
    }

    async fn save(&self, entity: &DocumentEntity) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        ops::save(&mut tx, entity).await?;
        tx.commit().await?;
        Ok(())
    }

    async fn save_batch(&self, entities: &[DocumentEntity]) -> Result<()> {
        if entities.is_empty() {
            return Ok(());
        }

        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| AppError::Database(format!("Failed to begin transaction: {}", e)))?;

        ops::save_batch(&mut tx, entities).await?;

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

        ops::delete_batch_optimized(&mut tx, ids).await?;

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
impl DocumentRepositoryPort for SqliteDocumentRepository {
    async fn list_metadata(&self) -> Result<Vec<DocumentEntity>> {
        let mut conn = self.pool.acquire().await?;
        ops::list_metadata(&mut conn).await
    }
    async fn find_metadata_by_ids(&self, ids: &[String]) -> Result<Vec<DocumentEntity>> {
        let mut conn = self.pool.acquire().await?;
        ops::find_metadata_by_ids(&mut conn, ids).await
    }
    async fn rename(&self, id: &str, name: &str) -> Result<()> {
        let mut conn = self.pool.acquire().await?;
        ops::rename(&mut conn, id, name).await
    }

    async fn find_file_path_by_id(&self, document_id: &str) -> Result<String> {
        let mut conn = self.pool.acquire().await?;
        ops::find_file_path_by_id(&mut conn, document_id).await
    }

    async fn document_exists(&self, document_id: &str) -> Result<bool> {
        let mut conn = self.pool.acquire().await?;
        ops::exists(&mut conn, document_id).await
    }

    async fn find_id_by_path(&self, file_path: &str) -> Result<Option<String>> {
        let mut conn = self.pool.acquire().await?;
        ops::find_id_by_path(&mut conn, file_path).await
    }

    async fn delete(&self, document_id: &str) -> Result<()> {
        let mut conn = self.pool.acquire().await?;
        ops::delete(&mut conn, document_id).await
    }

    async fn find_by_checksum(
        &self,
        checksum: &crate::domain::value_objects::Checksum,
    ) -> Result<Option<crate::domain::entities::Document>> {
        let mut tx = self.pool.begin().await?;
        let result = ops::find_aggregate_by_checksum_tx(&mut tx, checksum.as_str()).await?;
        tx.commit().await?;
        Ok(result)
    }

    async fn count_documents(&self) -> Result<i64> {
        let mut conn = self.pool.acquire().await?;
        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM documents")
            .fetch_one(&mut *conn)
            .await?;
        Ok(count.0)
    }

    async fn count_chunks(&self) -> Result<i64> {
        let mut conn = self.pool.acquire().await?;
        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM text_chunks")
            .fetch_one(&mut *conn)
            .await?;
        Ok(count.0)
    }

    async fn count_by_checksum(&self, checksum: &str) -> Result<u64> {
        let mut conn = self.pool.acquire().await?;
        ops::count_by_checksum(&mut conn, checksum).await
    }

    async fn list_checksums(&self) -> Result<Vec<String>> {
        let mut conn = self.pool.acquire().await?;
        ops::list_checksums(&mut conn).await
    }

    async fn find_checksum_by_id(&self, document_id: &str) -> Result<String> {
        let mut conn = self.pool.acquire().await?;
        ops::find_checksum_by_id(&mut conn, document_id).await
    }

    async fn find_all_paginated(&self, limit: usize) -> Result<Vec<DocumentEntity>> {
        let mut conn = self.pool.acquire().await?;
        ops::find_all_paginated(&mut conn, limit).await
    }
}

impl crate::application::ports::DocumentRepository for SqliteDocumentRepository {}

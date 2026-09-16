//! Document Repository Implementation
//!
//! SQLite pool-based implementation of DocumentRepository that delegates to ops.rs.

use crate::application::ports::{DocumentRepositoryPort, Filter, RepositoryPort};
use crate::domain::entities::Document as DocumentEntity;
use crate::infrastructure::persistence::mappers::DocumentModel;
use crate::shared::error::{AppError, Result};
use async_trait::async_trait;
use sqlx::SqlitePool;

use super::ops;

#[derive(Debug, Clone)]
pub struct DocumentFilter {
    pub path_pattern: Option<String>,
    pub status: Option<String>,
    pub limit: Option<usize>,
}

impl Filter for DocumentFilter {
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
pub struct SqliteDocumentRepository {
    pool: SqlitePool,
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
        let entity = self.find_by_path(file_path).await?;

        match entity {
            Some(doc_entity) => {
                let mut conn = self.pool.acquire().await?;
                let chunks =
                    ops::fetch_chunks_for_document(&mut conn, doc_entity.id().as_str()).await?;
                let tags =
                    ops::fetch_tags_for_document(&mut conn, doc_entity.id().as_str()).await?;
                let aggregate = doc_entity.from_parts(chunks, tags)?;
                Ok(Some(aggregate))
            }
            None => Ok(None),
        }
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
        let mut conn = self.pool.acquire().await?;
        ops::find_by_id(&mut conn, id).await
    }

    async fn find_by_filter(&self, filter: &dyn Filter) -> Result<Vec<DocumentEntity>> {
        filter.validate()?;

        let filter_any = filter.as_any();
        if let Some(doc_filter) = filter_any.downcast_ref::<DocumentFilter>() {
            let path_pattern = doc_filter.path_pattern.clone();
            let status = doc_filter.status.clone();
            let limit = doc_filter.limit.unwrap_or(1000);

            let mut query = String::from(
                "SELECT id, file_path, file_name, file_type, mime_type, size_bytes,
                 modified_at, indexed_at, checksum, status, language, category,
                 quality_score, access_count, last_accessed_at, word_count FROM documents WHERE 1=1",
            );

            if path_pattern.is_some() {
                query.push_str(" AND file_path LIKE ?");
            }
            if status.is_some() {
                query.push_str(" AND status = ?");
            }
            query.push_str(" ORDER BY indexed_at DESC LIMIT ?");

            let mut query_builder = sqlx::query_as::<_, DocumentModel>(&query);

            if let Some(pattern) = &path_pattern {
                query_builder = query_builder.bind(pattern);
            }
            if let Some(s) = &status {
                query_builder = query_builder.bind(s);
            }
            query_builder = query_builder.bind(limit as i64);

            let mut conn = self.pool.acquire().await?;
            let db_models = query_builder.fetch_all(&mut *conn).await.map_err(|e| {
                AppError::Database(format!("Failed to find documents by filter: {}", e))
            })?;

            Ok(
                crate::infrastructure::persistence::mappers::DocumentMapper::to_entities(
                    &db_models,
                ),
            )
        } else {
            self.find_all().await
        }
    }

    async fn find_all(&self) -> Result<Vec<DocumentEntity>> {
        let mut conn = self.pool.acquire().await?;
        ops::find_all(&mut conn).await
    }

    async fn save(&self, entity: &DocumentEntity) -> Result<()> {
        let mut conn = self.pool.acquire().await?;
        ops::save(&mut conn, entity).await
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
        let mut conn = self.pool.acquire().await?;
        ops::find_by_checksum(&mut conn, checksum).await
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

    async fn find_all_paginated(&self, limit: usize) -> Result<Vec<DocumentEntity>> {
        let mut conn = self.pool.acquire().await?;
        ops::find_all_paginated(&mut conn, limit).await
    }
}

impl crate::application::ports::DocumentRepository for SqliteDocumentRepository {}

//! Chunk Repository Implementation
//!
//! Infrastructure implementation for text chunk persistence using SQLite.
//!
//! # Architecture
//!
//! This repository implements the `RepositoryPort<ChunkEntity>` trait,
//! providing CRUD operations for chunks using SQLite. All database
//! operations go through the ChunkMapper layer to maintain clean
//! separation between domain entities and database models.
//!
//! # Migration Notes
//!
//! - DB models are now internal and NOT exported

use crate::infrastructure::persistence::mappers::{ChunkMapper, ChunkModel};
use async_trait::async_trait;
use lattice_core::application::ports::{ChunkRepositoryPort, Filter, RepositoryPort};
use lattice_core::domain::entities::chunk::Chunk as ChunkEntity;
use lattice_core::shared::error::{AppError, Result};
use sqlx::{Row, SqlitePool};
use std::collections::HashMap;

/// Filter for querying chunks by document ID.
#[derive(Debug, Clone)]
pub struct ChunkFilter {
    /// Filter by document ID
    pub document_id: Option<String>,
    /// Limit number of results
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

/// SQLite implementation of chunk repository.
///
/// Handles persistence of text chunks in SQLite database.
/// Uses ChunkMapper to convert between domain entities and database models.
///
/// # Thread Safety
///
/// This repository is thread-safe through SQLitePool's internal connection management.
#[derive(Debug, Clone)]
pub struct ChunkRepository {
    pool: SqlitePool,
}

impl ChunkRepository {
    /// Create a new chunk repository.
    ///
    /// # Arguments
    ///
    /// * `pool` - SQLite connection pool
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Find chunks by document ID.
    ///
    /// # Arguments
    ///
    /// * `document_id` - Document ID
    ///
    /// # Returns
    ///
    /// Vector of chunk entities ordered by chunk_index ascending.
    ///
    /// # Errors
    ///
    /// - `AppError::Database` if query fails
    pub async fn find_by_document(&self, document_id: &str) -> Result<Vec<ChunkEntity>> {
        // SQLite reads a negative LIMIT as no limit.
        self.find_first_by_document_inner(document_id, -1).await
    }

    async fn find_first_by_document_inner(
        &self,
        document_id: &str,
        limit: i64,
    ) -> Result<Vec<ChunkEntity>> {
        let db_models = sqlx::query_as::<_, ChunkModel>(
            r#"
            SELECT
                id,
                document_id,
                content,
                chunk_index,
                contextualized_content,
                context_prefix,
                start_char,
                end_char,
                language,
                token_count,
                word_count,
                has_code,
                section, page_number, from_ocr
            FROM text_chunks
            WHERE document_id = ?
            ORDER BY chunk_index ASC
            LIMIT ?
            "#,
        )
        .bind(document_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AppError::Database(format!("Failed to find chunks by document: {}", e)))?;

        ChunkMapper::to_entities(&db_models)
    }

    /// Delete all chunks for a document.
    ///
    /// # Arguments
    ///
    /// * `document_id` - Document ID
    ///
    /// # Errors
    ///
    /// - `AppError::Database` if deletion fails
    pub async fn delete_by_document(&self, document_id: &str) -> Result<()> {
        sqlx::query("DELETE FROM text_chunks WHERE document_id = ?")
            .bind(document_id)
            .execute(&self.pool)
            .await
            .map_err(|e| AppError::Database(format!("Failed to delete chunks: {}", e)))?;
        Ok(())
    }

    /// Count chunks for a document.
    ///
    /// # Arguments
    ///
    /// * `document_id` - Document ID
    ///
    /// # Returns
    ///
    /// Total count of chunks for the document.
    ///
    /// # Errors
    ///
    /// - `AppError::Database` if count query fails
    pub async fn count_by_document(&self, document_id: &str) -> Result<usize> {
        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM text_chunks WHERE document_id = ?")
                .bind(document_id)
                .fetch_one(&self.pool)
                .await
                .map_err(|e| AppError::Database(format!("Failed to count chunks: {}", e)))?;
        Ok(count as usize)
    }
}

#[async_trait]
impl RepositoryPort<ChunkEntity> for ChunkRepository {
    async fn find_by_id(&self, id: &str) -> Result<Option<ChunkEntity>> {
        let row = sqlx::query_as::<_, ChunkModel>(
            r#"
            SELECT
                id,
                document_id,
                content,
                chunk_index,
                contextualized_content,
                context_prefix,
                start_char,
                end_char,
                language,
                token_count,
                word_count,
                has_code,
                section, page_number, from_ocr
            FROM text_chunks
            WHERE id = ?
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AppError::Database(format!("Failed to find chunk by id: {}", e)))?;

        match row {
            Some(model) => Ok(Some(ChunkMapper::to_entity(&model)?)),
            None => Ok(None),
        }
    }

    async fn find_by_filter(&self, filter: &dyn Filter) -> Result<Vec<ChunkEntity>> {
        filter.validate()?;

        // Downcast to ChunkFilter if possible
        let filter_any = filter.as_any();
        if let Some(chunk_filter) = filter_any.downcast_ref::<ChunkFilter>() {
            if let Some(document_id) = &chunk_filter.document_id {
                let limit = chunk_filter
                    .limit
                    .map(i64::try_from)
                    .transpose()
                    .map_err(|_| AppError::InvalidInput("Chunk limit exceeds SQLite range".into()))?
                    .unwrap_or(-1);
                return self.find_first_by_document_inner(document_id, limit).await;
            }
        }

        // If no specific filter, return empty (chunks are always tied to documents)
        Ok(vec![])
    }

    async fn find_all(&self) -> Result<Vec<ChunkEntity>> {
        let db_models = sqlx::query_as::<_, ChunkModel>(
            r#"
            SELECT
                id,
                document_id,
                content,
                chunk_index,
                contextualized_content,
                context_prefix,
                start_char,
                end_char,
                language,
                token_count,
                word_count,
                has_code,
                section, page_number, from_ocr
            FROM text_chunks
            ORDER BY document_id, chunk_index ASC
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AppError::Database(format!("Failed to list all chunks: {}", e)))?;

        ChunkMapper::to_entities(&db_models)
    }

    async fn save(&self, entity: &ChunkEntity) -> Result<()> {
        let model = ChunkMapper::to_model(entity)?;

        sqlx::query(
            r#"
            INSERT INTO text_chunks (
                id, document_id, content, chunk_index,
                contextualized_content, context_prefix, start_char, end_char,
                language, token_count, word_count, has_code, section, page_number, from_ocr
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(id) DO UPDATE SET
                content = excluded.content,
                chunk_index = excluded.chunk_index,
                contextualized_content = excluded.contextualized_content,
                context_prefix = excluded.context_prefix,
                start_char = excluded.start_char,
                end_char = excluded.end_char,
                language = excluded.language,
                token_count = excluded.token_count,
                word_count = excluded.word_count,
                has_code = excluded.has_code,
                section = excluded.section, page_number = excluded.page_number,
                from_ocr = excluded.from_ocr
            "#,
        )
        .bind(&model.id)
        .bind(&model.document_id)
        .bind(&model.content)
        .bind(model.chunk_index)
        .bind(&model.contextualized_content)
        .bind(&model.context_prefix)
        .bind(model.start_char)
        .bind(model.end_char)
        .bind(&model.language)
        .bind(model.token_count)
        .bind(model.word_count)
        .bind(model.has_code)
        .bind(&model.section)
        .bind(model.page_number)
        .bind(model.from_ocr)
        .execute(&self.pool)
        .await
        .map_err(|e| AppError::Database(format!("Failed to save chunk: {}", e)))?;

        Ok(())
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

        crate::infrastructure::persistence::repositories::chunk::ops::save_batch_optimized(
            &mut tx, entities,
        )
        .await?;

        tx.commit()
            .await
            .map_err(|e| AppError::Database(format!("Failed to commit transaction: {}", e)))?;

        Ok(())
    }

    async fn delete(&self, id: &str) -> Result<()> {
        sqlx::query("DELETE FROM text_chunks WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| AppError::Database(format!("Failed to delete chunk: {}", e)))?;
        Ok(())
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
            sqlx::query("DELETE FROM text_chunks WHERE id = ?")
                .bind(id)
                .execute(&mut *tx)
                .await
                .map_err(|e| {
                    AppError::Database(format!("Failed to delete chunk in batch: {}", e))
                })?;
        }

        tx.commit()
            .await
            .map_err(|e| AppError::Database(format!("Failed to commit transaction: {}", e)))?;

        Ok(())
    }

    async fn count(&self) -> Result<usize> {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM text_chunks")
            .fetch_one(&self.pool)
            .await
            .map_err(|e| AppError::Database(format!("Failed to count chunks: {}", e)))?;
        Ok(count as usize)
    }

    async fn exists(&self, id: &str) -> Result<bool> {
        sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM text_chunks WHERE id = ?)")
            .bind(id)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| AppError::Database(format!("Failed to check chunk existence: {}", e)))
    }
}

/// Implementation of ChunkRepositoryPort for ChunkRepository.
///
/// Provides chunk-specific repository operations beyond generic CRUD.
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
        self.save(&chunk).await?;
        Ok(chunk)
    }

    async fn find_by_document(&self, document_id: &str) -> Result<Vec<ChunkEntity>> {
        // Delegate to inherent impl to avoid trait-recursion
        ChunkRepository::find_by_document(self, document_id).await
    }

    async fn find_first_by_document(
        &self,
        document_id: &str,
        limit: usize,
    ) -> Result<Vec<ChunkEntity>> {
        let limit = i64::try_from(limit).unwrap_or(i64::MAX);
        self.find_first_by_document_inner(document_id, limit).await
    }

    async fn find_by_ids(&self, chunk_ids: &[String]) -> Result<Vec<ChunkEntity>> {
        if chunk_ids.is_empty() {
            return Ok(Vec::new());
        }

        const BATCH_SIZE: usize = 900; // SQLite parameter limit safety
        let mut all = Vec::new();

        for batch in chunk_ids.chunks(BATCH_SIZE) {
            let placeholders = batch.iter().map(|_| "?").collect::<Vec<_>>().join(", ");
            let query = format!(
                r#"
                SELECT
                    id,
                    document_id,
                    content,
                    chunk_index,
                    contextualized_content,
                    context_prefix,
                    start_char,
                    end_char,
                    language,
                    token_count,
                    word_count,
                    has_code,
                    section, page_number, from_ocr
                FROM text_chunks
                WHERE id IN ({})
                "#,
                placeholders
            );

            let mut query_builder = sqlx::query_as::<_, ChunkModel>(&query);
            for id in batch {
                query_builder = query_builder.bind(id);
            }

            let db_models = query_builder
                .fetch_all(&self.pool)
                .await
                .map_err(|e| AppError::Database(format!("Failed to find chunks by ids: {}", e)))?;
            all.extend(ChunkMapper::to_entities(&db_models)?);
        }

        Ok(all)
    }

    async fn find_content_by_ids(&self, chunk_ids: &[String]) -> Result<HashMap<String, String>> {
        const BATCH: usize = 400;
        let mut content = HashMap::with_capacity(chunk_ids.len());
        for ids in chunk_ids.chunks(BATCH) {
            let mut query = sqlx::QueryBuilder::<sqlx::Sqlite>::new(
                "SELECT id, content FROM text_chunks WHERE id IN (",
            );
            let mut separated = query.separated(", ");
            for id in ids {
                separated.push_bind(id);
            }
            query.push(")");
            let rows = query.build().fetch_all(&self.pool).await.map_err(|error| {
                AppError::Database(format!("Failed to fetch candidate chunk content: {error}"))
            })?;
            for row in rows {
                let id: String = row
                    .try_get("id")
                    .map_err(|error| AppError::Database(format!("row.id: {error}")))?;
                let body: String = row
                    .try_get("content")
                    .map_err(|error| AppError::Database(format!("row.content: {error}")))?;
                content.insert(id, body);
            }
        }
        Ok(content)
    }

    async fn find_conversation_attached_chunk_ids(&self) -> Result<Vec<String>> {
        sqlx::query_scalar(
            "SELECT tc.id FROM text_chunks tc \
             JOIN documents d ON d.id = tc.document_id \
             WHERE d.owner_conversation_id IS NOT NULL",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| {
            AppError::Database(format!(
                "Failed to list conversation attachment chunks: {e}"
            ))
        })
    }

    async fn delete_by_document(&self, document_id: &str) -> Result<()> {
        // Delegate to inherent impl to avoid trait-recursion
        ChunkRepository::delete_by_document(self, document_id).await
    }

    async fn count_all(&self) -> Result<i64> {
        sqlx::query_scalar("SELECT COUNT(*) FROM text_chunks")
            .fetch_one(&self.pool)
            .await
            .map_err(|e| AppError::Database(format!("Failed to count all chunks: {}", e)))
    }

    async fn count_indexed_documents(&self) -> Result<i64> {
        sqlx::query_scalar("SELECT COUNT(DISTINCT document_id) FROM text_chunks")
            .fetch_one(&self.pool)
            .await
            .map_err(|e| AppError::Database(format!("Failed to count indexed documents: {}", e)))
    }

    async fn count_by_document(&self, document_id: &str) -> Result<i64> {
        sqlx::query_scalar("SELECT COUNT(*) FROM text_chunks WHERE document_id = ?")
            .bind(document_id)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| AppError::Database(format!("Failed to count chunks by document: {}", e)))
    }

    async fn create_batch(&self, chunks: Vec<ChunkEntity>) -> Result<Vec<ChunkEntity>> {
        self.save_batch(&chunks).await?;
        Ok(chunks)
    }
}

#[cfg(test)]
mod tests;

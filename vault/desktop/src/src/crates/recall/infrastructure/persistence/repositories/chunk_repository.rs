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
//! - Phase 1: Created domain entities and mappers
//! - Phase 2: Implemented `RepositoryPort<ChunkEntity>` (current)
//! - DB models are now internal and NOT exported

use crate::application::ports::{ChunkRepositoryPort, Filter, RepositoryPort};
use crate::domain::entities::chunk::Chunk as ChunkEntity;
use crate::infrastructure::persistence::mappers::{ChunkMapper, ChunkModel};
use crate::shared::error::{AppError, Result};
use async_trait::async_trait;
use sqlx::{Row, SqlitePool};

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
        let rows = sqlx::query(
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
                section
            FROM text_chunks
            WHERE document_id = ?
            ORDER BY chunk_index ASC
            "#,
        )
        .bind(document_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AppError::Database(format!("Failed to find chunks by document: {}", e)))?;

        let db_models: Vec<ChunkModel> = rows
            .iter()
            .map(|row| ChunkModel {
                id: row.get("id"),
                document_id: row.get("document_id"),
                content: row.get("content"),
                chunk_index: row.get("chunk_index"),
                contextualized_content: row.get("contextualized_content"),
                context_prefix: row.get("context_prefix"),
                start_char: row.get("start_char"),
                end_char: row.get("end_char"),
                language: row.get("language"),
                token_count: row.get("token_count"),
                word_count: row.get("word_count"),
                has_code: row.get("has_code"),
                section: row.get("section"),
            })
            .collect();

        Ok(ChunkMapper::to_entities(&db_models))
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
        let row = sqlx::query(
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
                section
            FROM text_chunks
            WHERE id = ?
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AppError::Database(format!("Failed to find chunk by id: {}", e)))?;

        match row {
            Some(r) => {
                let model = ChunkModel {
                    id: r.get("id"),
                    document_id: r.get("document_id"),
                    content: r.get("content"),
                    chunk_index: r.get("chunk_index"),
                    contextualized_content: r.get("contextualized_content"),
                    context_prefix: r.get("context_prefix"),
                    start_char: r.get("start_char"),
                    end_char: r.get("end_char"),
                    language: r.get("language"),
                    token_count: r.get("token_count"),
                    word_count: r.get("word_count"),
                    has_code: r.get("has_code"),
                    section: r.get("section"),
                };
                Ok(Some(ChunkMapper::to_entity(&model)?))
            }
            None => Ok(None),
        }
    }

    async fn find_by_filter(&self, filter: &dyn Filter) -> Result<Vec<ChunkEntity>> {
        filter.validate()?;

        // Downcast to ChunkFilter if possible
        let filter_any = filter.as_any();
        if let Some(chunk_filter) = filter_any.downcast_ref::<ChunkFilter>() {
            if let Some(document_id) = &chunk_filter.document_id {
                return self.find_by_document(document_id).await;
            }
        }

        // If no specific filter, return empty (chunks are always tied to documents)
        Ok(vec![])
    }

    async fn find_all(&self) -> Result<Vec<ChunkEntity>> {
        let rows = sqlx::query(
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
                section
            FROM text_chunks
            ORDER BY document_id, chunk_index ASC
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AppError::Database(format!("Failed to list all chunks: {}", e)))?;

        let db_models: Vec<ChunkModel> = rows
            .iter()
            .map(|row| ChunkModel {
                id: row.get("id"),
                document_id: row.get("document_id"),
                content: row.get("content"),
                chunk_index: row.get("chunk_index"),
                contextualized_content: row.get("contextualized_content"),
                context_prefix: row.get("context_prefix"),
                start_char: row.get("start_char"),
                end_char: row.get("end_char"),
                language: row.get("language"),
                token_count: row.get("token_count"),
                word_count: row.get("word_count"),
                has_code: row.get("has_code"),
                section: row.get("section"),
            })
            .collect();

        Ok(ChunkMapper::to_entities(&db_models))
    }

    async fn save(&self, entity: &ChunkEntity) -> Result<()> {
        // Convert entity to DB model
        let model = ChunkMapper::to_model(entity);

        sqlx::query(
            r#"
            INSERT INTO text_chunks (
                id, document_id, content, chunk_index,
                contextualized_content, context_prefix, start_char, end_char,
                language, token_count, word_count, has_code, section
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
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
                section = excluded.section
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

        for entity in entities {
            let model = ChunkMapper::to_model(entity);

            sqlx::query(
                r#"
                INSERT INTO text_chunks (
                    id, document_id, content, chunk_index,
                    contextualized_content, context_prefix, start_char, end_char,
                    language, token_count, word_count, has_code, section
                )
                VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
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
                    section = excluded.section
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
            .execute(&mut *tx)
            .await
            .map_err(|e| AppError::Database(format!("Failed to save chunk in batch: {}", e)))?;
        }

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
        use crate::shared::domain_types::DocumentId;
        let doc_id = DocumentId::from(document_id.to_string());
        let chunk = ChunkEntity::new(doc_id, content.to_string(), index);
        self.save(&chunk).await?;
        Ok(chunk)
    }

    async fn find_by_document(&self, document_id: &str) -> Result<Vec<ChunkEntity>> {
        // Delegate to inherent impl to avoid trait-recursion
        ChunkRepository::find_by_document(self, document_id).await
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
                    section
                FROM text_chunks
                WHERE id IN ({})
                "#,
                placeholders
            );

            let mut query_builder = sqlx::query(&query);
            for id in batch {
                query_builder = query_builder.bind(id);
            }

            let rows = query_builder
                .fetch_all(&self.pool)
                .await
                .map_err(|e| AppError::Database(format!("Failed to find chunks by ids: {}", e)))?;

            let db_models: Vec<ChunkModel> = rows
                .iter()
                .map(|row| ChunkModel {
                    id: row.get("id"),
                    document_id: row.get("document_id"),
                    content: row.get("content"),
                    chunk_index: row.get("chunk_index"),
                    contextualized_content: row.get("contextualized_content"),
                    context_prefix: row.get("context_prefix"),
                    start_char: row.get("start_char"),
                    end_char: row.get("end_char"),
                    language: row.get("language"),
                    token_count: row.get("token_count"),
                    word_count: row.get("word_count"),
                    has_code: row.get("has_code"),
                    section: row.get("section"),
                })
                .collect();

            all.extend(ChunkMapper::to_entities(&db_models));
        }

        Ok(all)
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
mod tests {
    use super::*;
    use crate::domain_types::DocumentId;
    use sqlx::sqlite::SqlitePoolOptions;

    async fn create_test_pool() -> SqlitePool {
        SqlitePoolOptions::new().connect(":memory:").await.unwrap()
    }

    async fn setup_schema(pool: &SqlitePool) {
        sqlx::query(
            r#"
            CREATE TABLE text_chunks (
                id TEXT PRIMARY KEY,
                document_id TEXT NOT NULL,
                content TEXT NOT NULL,
                chunk_index INTEGER NOT NULL,
                contextualized_content TEXT,
                context_prefix TEXT,
                start_char INTEGER,
                end_char INTEGER,
                created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
            )
            "#,
        )
        .execute(pool)
        .await
        .unwrap();
    }

    #[ignore] // TODO: Fix in Quality Phase - Oracle Phase 3 quarantine
    #[tokio::test]

    async fn test_save_and_find_by_id() {
        let pool = create_test_pool().await;
        setup_schema(&pool).await;
        let repo = ChunkRepository::new(pool);

        let doc_id = DocumentId::new();
        let entity = ChunkEntity::new(doc_id.clone(), "Test chunk content".to_string(), 0);

        repo.save(&entity).await.unwrap();

        let found = repo.find_by_id(entity.id().as_str()).await.unwrap();
        assert!(found.is_some());

        let found_entity = found.unwrap();
        assert_eq!(found_entity.id().as_str(), entity.id().as_str());
        assert_eq!(found_entity.content(), "Test chunk content");
        assert_eq!(found_entity.index(), 0);
    }

    #[ignore] // TODO: Fix in Quality Phase - Oracle Phase 3 quarantine
    #[tokio::test]

    async fn test_find_by_document() {
        let pool = create_test_pool().await;
        setup_schema(&pool).await;
        let repo = ChunkRepository::new(pool);

        let doc_id = DocumentId::new();
        let chunk1 = ChunkEntity::new(doc_id.clone(), "Chunk 1".to_string(), 0);
        let chunk2 = ChunkEntity::new(doc_id.clone(), "Chunk 2".to_string(), 1);

        repo.save(&chunk1).await.unwrap();
        repo.save(&chunk2).await.unwrap();

        let chunks = repo.find_by_document(doc_id.as_str()).await.unwrap();
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].content(), "Chunk 1");
        assert_eq!(chunks[1].content(), "Chunk 2");
    }

    #[ignore] // TODO: Fix in Quality Phase - Oracle Phase 3 quarantine
    #[tokio::test]

    async fn test_save_batch() {
        let pool = create_test_pool().await;
        setup_schema(&pool).await;
        let repo = ChunkRepository::new(pool);

        let doc_id = DocumentId::new();
        let entities = vec![
            ChunkEntity::new(doc_id.clone(), "Chunk 1".to_string(), 0),
            ChunkEntity::new(doc_id.clone(), "Chunk 2".to_string(), 1),
            ChunkEntity::new(doc_id.clone(), "Chunk 3".to_string(), 2),
        ];

        repo.save_batch(&entities).await.unwrap();

        let count = repo.count().await.unwrap();
        assert_eq!(count, 3);
    }

    #[ignore] // TODO: Fix in Quality Phase - Oracle Phase 3 quarantine
    #[tokio::test]

    async fn test_delete() {
        let pool = create_test_pool().await;
        setup_schema(&pool).await;
        let repo = ChunkRepository::new(pool);

        let doc_id = DocumentId::new();
        let entity = ChunkEntity::new(doc_id, "Test chunk".to_string(), 0);

        repo.save(&entity).await.unwrap();
        assert_eq!(repo.count().await.unwrap(), 1);

        repo.delete(entity.id().as_str()).await.unwrap();
        assert_eq!(repo.count().await.unwrap(), 0);
    }

    #[ignore] // TODO: Fix in Quality Phase - Oracle Phase 3 quarantine
    #[tokio::test]

    async fn test_delete_by_document() {
        let pool = create_test_pool().await;
        setup_schema(&pool).await;
        let repo = ChunkRepository::new(pool);

        let doc_id = DocumentId::new();
        let chunks = vec![
            ChunkEntity::new(doc_id.clone(), "Chunk 1".to_string(), 0),
            ChunkEntity::new(doc_id.clone(), "Chunk 2".to_string(), 1),
        ];

        repo.save_batch(&chunks).await.unwrap();
        assert_eq!(repo.count().await.unwrap(), 2);

        repo.delete_by_document(doc_id.as_str()).await.unwrap();
        assert_eq!(repo.count().await.unwrap(), 0);
    }

    #[ignore] // TODO: Fix in Quality Phase - Oracle Phase 3 quarantine
    #[tokio::test]

    async fn test_count_by_document() {
        let pool = create_test_pool().await;
        setup_schema(&pool).await;
        let repo = ChunkRepository::new(pool);

        let doc_id = DocumentId::new();
        let chunks = vec![
            ChunkEntity::new(doc_id.clone(), "Chunk 1".to_string(), 0),
            ChunkEntity::new(doc_id.clone(), "Chunk 2".to_string(), 1),
            ChunkEntity::new(doc_id.clone(), "Chunk 3".to_string(), 2),
        ];

        repo.save_batch(&chunks).await.unwrap();

        let count = repo.count_by_document(doc_id.as_str()).await.unwrap();
        assert_eq!(count, 3);
    }

    #[ignore] // TODO: Fix in Quality Phase - Oracle Phase 3 quarantine
    #[tokio::test]

    async fn test_exists() {
        let pool = create_test_pool().await;
        setup_schema(&pool).await;
        let repo = ChunkRepository::new(pool);

        let doc_id = DocumentId::new();
        let entity = ChunkEntity::new(doc_id, "Test chunk".to_string(), 0);

        assert!(!repo.exists(entity.id().as_str()).await.unwrap());

        repo.save(&entity).await.unwrap();

        assert!(repo.exists(entity.id().as_str()).await.unwrap());
    }
}

//! Document Repository Implementation
//!
//! Infrastructure implementation for document persistence using SQLite.
//!
//! # Architecture
//!
//! This repository implements both:
//! - `RepositoryPort<DocumentEntity>` - Generic CRUD operations for entity
//! - `RepositoryPort<Document>` - Aggregate operations (entity + chunks + tags)
//! - `DocumentRepositoryPort` - Document-specific operations (path lookups)
//!
//! All database operations go through the DocumentMapper layer to maintain clean
//! separation between domain entities and database models.
//!
//! # Migration Notes
//!
//! - Phase 1: Created domain entities and mappers
//! - Phase 2: Implemented `RepositoryPort<DocumentEntity>`
//! - Phase 3: Implemented DocumentRepositoryPort for file operations
//! - Phase 4: Implemented `RepositoryPort<Document>` for DDD compliance (current)
//! - DB models are now internal and NOT exported

use crate::application::ports::{DocumentRepositoryPort, Filter, RepositoryPort};
use crate::domain::entities::chunk::Chunk;
use crate::domain::entities::Document;
use crate::domain::entities::Document as DocumentEntity;
use crate::infrastructure::persistence::database::{query_with_quick_timeout, query_with_timeout};
use crate::infrastructure::persistence::mappers::{
    ChunkMapper, ChunkModel, DocumentMapper, DocumentModel,
};
use crate::shared::domain_types::TagId;
use crate::shared::error::{AppError, Result};
use async_trait::async_trait;
use sqlx::{QueryBuilder, Row, Sqlite, SqlitePool};

/// Filter for querying documents by various criteria.
#[derive(Debug, Clone)]
pub struct DocumentFilter {
    /// Filter by file path pattern (SQL LIKE)
    pub path_pattern: Option<String>,
    /// Filter by status
    pub status: Option<String>,
    /// Limit number of results
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

/// SQLite implementation of document repository.
///
/// Handles persistence of document metadata in SQLite database.
/// Uses DocumentMapper to convert between domain entities and database models.
///
/// # Thread Safety
///
/// This repository is thread-safe through SQLitePool's internal connection management.
#[derive(Debug, Clone)]
pub struct DocumentRepository {
    pool: SqlitePool,
}

impl DocumentRepository {
    /// Create a new document repository.
    ///
    /// # Arguments
    ///
    /// * `pool` - SQLite connection pool
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Find a document by its file path.
    ///
    /// # Arguments
    ///
    /// * `file_path` - Full file path
    ///
    /// # Returns
    ///
    /// `Some(DocumentEntity)` if found, `None` if not found.
    ///
    /// # Errors
    ///
    /// - `AppError::Database` if query fails
    /// - `AppError::InvalidData` if DB model cannot be converted to domain entity
    pub async fn find_by_path(&self, file_path: &str) -> Result<Option<DocumentEntity>> {
        let pool = self.pool.clone();
        let file_path = file_path.to_string();

        let db_model = query_with_quick_timeout(|| async {
            sqlx::query_as::<_, DocumentModel>(
                r#"
                SELECT
                    id, file_path, file_name, file_type, mime_type,
                    size_bytes, modified_at, indexed_at, checksum, status,
                    language, category, quality_score, access_count,
                    last_accessed_at, word_count
                FROM documents
                WHERE file_path = ?
                "#
            )
            .bind(file_path.clone())
            .fetch_optional(&pool)
            .await
        })
        .await
        .map_err(|e| {
            AppError::Database(format!(
                "Failed to find document at path '{}'. File may not be indexed or database may be corrupted. Error: {}",
                file_path, e
            ))
        })?;

        match db_model {
            Some(model) => Ok(Some(DocumentMapper::to_entity(&model)?)),
            None => Ok(None),
        }
    }

    /// Find document aggregate by exact file path match.
    ///
    /// Uses exact equality (=) instead of pattern matching (LIKE) for performance
    /// and correctness in duplicate detection.
    ///
    /// # Arguments
    ///
    /// * `file_path` - Full file path to search for (exact match)
    ///
    /// # Returns
    ///
    /// `Some(Document)` if found with exact path match, `None` otherwise.
    ///
    /// # Errors
    ///
    /// - `AppError::Database` if query fails
    pub async fn find_aggregate_by_path_exact(&self, file_path: &str) -> Result<Option<Document>> {
        // First, find the document entity by exact path
        let entity = self.find_by_path(file_path).await?;

        match entity {
            Some(doc_entity) => {
                // Convert entity to aggregate Document
                let document = Self::entity_to_aggregate_document(&doc_entity)?;

                // Fetch chunks for this document
                let chunks = self
                    .fetch_chunks_for_document(doc_entity.id().as_str())
                    .await?;

                // Fetch tags for this document
                let tags = self
                    .fetch_tags_for_document(doc_entity.id().as_str())
                    .await?;

                // Reconstruct complete aggregate
                let aggregate = Document::from_parts(document, chunks, tags)?;

                Ok(Some(aggregate))
            }
            None => Ok(None),
        }
    }

    /// Find documents by file path pattern.
    ///
    /// # Arguments
    ///
    /// * `pattern` - SQL LIKE pattern (e.g., "/path/to/%")
    ///
    /// # Returns
    ///
    /// Vector of matching document entities ordered by file_path descending.
    ///
    /// # Errors
    ///
    /// - `AppError::Database` if query fails
    pub async fn find_by_path_pattern(&self, pattern: &str) -> Result<Vec<DocumentEntity>> {
        let pool = self.pool.clone();
        let pattern = pattern.to_string();

        let db_models: Vec<DocumentModel> = query_with_timeout(|| async {
            sqlx::query_as::<_, DocumentModel>(
                r#"
                SELECT
                    id, file_path, file_name, file_type, mime_type,
                    size_bytes, modified_at, indexed_at, checksum, status,
                    language, category, quality_score, access_count,
                    last_accessed_at, word_count
                FROM documents
                WHERE file_path LIKE ?
                ORDER BY file_path DESC
                "#,
            )
            .bind(pattern.clone())
            .fetch_all(&pool)
            .await
        })
        .await
        .map_err(|e| AppError::Database(format!("Failed to find documents by pattern: {}", e)))?;

        Ok(DocumentMapper::to_entities(&db_models))
    }

    /// Check if a document exists by file path.
    ///
    /// # Arguments
    ///
    /// * `file_path` - Full file path
    ///
    /// # Returns
    ///
    /// `true` if document exists, `false` otherwise.
    ///
    /// # Errors
    ///
    /// - `AppError::Database` if query fails
    pub async fn exists_by_path(&self, file_path: &str) -> Result<bool> {
        let pool = self.pool.clone();
        let file_path = file_path.to_string();

        query_with_quick_timeout(|| async {
            sqlx::query_scalar::<_, bool>(
                "SELECT EXISTS(SELECT 1 FROM documents WHERE file_path = ?)",
            )
            .bind(file_path.clone())
            .fetch_one(&pool)
            .await
        })
        .await
        .map_err(|e| AppError::Database(format!("Failed to check document existence: {}", e)))
    }

    /// Get the total count of indexed documents.
    ///
    /// # Returns
    ///
    /// Total number of documents in the database as `i64`.
    ///
    /// # Errors
    ///
    /// - `AppError::Database` if query fails
    pub async fn count_documents(&self) -> Result<i64> {
        let pool = self.pool.clone();

        let count = query_with_quick_timeout(|| async {
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM documents")
                .fetch_one(&pool)
                .await
        })
        .await
        .map_err(|e| AppError::Database(format!("Failed to count documents: {}", e)))?;

        Ok(count)
    }

    /// Get the total count of chunks across all documents.
    ///
    /// # Returns
    ///
    /// Total number of chunks in the database as `i64`.
    ///
    /// # Errors
    ///
    /// - `AppError::Database` if query fails
    pub async fn count_chunks(&self) -> Result<i64> {
        let pool = self.pool.clone();

        let count = query_with_quick_timeout(|| async {
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM text_chunks")
                .fetch_one(&pool)
                .await
        })
        .await
        .map_err(|e| AppError::Database(format!("Failed to count chunks: {}", e)))?;

        Ok(count)
    }

    // ========================================================================
    // Aggregate Repository Helper Methods
    // ========================================================================

    /// Convert DocumentEntity to aggregate Document.
    ///
    /// Converts the anemic DocumentEntity (from database) to the rich
    /// aggregate Document with value objects.
    ///
    /// # Arguments
    ///
    /// * `entity` - Document entity from database
    ///
    /// # Returns
    ///
    /// Aggregate Document with FileMetadata and Checksum value objects.
    ///
    /// # Errors
    ///
    /// - `AppError::InvalidData` if conversion fails
    fn entity_to_aggregate_document(
        entity: &DocumentEntity,
    ) -> Result<crate::domain::entities::document::Document> {
        use crate::domain::entities::document::{
            Document as AggregateDocument, DocumentStatus as AggregateStatus,
        };
        use crate::domain::value_objects::checksum::Checksum;
        use crate::domain::value_objects::file_metadata::FileMetadata;

        // Create FileMetadata value object from entity fields
        let metadata = FileMetadata::new(
            entity.file_name().to_string(),
            entity.mime_type().to_string(),
            entity.size_bytes(),
            *entity.modified_at(), // Dereference to get DateTime<Utc>
        )?;

        // Create Checksum value object from hex string
        let checksum = Checksum::new(entity.checksum().to_string())?;

        // Map status enum
        let status = match entity.status() {
            crate::domain::entities::document::DocumentStatus::Pending => {
                AggregateStatus::Processing
            }
            crate::domain::entities::document::DocumentStatus::Processing => {
                AggregateStatus::Processing
            }
            crate::domain::entities::document::DocumentStatus::Indexed => AggregateStatus::Indexed,
            crate::domain::entities::document::DocumentStatus::Failed => AggregateStatus::Failed,
        };

        // Create aggregate Document via with_id (Oracle: Document is the single source of truth)
        Ok(AggregateDocument::with_id(
            entity.id().clone(),
            entity.validated_file_path().clone(),
            entity.file_name().to_string(),
            entity.file_type().map(|s| s.to_string()),
            entity.mime_type().to_string(),
            entity.size_bytes(),
            *entity.modified_at(),
            *entity.indexed_at(),
            checksum,
            status,
            entity.error_message().map(|s| s.to_string()),
            entity.language().clone(),
            entity.category().clone(),
            entity.quality_score(),
            entity.access_count(),
            entity.last_accessed_at().copied(),
            entity.word_count(),
            entity.content().to_string(),
        ))
    }

    /// Convert aggregate Document to DocumentModel for database persistence.
    ///
    /// Converts the rich aggregate Document with value objects to the
    /// anemic database model.
    ///
    /// # Arguments
    ///
    /// * `doc` - Aggregate Document
    ///
    /// # Returns
    ///
    /// Database model ready for persistence.
    fn aggregate_document_to_model(
        doc: &crate::domain::entities::document::Document,
    ) -> DocumentModel {
        use crate::domain::entities::document::DocumentStatus as AggregateStatus;

        // Map status enum
        let status = match doc.status() {
            AggregateStatus::Processing => "processing".to_string(),
            AggregateStatus::Indexed => "indexed".to_string(),
            AggregateStatus::Pending => "pending".to_string(),
            AggregateStatus::Failed => "failed".to_string(),
        };

        DocumentModel {
            id: doc.id().as_str().to_string(),
            file_path: doc.file_path().display().to_string(),
            file_name: doc.file_name().to_string(),
            file_type: doc.file_type().map(|s| s.to_string()),
            mime_type: doc.mime_type().to_string(),
            size_bytes: doc.size_bytes(),
            modified_at: doc.modified_at().to_rfc3339(),
            indexed_at: doc.indexed_at().to_rfc3339(),
            checksum: doc.checksum().as_str().to_string(), // Use as_str() instead of as_hex()
            status,
            error_message: None, // Aggregate Document doesn't have error_message field
            // Rich metadata fields (convert to SQLite types)
            language: doc.language().to_string(),
            category: doc.category().to_string(),
            quality_score: doc.quality_score() as f64, // Convert f32 -> f64 for SQLite
            access_count: doc.access_count() as i64,   // Convert i32 -> i64 for SQLite
            last_accessed_at: doc.last_accessed_at().map(|dt| dt.to_rfc3339()),
            word_count: doc.word_count() as i64, // Convert i32 -> i64 for SQLite
            content: doc.content().to_string(),  // Oracle Step 1: Content field
        }
    }

    /// Fetch all chunks for a document.
    ///
    /// # Arguments
    ///
    /// * `doc_id` - Document ID
    ///
    /// # Returns
    ///
    /// Vector of chunks ordered by chunk index.
    ///
    /// # Errors
    ///
    /// - `AppError::Database` if query fails
    async fn fetch_chunks_for_document(&self, doc_id: &str) -> Result<Vec<Chunk>> {
        let db_models = sqlx::query_as::<_, ChunkModel>(
            r#"
            SELECT
                id, document_id, content, chunk_index,
                contextualized_content, context_prefix,
                start_char, end_char, language,
                token_count, word_count, has_code, section
            FROM text_chunks
            WHERE document_id = ?
            ORDER BY chunk_index ASC
            "#,
        )
        .bind(doc_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AppError::Database(format!("Failed to fetch chunks for document: {}", e)))?;

        Ok(ChunkMapper::to_entities(&db_models))
    }

    /// Fetch all tag IDs for a document.
    ///
    /// # Arguments
    ///
    /// * `doc_id` - Document ID
    ///
    /// # Returns
    ///
    /// Vector of tag IDs.
    ///
    /// # Errors
    ///
    /// - `AppError::Database` if query fails
    async fn fetch_tags_for_document(&self, doc_id: &str) -> Result<Vec<TagId>> {
        let rows = sqlx::query("SELECT tag_id FROM document_tags WHERE document_id = ?")
            .bind(doc_id)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| AppError::Database(format!("Failed to fetch tags for document: {}", e)))?;

        Ok(rows
            .into_iter()
            .filter_map(|row| {
                let tag_id: String = row.get("tag_id");
                TagId::from_string(tag_id).ok()
            })
            .collect())
    }

    /// Save chunks for a document (transactional).
    ///
    /// Replaces all existing chunks with the provided chunks.
    ///
    /// # Arguments
    ///
    /// * `doc_id` - Document ID
    /// * `chunks` - Chunks to save
    /// * `tx` - Active transaction
    ///
    /// # Errors
    ///
    /// - `AppError::Database` if save fails
    async fn save_chunks_transactional(
        &self,
        doc_id: &str,
        chunks: &[Chunk],
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    ) -> Result<()> {
        // Delete existing chunks
        sqlx::query("DELETE FROM text_chunks WHERE document_id = ?")
            .bind(doc_id)
            .execute(&mut **tx)
            .await
            .map_err(|e| AppError::Database(format!("Failed to delete old chunks: {}", e)))?;

        // Insert new chunks
        for chunk in chunks {
            let model = ChunkMapper::to_model(chunk);

            sqlx::query(
                r#"
                INSERT INTO text_chunks (
                    id, document_id, content, chunk_index,
                    contextualized_content, context_prefix, start_char, end_char
                )
                VALUES (?, ?, ?, ?, ?, ?, ?, ?)
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
            .execute(&mut **tx)
            .await
            .map_err(|e| AppError::Database(format!("Failed to insert chunk: {}", e)))?;
        }

        Ok(())
    }

    /// Save tag relationships for a document (transactional).
    ///
    /// Replaces all existing tag relationships with the provided tags.
    ///
    /// # Arguments
    ///
    /// * `doc_id` - Document ID
    /// * `tags` - Tag IDs to associate
    /// * `tx` - Active transaction
    ///
    /// # Errors
    ///
    /// - `AppError::Database` if save fails
    async fn save_tags_transactional(
        &self,
        doc_id: &str,
        tags: &[TagId],
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    ) -> Result<()> {
        // Delete existing tag relationships
        sqlx::query!("DELETE FROM document_tags WHERE document_id = ?", doc_id)
            .execute(&mut **tx)
            .await
            .map_err(|e| AppError::Database(format!("Failed to delete old tags: {}", e)))?;

        // Insert new tag relationships
        for tag_id in tags {
            let tag_id_str = tag_id.as_str();
            sqlx::query!(
                "INSERT INTO document_tags (document_id, tag_id) VALUES (?, ?)",
                doc_id,
                tag_id_str
            )
            .execute(&mut **tx)
            .await
            .map_err(|e| AppError::Database(format!("Failed to insert tag relationship: {}", e)))?;
        }

        Ok(())
    }

    async fn find_entity_by_id_internal(&self, id: &str) -> Result<Option<DocumentEntity>> {
        let pool = self.pool.clone();
        let id = id.to_string();

        let db_model = query_with_quick_timeout(|| async {
            sqlx::query_as::<_, DocumentModel>(
                r#"
                SELECT
                    id, file_path, file_name, file_type, mime_type,
                    size_bytes, modified_at, indexed_at, checksum, status,
                    language, category, quality_score, access_count,
                    last_accessed_at, word_count
                FROM documents
                WHERE id = ?
                "#,
            )
            .bind(id.clone())
            .fetch_optional(&pool)
            .await
        })
        .await
        .map_err(|e| AppError::Database(format!("Failed to find document by id: {}", e)))?;

        match db_model {
            Some(model) => Ok(Some(DocumentMapper::to_entity(&model)?)),
            None => Ok(None),
        }
    }

    async fn find_all_entities_internal(&self) -> Result<Vec<DocumentEntity>> {
        let pool = self.pool.clone();
        let db_models: Vec<DocumentModel> = query_with_timeout(|| async {
            sqlx::query_as::<_, DocumentModel>(
                r#"
                SELECT
                    id, file_path, file_name, file_type, mime_type,
                    size_bytes, modified_at, indexed_at, checksum, status,
                    language, category, quality_score, access_count,
                    last_accessed_at, word_count
                FROM documents
                ORDER BY indexed_at DESC
                "#,
            )
            .fetch_all(&pool)
            .await
        })
        .await
        .map_err(|e| AppError::Database(format!("Failed to list documents: {}", e)))?;

        Ok(DocumentMapper::to_entities(&db_models))
    }

    async fn find_entities_by_filter_internal(
        &self,
        filter: &dyn Filter,
    ) -> Result<Vec<DocumentEntity>> {
        filter.validate()?;
        let doc_filter = filter
            .as_any()
            .downcast_ref::<DocumentFilter>()
            .ok_or_else(|| AppError::InvalidInput("Expected DocumentFilter".to_string()))?;

        let pool = self.pool.clone();
        let path_pattern = doc_filter.path_pattern.clone();
        let status = doc_filter.status.clone();
        let limit = doc_filter.limit;

        let db_models: Vec<DocumentModel> = query_with_timeout(|| async move {
            let mut qb: QueryBuilder<'_, Sqlite> = QueryBuilder::new(
                r#"
                SELECT
                    id, file_path, file_name, file_type, mime_type,
                    size_bytes, modified_at, indexed_at, checksum, status,
                    language, category, quality_score, access_count,
                    last_accessed_at, word_count
                FROM documents
                WHERE 1=1
                "#,
            );

            if let Some(pattern) = &path_pattern {
                qb.push(" AND file_path LIKE ").push_bind(pattern);
            }

            if let Some(status) = &status {
                qb.push(" AND status = ").push_bind(status);
            }

            qb.push(" ORDER BY indexed_at DESC");

            if let Some(limit) = limit {
                qb.push(" LIMIT ").push_bind(limit as i64);
            }

            qb.build_query_as::<DocumentModel>().fetch_all(&pool).await
        })
        .await
        .map_err(|e| AppError::Database(format!("Failed to filter documents: {}", e)))?;

        Ok(DocumentMapper::to_entities(&db_models))
    }

    async fn exists_internal(&self, id: &str) -> Result<bool> {
        let pool = self.pool.clone();
        let id = id.to_string();

        query_with_quick_timeout(|| async {
            sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM documents WHERE id = ?)")
                .bind(id.clone())
                .fetch_one(&pool)
                .await
        })
        .await
        .map_err(|e| AppError::Database(format!("Failed to check document existence: {}", e)))
    }

    async fn delete_internal(&self, id: &str) -> Result<()> {
        let pool = self.pool.clone();
        let id = id.to_string();

        query_with_timeout(|| async {
            sqlx::query("DELETE FROM documents WHERE id = ?")
                .bind(id.clone())
                .execute(&pool)
                .await
        })
        .await
        .map_err(|e| AppError::Database(format!("Failed to delete document: {}", e)))?;

        Ok(())
    }

    async fn delete_batch_internal(&self, ids: &[&str]) -> Result<()> {
        if ids.is_empty() {
            return Ok(());
        }

        let pool = self.pool.clone();
        let mut tx = pool.begin().await.map_err(|e| {
            AppError::Database(format!("Failed to begin delete batch transaction: {}", e))
        })?;

        for id in ids {
            sqlx::query("DELETE FROM documents WHERE id = ?")
                .bind(*id)
                .execute(&mut *tx)
                .await
                .map_err(|e| {
                    AppError::Database(format!("Failed to delete document in batch: {}", e))
                })?;
        }

        tx.commit().await.map_err(|e| {
            AppError::Database(format!("Failed to commit delete batch transaction: {}", e))
        })?;

        Ok(())
    }
}

/// Implementation of DocumentRepositoryPort for document-specific operations.
///
/// This implementation provides file path lookups and document-file relationship queries.
#[async_trait]
impl DocumentRepositoryPort for DocumentRepository {
    async fn find_file_path_by_id(&self, document_id: &str) -> Result<String> {
        let pool = self.pool.clone();
        let id = document_id.to_string();

        let file_path = query_with_quick_timeout(|| async {
            sqlx::query!("SELECT file_path FROM documents WHERE id = ?", id)
                .fetch_optional(&pool)
                .await
        })
        .await
        .map_err(|e| {
            AppError::Database(format!(
                "Failed to find file path for document '{}': {}",
                document_id, e
            ))
        })?;

        match file_path {
            Some(record) => Ok(record.file_path),
            None => Err(AppError::NotFound(format!(
                "Document not found: {}",
                document_id
            ))),
        }
    }

    async fn rename(&self, document_id: &str, new_file_name: &str) -> Result<()> {
        let pool = self.pool.clone();
        let id = document_id.to_string();
        let name = new_file_name.to_string();

        // Non-macro form: this SQL is new, and the compile-time-checked
        // macros require a regenerated offline cache to change.
        let result = sqlx::query(
            "UPDATE documents SET file_name = ?1, updated_at = CURRENT_TIMESTAMP WHERE id = ?2",
        )
        .bind(&name)
        .bind(&id)
        .execute(&pool)
        .await
        .map_err(|e| {
            AppError::Database(format!(
                "Failed to rename document '{}': {}",
                document_id, e
            ))
        })?;

        if result.rows_affected() == 0 {
            return Err(AppError::NotFound(format!(
                "Document not found: {}",
                document_id
            )));
        }

        Ok(())
    }

    async fn document_exists(&self, document_id: &str) -> Result<bool> {
        self.exists_internal(document_id).await
    }

    async fn find_id_by_path(&self, file_path: &str) -> Result<Option<String>> {
        let pool = self.pool.clone();
        let path = file_path.to_string();

        let record = query_with_quick_timeout(|| async {
            sqlx::query!("SELECT id FROM documents WHERE file_path = ?", path)
                .fetch_optional(&pool)
                .await
        })
        .await
        .map_err(|e| {
            AppError::Database(format!(
                "Failed to find document ID for path '{}': {}",
                file_path, e
            ))
        })?;

        Ok(record.and_then(|r| r.id))
    }

    async fn delete(&self, document_id: &str) -> Result<()> {
        self.delete_internal(document_id).await
    }

    async fn find_by_checksum(
        &self,
        checksum: &crate::domain::value_objects::Checksum,
    ) -> Result<Option<DocumentEntity>> {
        let pool = self.pool.clone();
        let checksum_str = checksum.as_str().to_string();

        let record = query_with_quick_timeout(|| async {
            sqlx::query_as::<_, crate::infrastructure::persistence::mappers::DocumentModel>(
                r#"
                SELECT
                    id,
                    file_path,
                    file_name,
                    file_type,
                    mime_type,
                    size_bytes,
                    modified_at,
                    indexed_at,
                    checksum,
                    status,
                    language,
                    category,
                    quality_score,
                    access_count,
                    last_accessed_at,
                    word_count
                FROM documents
                WHERE checksum = ?
                "#,
            )
            .bind(&checksum_str)
            .fetch_optional(&pool)
            .await
        })
        .await
        .map_err(|e| {
            AppError::Database(format!(
                "Failed to find document by checksum '{}': {}",
                checksum.as_str(),
                e
            ))
        })?;

        match record {
            Some(model) => Ok(Some(
                crate::infrastructure::persistence::mappers::DocumentMapper::to_entity(&model)?,
            )),
            None => Ok(None),
        }
    }

    async fn count_documents(&self) -> Result<i64> {
        let pool = self.pool.clone();
        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM documents")
            .fetch_one(&pool)
            .await?;
        Ok(count.0)
    }

    async fn count_chunks(&self) -> Result<i64> {
        let pool = self.pool.clone();
        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM text_chunks")
            .fetch_one(&pool)
            .await?;
        Ok(count.0)
    }

    async fn find_all_paginated(&self, limit: usize) -> Result<Vec<DocumentEntity>> {
        let pool = self.pool.clone();
        let limit_i64 = limit as i64;

        let db_models: Vec<crate::infrastructure::persistence::mappers::DocumentModel> =
            query_with_quick_timeout(|| async {
                sqlx::query_as::<_, crate::infrastructure::persistence::mappers::DocumentModel>(
                    r#"
                    SELECT
                        id,
                        file_path,
                        file_name,
                        file_type,
                        mime_type,
                        size_bytes,
                        modified_at,
                        indexed_at,
                        checksum,
                        status,
                        language,
                        category,
                        quality_score,
                        access_count,
                        last_accessed_at,
                        word_count
                    FROM documents
                    ORDER BY indexed_at DESC
                    LIMIT ?
                    "#,
                )
                .bind(limit_i64)
                .fetch_all(&pool)
                .await
            })
            .await
            .map_err(|e| {
                AppError::Database(format!("Failed to list documents (paginated): {}", e))
            })?;

        Ok(crate::infrastructure::persistence::mappers::DocumentMapper::to_entities(&db_models))
    }
}

/// Implementation of `RepositoryPort<Document>` for DDD compliance.
///
/// This implementation works with Document (aggregate root) instead of
/// DocumentEntity. It handles the persistence of the entire aggregate including
/// document, chunks, and tags atomically.
///
/// # Architecture
///
/// Following DDD principles, repositories should work with aggregate roots.
/// This implementation:
/// - Loads the complete aggregate (document + chunks + tags)
/// - Saves the aggregate atomically using transactions
/// - Maintains aggregate invariants
#[async_trait]
impl RepositoryPort<Document> for DocumentRepository {
    async fn find_by_id(&self, id: &str) -> Result<Option<Document>> {
        // 1. Fetch document entity directly from storage
        let entity = self.find_entity_by_id_internal(id).await?;

        match entity {
            Some(ent) => {
                // 2. Convert entity to aggregate Document
                let document = Self::entity_to_aggregate_document(&ent)?;

                // 3. Fetch chunks for the document
                let chunks = self.fetch_chunks_for_document(id).await?;

                // 4. Fetch tags for the document
                let tags = self.fetch_tags_for_document(id).await?;

                // 5. Reconstruct aggregate from parts
                Ok(Some(Document::from_parts(document, chunks, tags)?))
            }
            None => Ok(None),
        }
    }

    async fn find_by_filter(&self, filter: &dyn Filter) -> Result<Vec<Document>> {
        // First get filtered documents directly from storage
        let entities = self.find_entities_by_filter_internal(filter).await?;

        // Then load chunks/tags for each document to build aggregates
        let mut aggregates = Vec::new();
        for entity in entities {
            let doc_id = entity.id().as_str();
            let document = Self::entity_to_aggregate_document(&entity)?;
            let chunks = self.fetch_chunks_for_document(doc_id).await?;
            let tags = self.fetch_tags_for_document(doc_id).await?;
            aggregates.push(Document::from_parts(document, chunks, tags)?);
        }

        Ok(aggregates)
    }

    async fn find_all(&self) -> Result<Vec<Document>> {
        // Get all documents directly from storage
        let entities = self.find_all_entities_internal().await?;

        // Load chunks/tags for each to build aggregates
        let mut aggregates = Vec::new();
        for entity in entities {
            let doc_id = entity.id().as_str();
            let document = Self::entity_to_aggregate_document(&entity)?;
            let chunks = self.fetch_chunks_for_document(doc_id).await?;
            let tags = self.fetch_tags_for_document(doc_id).await?;
            aggregates.push(Document::from_parts(document, chunks, tags)?);
        }

        Ok(aggregates)
    }

    async fn save(&self, aggregate: &Document) -> Result<()> {
        // Begin transaction for atomic aggregate save
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| AppError::Database(format!("Failed to begin transaction: {}", e)))?;

        // 1. Convert aggregate Document to database model
        let doc = aggregate.document();
        let model = Self::aggregate_document_to_model(doc);

        sqlx::query!(
            r#"
            INSERT INTO documents (
                id, file_path, file_name, file_type, mime_type,
                size_bytes, modified_at, indexed_at, checksum, status,
                language, category, quality_score, access_count, last_accessed_at, word_count
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(file_path) DO UPDATE SET
                file_path = excluded.file_path,
                file_name = excluded.file_name,
                file_type = excluded.file_type,
                mime_type = excluded.mime_type,
                size_bytes = excluded.size_bytes,
                modified_at = excluded.modified_at,
                indexed_at = excluded.indexed_at,
                checksum = excluded.checksum,
                status = excluded.status,
                language = excluded.language,
                category = excluded.category,
                quality_score = excluded.quality_score,
                access_count = excluded.access_count,
                last_accessed_at = excluded.last_accessed_at,
                word_count = excluded.word_count,
                updated_at = CURRENT_TIMESTAMP
            "#,
            model.id,
            model.file_path,
            model.file_name,
            model.file_type,
            model.mime_type,
            model.size_bytes,
            model.modified_at,
            model.indexed_at,
            model.checksum,
            model.status,
            model.language,
            model.category,
            model.quality_score,
            model.access_count,
            model.last_accessed_at,
            model.word_count
        )
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::Database(format!("Failed to save document: {}", e)))?;

        // 2. Save chunks (replace all)
        let doc_id = &aggregate.document_id();
        self.save_chunks_transactional(doc_id, aggregate.chunks(), &mut tx)
            .await?;

        // 3. Save tags (replace all)
        self.save_tags_transactional(doc_id, aggregate.tags(), &mut tx)
            .await?;

        // Commit transaction
        tx.commit()
            .await
            .map_err(|e| AppError::Database(format!("Failed to commit aggregate save: {}", e)))?;

        Ok(())
    }

    async fn save_batch(&self, aggregates: &[Document]) -> Result<()> {
        if aggregates.is_empty() {
            return Ok(());
        }

        // Begin transaction for atomic batch save
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| AppError::Database(format!("Failed to begin transaction: {}", e)))?;

        for aggregate in aggregates {
            // Convert aggregate Document to database model
            let doc = aggregate.document();
            let model = Self::aggregate_document_to_model(doc);

            sqlx::query!(
                r#"
                INSERT INTO documents (
                    id, file_path, file_name, file_type, mime_type,
                    size_bytes, modified_at, indexed_at, checksum, status,
                    language, category, quality_score, access_count, last_accessed_at, word_count
                )
                VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                ON CONFLICT(file_path) DO UPDATE SET
                    file_path = excluded.file_path,
                    file_name = excluded.file_name,
                    file_type = excluded.file_type,
                    mime_type = excluded.mime_type,
                    size_bytes = excluded.size_bytes,
                    modified_at = excluded.modified_at,
                    indexed_at = excluded.indexed_at,
                    checksum = excluded.checksum,
                    status = excluded.status,
                    language = excluded.language,
                    category = excluded.category,
                    quality_score = excluded.quality_score,
                    access_count = excluded.access_count,
                    last_accessed_at = excluded.last_accessed_at,
                    word_count = excluded.word_count,
                    updated_at = CURRENT_TIMESTAMP
                "#,
                model.id,
                model.file_path,
                model.file_name,
                model.file_type,
                model.mime_type,
                model.size_bytes,
                model.modified_at,
                model.indexed_at,
                model.checksum,
                model.status,
                model.language,
                model.category,
                model.quality_score,
                model.access_count,
                model.last_accessed_at,
                model.word_count
            )
            .execute(&mut *tx)
            .await
            .map_err(|e| AppError::Database(format!("Failed to save document in batch: {}", e)))?;

            // Save chunks
            let doc_id = &aggregate.document_id();
            self.save_chunks_transactional(doc_id, aggregate.chunks(), &mut tx)
                .await?;

            // Save tags
            self.save_tags_transactional(doc_id, aggregate.tags(), &mut tx)
                .await?;
        }

        // Commit transaction
        tx.commit().await.map_err(|e| {
            AppError::Database(format!("Failed to commit batch aggregate save: {}", e))
        })?;

        Ok(())
    }

    async fn delete(&self, id: &str) -> Result<()> {
        // SQLite foreign keys should cascade delete chunks/tags.
        self.delete_internal(id).await
    }

    async fn delete_batch(&self, ids: &[&str]) -> Result<()> {
        self.delete_batch_internal(ids).await
    }

    async fn count(&self) -> Result<usize> {
        Ok(self.count_documents().await? as usize)
    }

    async fn exists(&self, id: &str) -> Result<bool> {
        self.exists_internal(id).await
    }
}

// ============================================================================
// Unified Trait Implementation (E0225 Fix)
// ============================================================================

/// Unified document repository trait implementation.
///
/// This blanket implementation enables DocumentRepository to be used as a trait object
/// without E0225 errors. Since DocumentRepository already implements both
/// `RepositoryPort<Document>` and `DocumentRepositoryPort`, this marker implementation
/// allows it to satisfy the unified trait.
impl crate::application::ports::DocumentRepository for DocumentRepository {}

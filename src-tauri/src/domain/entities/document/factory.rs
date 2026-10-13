//! Constructing documents: from a file on disk, from stored values, or with a known id and parts.
use super::{Category, Document, DocumentStatus, Language};
use crate::domain::entities::chunk::Chunk;
use crate::domain::value_objects::checksum::Checksum;
use crate::shared::error::Result;
use crate::shared::types::{DocumentId, TagId, ValidatedFilePath};
use chrono::{DateTime, Utc};

/// A document's stored fields, for [`Document::with_id`]. Chunks, tags and
/// source context are attached afterwards.
pub struct StoredDocument {
    pub id: DocumentId,
    pub file_path: ValidatedFilePath,
    pub file_name: String,
    pub file_type: Option<String>,
    pub mime_type: String,
    pub size_bytes: i64,
    pub modified_at: DateTime<Utc>,
    pub indexed_at: DateTime<Utc>,
    pub checksum: Checksum,
    pub status: DocumentStatus,
    pub error_message: Option<String>,
    pub language: Language,
    pub category: Category,
    pub quality_score: f32,
    pub access_count: i32,
    pub last_accessed_at: Option<DateTime<Utc>>,
    pub word_count: i32,
    pub content: String,
}

impl Document {
    /// Create document from file metadata and content.
    ///
    /// This is a factory method for creating documents from file system data,
    /// used by indexing use cases.
    ///
    /// # Arguments
    ///
    /// * `file_path` - Validated file path
    /// * `metadata` - File metadata from file system
    /// * `checksum` - Content checksum
    /// * `content` - Document text content
    /// * `chunking_strategy` - Strategy for chunking this document
    ///
    /// # Returns
    ///
    /// A new Document instance with chunks generated from content.
    ///
    /// # Errors
    ///
    /// Returns `AppError::InvalidState` if non-empty content produces no chunks.
    ///
    /// # Domain Invariants
    ///
    /// - Non-empty content → ≥1 chunk
    /// - Empty content → 0 chunks (valid)
    /// - Chunks have sequential indices starting at 0
    pub fn from_file(
        file_path: crate::shared::types::ValidatedFilePath,
        metadata: crate::domain::value_objects::FileMetadata,
        checksum: crate::domain::value_objects::Checksum,
        content: String,
        chunking_strategy: crate::domain::value_objects::ChunkingStrategy,
    ) -> Result<Self> {
        use crate::shared::error::AppError;

        let document_id = crate::shared::types::DocumentId::new();

        let chunks = if content.trim().is_empty() {
            // Empty/whitespace-only content → no chunks (valid state)
            Vec::new()
        } else {
            // Non-empty content → chunk it
            match chunking_strategy.chunk(&content, &document_id) {
                Ok(chunks) => chunks,
                Err(e) => {
                    // Chunking failed - this is a critical error
                    return Err(AppError::InvalidState(format!(
                        "Failed to chunk document content: {}",
                        e
                    )));
                }
            }
        };

        // Enforce domain invariant: non-empty content MUST produce chunks
        if !content.trim().is_empty() && chunks.is_empty() {
            return Err(AppError::InvalidState(
                "Non-empty document content produced no chunks".to_string(),
            ));
        }

        Ok(Self {
            id: document_id,
            file_path,
            file_name: metadata.file_name().to_string(),
            file_type: std::path::Path::new(metadata.file_name())
                .extension()
                .and_then(|ext| ext.to_str())
                .map(|s| s.to_string()),
            mime_type: metadata.mime_type().to_string(),
            size_bytes: metadata.size_bytes(),
            modified_at: metadata.modified_at(),
            indexed_at: chrono::Utc::now(),
            checksum,
            status: DocumentStatus::Pending,
            error_message: None,
            language: Language::default(),
            category: Category::default(),
            quality_score: 0.5,
            access_count: 0,
            last_accessed_at: None,
            word_count: content.split_whitespace().count() as i32,
            content,
            chunks,
            tags: Vec::new(),
            source_context: None,
        })
    }

    /// Create new document.
    ///
    /// # Arguments
    ///
    /// * `file_path` - Validated file path
    /// * `file_name` - Name of the file
    /// * `mime_type` - MIME type (e.g., "text/plain")
    /// * `size_bytes` - File size in bytes
    /// * `checksum` - Content checksum (e.g., SHA-256 hash)
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use lattice::domain::entities::document::Document;
    /// use lattice::shared::types::ValidatedFilePath;
    /// use std::path::PathBuf;
    ///
    /// let path = ValidatedFilePath::new(PathBuf::from("/docs/report.pdf")).unwrap();
    /// let doc = Document::new(
    ///     path,
    ///     "report.pdf".to_string(),
    ///     "application/pdf".to_string(),
    ///     2048,
    ///     "sha256:abc123".to_string(),
    /// );
    /// assert_eq!(doc.size_bytes(), 2048);
    /// ```
    pub fn new(
        file_path: ValidatedFilePath,
        file_name: String,
        mime_type: String,
        size_bytes: i64,
        checksum: Checksum,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: DocumentId::new(),
            file_path,
            file_name,
            file_type: None,
            mime_type,
            size_bytes,
            modified_at: now,
            indexed_at: now,
            checksum,
            status: DocumentStatus::Pending,
            error_message: None,
            language: Language::default(),
            category: Category::default(),
            quality_score: 0.5,
            access_count: 0,
            last_accessed_at: None,
            word_count: 0,
            content: String::new(),
            chunks: Vec::new(),
            tags: Vec::new(),
            source_context: None,
        }
    }

    /// Create document with existing ID (for reconstruction from storage).
    ///
    /// This is typically used by repositories when loading documents from storage.
    pub fn with_id(stored: StoredDocument) -> Self {
        let StoredDocument {
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
            error_message,
            language,
            category,
            quality_score,
            access_count,
            last_accessed_at,
            word_count,
            content,
        } = stored;
        Self {
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
            error_message,
            language,
            category,
            quality_score,
            access_count,
            last_accessed_at,
            word_count,
            content,
            chunks: Vec::new(),
            tags: Vec::new(),
            source_context: None,
        }
    }

    /// Reconstitution Factory for the Document Aggregate.
    ///
    /// This method acts as the bridge between the persistence layer (Data Model)
    /// and the domain layer (Domain Model). It takes the disconnected parts loaded
    /// from the database and fuses them into a valid Document Entity, enforcing
    /// all domain invariants.
    ///
    /// # Aggregate invariants
    ///
    /// In Domain-Driven Design (DDD), the Aggregate Root must encapsulate all data
    /// necessary to enforce its invariants. Since "must have chunks" is a core
    /// invariant, the Document Entity must hold the chunks and tags directly.
    ///
    /// # Arguments
    ///
    /// * `self` - The Document entity metadata (loaded from the documents table)
    /// * `chunks` - The chunks belonging to this document (loaded from chunks table)
    /// * `tags` - The tag IDs associated with this document (loaded from document_tags table)
    ///
    /// # Errors
    ///
    /// Returns `AppError::InvalidState` if:
    /// - The chunks vector is empty (violates invariant)
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use lattice::domain::entities::Document;
    /// # use lattice::domain::entities::chunk::Chunk;
    /// # use lattice::shared::types::TagId;
    /// # use lattice::shared::error::Result;
    /// # fn example(metadata: Document, chunks: Vec<Chunk>, tags: Vec<TagId>) -> Result<Document> {
    /// // Repository loads metadata, chunks, and tags separately, then reconstructs:
    /// let complete_entity = metadata.from_parts(chunks, tags)?;
    /// # Ok(complete_entity)
    /// # }
    /// ```
    pub fn from_parts(mut self, chunks: Vec<Chunk>, tags: Vec<TagId>) -> Result<Self> {
        use crate::shared::error::AppError;

        // INVARIANT: Document must have at least one chunk
        // This is a core business rule - documents without chunks cannot exist
        if chunks.is_empty() {
            return Err(AppError::InvalidState(
                "Document must have at least one chunk".into(),
            ));
        }

        // Attach the aggregate components
        self.chunks = chunks;
        self.tags = tags;

        Ok(self)
    }
}

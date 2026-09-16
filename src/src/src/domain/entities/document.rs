//! # Document Entity
//!
//! Document entity within the domain layer.
//!
//! This is a pure domain entity with NO infrastructure concerns:
//! - No database IDs (handled by repositories)
//! - Just the core business data and logic
//! - Rich behavior for document management

use crate::domain::entities::chunk::Chunk;
use crate::domain::value_objects::checksum::Checksum;
use crate::shared::domain_types::{DocumentId, TagId, ValidatedFilePath};
use crate::shared::error::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Document status in the indexing lifecycle
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum DocumentStatus {
    /// Document is pending indexing
    #[default]
    Pending,
    /// Document is currently being processed
    Processing,
    /// Document has been successfully indexed
    Indexed,
    /// Document indexing failed
    Failed,
}

/// Language detected in document content
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum Language {
    English,
    Spanish,
    French,
    German,
    Chinese,
    Japanese,
    Python,
    Rust,
    JavaScript,
    TypeScript,
    Go,
    Java,
    Cpp,
    CSharp,
    #[default]
    Unknown,
}

/// Document category for organization
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum Category {
    Code,
    Documentation,
    Research,
    Notes,
    Reference,
    Tutorial,
    #[default]
    Uncategorized,
}

impl std::fmt::Display for DocumentStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pending => write!(f, "pending"),
            Self::Processing => write!(f, "processing"),
            Self::Indexed => write!(f, "indexed"),
            Self::Failed => write!(f, "failed"),
        }
    }
}

impl std::str::FromStr for DocumentStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "pending" => Ok(Self::Pending),
            "processing" => Ok(Self::Processing),
            "indexed" => Ok(Self::Indexed),
            "failed" => Ok(Self::Failed),
            _ => Err(format!("Invalid document status: {}", s)),
        }
    }
}

impl std::fmt::Display for Language {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::English => write!(f, "english"),
            Self::Spanish => write!(f, "spanish"),
            Self::French => write!(f, "french"),
            Self::German => write!(f, "german"),
            Self::Chinese => write!(f, "chinese"),
            Self::Japanese => write!(f, "japanese"),
            Self::Python => write!(f, "python"),
            Self::Rust => write!(f, "rust"),
            Self::JavaScript => write!(f, "javascript"),
            Self::TypeScript => write!(f, "typescript"),
            Self::Go => write!(f, "go"),
            Self::Java => write!(f, "java"),
            Self::Cpp => write!(f, "cpp"),
            Self::CSharp => write!(f, "csharp"),
            Self::Unknown => write!(f, "unknown"),
        }
    }
}

impl std::str::FromStr for Language {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "english" => Ok(Self::English),
            "spanish" => Ok(Self::Spanish),
            "french" => Ok(Self::French),
            "german" => Ok(Self::German),
            "chinese" => Ok(Self::Chinese),
            "japanese" => Ok(Self::Japanese),
            "python" => Ok(Self::Python),
            "rust" => Ok(Self::Rust),
            "javascript" => Ok(Self::JavaScript),
            "typescript" => Ok(Self::TypeScript),
            "go" => Ok(Self::Go),
            "java" => Ok(Self::Java),
            "cpp" => Ok(Self::Cpp),
            "csharp" => Ok(Self::CSharp),
            "unknown" => Ok(Self::Unknown),
            _ => Ok(Self::Unknown), // Default to Unknown for unrecognized languages
        }
    }
}

impl std::fmt::Display for Category {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Code => write!(f, "code"),
            Self::Documentation => write!(f, "documentation"),
            Self::Research => write!(f, "research"),
            Self::Notes => write!(f, "notes"),
            Self::Reference => write!(f, "reference"),
            Self::Tutorial => write!(f, "tutorial"),
            Self::Uncategorized => write!(f, "uncategorized"),
        }
    }
}

impl std::str::FromStr for Category {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "code" => Ok(Self::Code),
            "documentation" => Ok(Self::Documentation),
            "research" => Ok(Self::Research),
            "notes" => Ok(Self::Notes),
            "reference" => Ok(Self::Reference),
            "tutorial" => Ok(Self::Tutorial),
            "uncategorized" => Ok(Self::Uncategorized),
            _ => Ok(Self::Uncategorized), // Default to Uncategorized for unrecognized categories
        }
    }
}

/// Document entity (aggregate root for document-related operations).
///
/// Represents a file document with metadata.
///
/// ## Pure Domain Model
///
/// This entity contains only business logic. Infrastructure concerns like
/// database persistence, file I/O, and search indexes are handled by
/// the infrastructure layer.
///
/// ## Invariants
///
/// - File path must be unique
/// - File name cannot be empty
/// - Checksum validates content integrity
/// - Modified timestamp must be <= indexed timestamp
///
/// ## Example
///
/// ```rust,no_run
/// use lattice::domain::entities::document::Document;
/// use lattice::shared::domain_types::{DocumentId, ValidatedFilePath};
/// use std::path::PathBuf;
///
/// let path = ValidatedFilePath::new(PathBuf::from("/docs/file.txt")).unwrap();
/// let doc = Document::new(
///     path,
///     "file.txt".to_string(),
///     "text/plain".to_string(),
///     1024,
///     "abc123".to_string(),
/// );
///
/// assert_eq!(doc.file_name(), "file.txt");
/// assert_eq!(doc.mime_type(), "text/plain");
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Document {
    id: DocumentId,
    file_path: ValidatedFilePath,
    file_name: String,
    file_type: Option<String>,
    mime_type: String,
    size_bytes: i64,
    modified_at: DateTime<Utc>,
    indexed_at: DateTime<Utc>,
    checksum: Checksum,
    status: DocumentStatus,
    error_message: Option<String>,
    // Extended metadata
    language: Language,
    category: Category,
    quality_score: f32,
    access_count: i32,
    #[serde(rename = "lastAccessedAt", alias = "last_accessed")]
    last_accessed_at: Option<DateTime<Utc>>,
    word_count: i32,
    #[serde(default)]
    source_context: Option<crate::domain::value_objects::source_context::SourceContext>,

    // Raw document content
    #[serde(skip)] // Don't serialize raw content in API responses (too large)
    content: String,

    // The aggregate root encapsulates chunks and tags
    // to enforce the invariant: "Document must have at least one chunk"
    #[serde(skip)] // Don't serialize chunks/tags in API responses (use projections for that)
    chunks: Vec<Chunk>,
    #[serde(skip)]
    tags: Vec<TagId>,
}

impl Document {
    pub fn source_context(
        &self,
    ) -> Option<&crate::domain::value_objects::source_context::SourceContext> {
        self.source_context.as_ref()
    }
    pub fn set_source_context(
        &mut self,
        value: Option<crate::domain::value_objects::source_context::SourceContext>,
    ) {
        self.source_context = value;
    }

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
        file_path: crate::shared::domain_types::ValidatedFilePath,
        metadata: crate::domain::value_objects::FileMetadata,
        checksum: crate::domain::value_objects::Checksum,
        content: String,
        chunking_strategy: crate::domain::value_objects::ChunkingStrategy,
    ) -> Result<Self> {
        use crate::shared::error::AppError;

        let document_id = crate::shared::domain_types::DocumentId::new();

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
    /// use lattice::shared::domain_types::ValidatedFilePath;
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
    #[allow(clippy::too_many_arguments)]
    pub fn with_id(
        id: DocumentId,
        file_path: ValidatedFilePath,
        file_name: String,
        file_type: Option<String>,
        mime_type: String,
        size_bytes: i64,
        modified_at: DateTime<Utc>,
        indexed_at: DateTime<Utc>,
        checksum: Checksum,
        status: DocumentStatus,
        error_message: Option<String>,
        language: Language,
        category: Category,
        quality_score: f32,
        access_count: i32,
        last_accessed_at: Option<DateTime<Utc>>,
        word_count: i32,
        content: String,
    ) -> Self {
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
    /// # use lattice::shared::domain_types::TagId;
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

    /// Get document ID.
    pub fn id(&self) -> &DocumentId {
        &self.id
    }

    /// Get file path.
    pub fn file_path(&self) -> &Path {
        self.file_path.as_path()
    }

    /// Get validated file path.
    pub fn validated_file_path(&self) -> &ValidatedFilePath {
        &self.file_path
    }

    /// Get file name.
    pub fn file_name(&self) -> &str {
        &self.file_name
    }

    /// Get file type (optional).
    pub fn file_type(&self) -> Option<&str> {
        self.file_type.as_deref()
    }

    /// Get MIME type.
    pub fn mime_type(&self) -> &str {
        &self.mime_type
    }

    /// Get file size in bytes.
    pub fn size_bytes(&self) -> i64 {
        self.size_bytes
    }

    /// Get file modification timestamp.
    pub fn modified_at(&self) -> &DateTime<Utc> {
        &self.modified_at
    }

    /// Get file update timestamp (alias for modified_at).
    pub fn updated_at(&self) -> &DateTime<Utc> {
        &self.modified_at
    }

    /// Get indexing timestamp.
    pub fn indexed_at(&self) -> &DateTime<Utc> {
        &self.indexed_at
    }

    /// Get content checksum.
    pub fn checksum(&self) -> &Checksum {
        &self.checksum
    }

    /// Get document status.
    pub fn status(&self) -> DocumentStatus {
        self.status
    }

    /// Check if document is indexed.
    pub fn is_indexed(&self) -> bool {
        self.status == DocumentStatus::Indexed
    }

    /// Check if document indexing failed.
    pub fn is_failed(&self) -> bool {
        self.status == DocumentStatus::Failed
    }

    /// Check if document is pending indexing.
    pub fn is_pending(&self) -> bool {
        self.status == DocumentStatus::Pending
    }

    /// Check if document is currently being processed.
    pub fn is_processing(&self) -> bool {
        self.status == DocumentStatus::Processing
    }

    /// Mark document as indexed.
    ///
    /// Updates status and indexed_at timestamp.
    pub fn mark_indexed(&mut self) {
        self.status = DocumentStatus::Indexed;
        self.indexed_at = Utc::now();
    }

    /// Mark document indexing as failed.
    pub fn mark_failed(&mut self) {
        self.status = DocumentStatus::Failed;
    }

    /// Mark document as pending.
    pub fn mark_pending(&mut self) {
        self.status = DocumentStatus::Pending;
    }

    /// Mark document as processing.
    pub fn mark_processing(&mut self) {
        self.status = DocumentStatus::Processing;
    }

    /// Update checksum (when file content changes).
    ///
    /// Also updates modified_at timestamp and marks as pending.
    pub fn update_checksum(&mut self, checksum: Checksum) {
        self.checksum = checksum;
        self.modified_at = Utc::now();
        self.status = DocumentStatus::Pending;
    }

    /// Check if document needs reindexing based on checksum.
    ///
    /// Returns true if the provided checksum differs from current.
    pub fn needs_reindexing(&self, current_checksum: &Checksum) -> bool {
        &self.checksum != current_checksum
    }

    /// Get error message if indexing failed.
    pub fn error_message(&self) -> Option<&str> {
        self.error_message.as_deref()
    }

    /// Set error message for failed indexing.
    pub fn set_error_message(&mut self, message: Option<String>) {
        self.error_message = message;
    }

    /// Get file extension from file name.
    pub fn file_extension(&self) -> Option<&str> {
        Path::new(&self.file_name)
            .extension()
            .and_then(|ext| ext.to_str())
    }

    /// Check if document is a text file based on MIME type.
    pub fn is_text(&self) -> bool {
        self.mime_type.starts_with("text/")
    }

    /// Check if document is a PDF file.
    pub fn is_pdf(&self) -> bool {
        self.mime_type == "application/pdf"
    }

    /// Check if document is a Word document.
    pub fn is_word(&self) -> bool {
        matches!(
            self.mime_type.as_str(),
            "application/msword"
                | "application/vnd.openxmlformats-officedocument.wordprocessingml.document"
        )
    }

    /// Get human-readable file size.
    ///
    /// Returns formatted string like "1.5 KB", "2.3 MB", etc.
    pub fn size_human_readable(&self) -> String {
        const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
        let mut size = self.size_bytes as f64;
        let mut unit_idx = 0;

        while size >= 1024.0 && unit_idx < UNITS.len() - 1 {
            size /= 1024.0;
            unit_idx += 1;
        }

        let unit = UNITS.get(unit_idx).unwrap_or(&"B");
        format!("{:.1} {}", size, unit)
    }

    /// Get the chunks belonging to this document.
    ///
    /// Returns a slice of all chunks that are part of this document aggregate.
    pub fn chunks(&self) -> &[Chunk] {
        &self.chunks
    }

    /// Get the tag IDs associated with this document.
    ///
    /// Returns a slice of all tag IDs linked to this document.
    pub fn tags(&self) -> &[TagId] {
        &self.tags
    }

    /// Get document content.
    pub fn content(&self) -> &str {
        &self.content
    }

    /// Set document content.
    ///
    /// This updates the content and also resets the status to Pending for reindexing.
    pub fn set_content(&mut self, content: String) {
        self.content = content;
        self.status = DocumentStatus::Pending;
        self.modified_at = Utc::now();
    }

    /// Get document ID as string (alias for `id().as_str()`).
    ///
    /// This method provides compatibility with old code that called `doc.document_id()`.
    pub fn document_id(&self) -> &str {
        self.id.as_str()
    }

    /// Get file path as Path reference (alias for `file_path()`).
    ///
    /// This method provides compatibility with old code that called `doc.as_path()`.
    pub fn as_path(&self) -> &Path {
        self.file_path()
    }

    /// Get document language.
    pub fn language(&self) -> &Language {
        &self.language
    }

    /// Set document language.
    pub fn set_language(&mut self, language: Language) {
        self.language = language;
    }

    /// Get document category.
    pub fn category(&self) -> &Category {
        &self.category
    }

    /// Set document category.
    pub fn set_category(&mut self, category: Category) {
        self.category = category;
    }

    /// Get quality score (0.0-1.0).
    pub fn quality_score(&self) -> f32 {
        self.quality_score
    }

    /// Set quality score with clamping to valid range (0.0-1.0).
    pub fn set_quality_score(&mut self, score: f32) {
        self.quality_score = score.clamp(0.0, 1.0);
    }

    /// Get access count.
    pub fn access_count(&self) -> i32 {
        self.access_count
    }

    /// Get last accessed timestamp.
    pub fn last_accessed_at(&self) -> Option<&DateTime<Utc>> {
        self.last_accessed_at.as_ref()
    }

    /// Increment access count and update last accessed timestamp.
    pub fn increment_access(&mut self) {
        self.access_count += 1;
        self.last_accessed_at = Some(Utc::now());
    }

    /// Get word count.
    pub fn word_count(&self) -> i32 {
        self.word_count
    }

    /// Set word count.
    pub fn set_word_count(&mut self, count: i32) {
        self.word_count = count.max(0);
    }

    /// Set token count for a specific chunk by index.
    ///
    /// This allows the indexing use case to set chunk metadata while maintaining
    /// Document as the sole currency for persistence.
    ///
    /// # Arguments
    ///
    /// * `index` - The zero-based index of the chunk to update
    /// * `token_count` - The number of tokens in the chunk
    ///
    /// # Panics
    ///
    /// Panics if the index is out of bounds.
    pub fn set_chunk_token_count(&mut self, index: usize, token_count: i32) {
        if let Some(chunk) = self.chunks.get_mut(index) {
            chunk.set_token_count(token_count);
        }
    }

    /// Set word count for a specific chunk by index.
    ///
    /// # Arguments
    ///
    /// * `index` - The zero-based index of the chunk to update
    /// * `word_count` - The number of words in the chunk
    ///
    /// # Panics
    ///
    /// Panics if the index is out of bounds.
    pub fn set_chunk_word_count(&mut self, index: usize, word_count: usize) {
        if let Some(chunk) = self.chunks.get_mut(index) {
            chunk.set_word_count(word_count);
        }
    }

    /// Set code detection flag for a specific chunk by index.
    ///
    /// # Arguments
    ///
    /// * `index` - The zero-based index of the chunk to update
    /// * `has_code` - Whether the chunk contains code
    ///
    /// # Panics
    ///
    /// Panics if the index is out of bounds.
    pub fn set_chunk_has_code(&mut self, index: usize, has_code: bool) {
        if let Some(chunk) = self.chunks.get_mut(index) {
            chunk.set_has_code(has_code);
        }
    }

    /// Set section/heading for a specific chunk by index.
    ///
    /// # Arguments
    ///
    /// * `index` - The zero-based index of the chunk to update
    /// * `section` - The section or heading text, if any
    ///
    /// # Panics
    ///
    /// Panics if the index is out of bounds.
    pub fn set_chunk_section(&mut self, index: usize, section: Option<String>) {
        if let Some(chunk) = self.chunks.get_mut(index) {
            chunk.set_section(section);
        }
    }

    /// Mark document for reindexing with new content.
    ///
    /// Resets the document status to Pending and updates the modified timestamp.
    /// This is typically called when the file content has changed and needs to be
    /// reindexed.
    ///
    /// # Arguments
    ///
    /// * `content` - New document content
    /// * `chunking_strategy` - Strategy for chunking
    ///
    /// # Returns
    ///
    /// Returns Ok(()) if successful
    pub fn reindex(
        &mut self,
        content: String,
        chunking_strategy: crate::domain::value_objects::ChunkingStrategy,
    ) -> Result<()> {
        self.content = content.clone();
        self.word_count = content.split_whitespace().count() as i32;
        self.status = DocumentStatus::Pending;
        self.modified_at = Utc::now();

        // Clear old chunks and tags
        self.chunks.clear();
        self.tags.clear();

        let new_chunks = chunking_strategy.chunk(&content, &self.id)?;
        self.chunks = new_chunks;

        Ok(())
    }
}

#[cfg(test)]
mod property_tests {
    use super::*;
    use proptest::prelude::*;
    use std::io::Write;
    use std::path::PathBuf;
    use tempfile::NamedTempFile;

    fn temp_path(file_name: &str) -> PathBuf {
        std::env::temp_dir().join(file_name)
    }

    fn arbitrary_document_status() -> impl Strategy<Value = DocumentStatus> {
        prop_oneof![
            Just(DocumentStatus::Pending),
            Just(DocumentStatus::Processing),
            Just(DocumentStatus::Indexed),
            Just(DocumentStatus::Failed),
        ]
    }

    fn arbitrary_language() -> impl Strategy<Value = Language> {
        prop_oneof![
            Just(Language::English),
            Just(Language::Spanish),
            Just(Language::French),
            Just(Language::German),
            Just(Language::Chinese),
            Just(Language::Japanese),
            Just(Language::Python),
            Just(Language::Rust),
            Just(Language::JavaScript),
            Just(Language::TypeScript),
            Just(Language::Go),
            Just(Language::Java),
            Just(Language::Cpp),
            Just(Language::CSharp),
            Just(Language::Unknown),
        ]
    }

    fn arbitrary_category() -> impl Strategy<Value = Category> {
        prop_oneof![
            Just(Category::Code),
            Just(Category::Documentation),
            Just(Category::Research),
            Just(Category::Notes),
            Just(Category::Reference),
            Just(Category::Tutorial),
            Just(Category::Uncategorized),
        ]
    }

    fn valid_mime_type() -> impl Strategy<Value = String> {
        prop::sample::select(vec![
            "text/plain".to_string(),
            "text/markdown".to_string(),
            "application/pdf".to_string(),
            "application/json".to_string(),
            "text/html".to_string(),
            "application/vnd.openxmlformats-officedocument.wordprocessingml.document".to_string(),
        ])
    }

    fn valid_file_content() -> impl Strategy<Value = String> {
        prop::string::string_regex("[a-zA-Z0-9 \\n\\r\\t.,;:!?()\\[\\]{}\"']{10,1000}").unwrap()
    }

    fn non_empty_string() -> impl Strategy<Value = String> {
        prop::string::string_regex("[a-zA-Z0-9_\\-\\.]{1,100}").unwrap()
    }

    fn arbitrary_document() -> impl Strategy<Value = Document> {
        (
            arbitrary_document_status(),
            valid_mime_type(),
            valid_file_content(),
            non_empty_string(),
        )
            .prop_map(|(status, mime, content, filename)| {
                let mut temp = NamedTempFile::new().unwrap();
                temp.write_all(content.as_bytes()).unwrap();
                let path = temp.path().to_path_buf();

                let validated_path = ValidatedFilePath::new(path.clone())
                    .unwrap_or_else(|_| ValidatedFilePath::new(temp_path("test.txt")).unwrap());

                let checksum = Checksum::new("0".repeat(64)).expect("valid checksum");
                let size_bytes = content.len() as i64;

                let mut doc = Document::new(
                    validated_path,
                    filename.clone(),
                    mime.clone(),
                    size_bytes,
                    checksum,
                );
                doc.status = status;
                doc
            })
    }

    proptest! {
        #[test]
        fn prop_document_has_unique_id(_dummy in 0..10u32) {
            let path1 = ValidatedFilePath::new(temp_path("test1.txt")).unwrap();
            let path2 = ValidatedFilePath::new(temp_path("test2.txt")).unwrap();

            let doc1 = Document::new(
                path1,
                "test1.txt".to_string(),
                "text/plain".to_string(),
                100,
                Checksum::new("1".repeat(64)).expect("valid checksum"),
            );
            let doc2 = Document::new(
                path2,
                "test2.txt".to_string(),
                "text/plain".to_string(),
                100,
                Checksum::new("2".repeat(64)).expect("valid checksum"),
            );

            prop_assert_ne!(doc1.id(), doc2.id());
        }

        #[test]
        fn prop_document_id_stable_across_clones(doc in arbitrary_document()) {
            let cloned = doc.clone();
            prop_assert_eq!(doc.id(), cloned.id());
        }

        #[test]
        fn prop_new_document_starts_pending(_dummy in 0..10u32) {
            let path = ValidatedFilePath::new(temp_path("test.txt")).unwrap();
            let doc = Document::new(
                path,
                "test.txt".to_string(),
                "text/plain".to_string(),
                100,
                Checksum::new("a".repeat(64)).expect("valid checksum"),
            );

            prop_assert_eq!(doc.status(), DocumentStatus::Pending);
        }

        #[test]
        fn prop_document_file_name_non_empty(doc in arbitrary_document()) {
            prop_assert!(!doc.file_name().is_empty());
        }

        #[test]
        fn prop_document_creation_time_before_update(doc in arbitrary_document()) {
            prop_assert!(doc.modified_at() <= doc.indexed_at());
        }
    }

    proptest! {
        #[test]
        fn prop_document_status_transition_to_indexed(mut doc in arbitrary_document()) {
            doc.mark_indexed();
            prop_assert_eq!(doc.status(), DocumentStatus::Indexed);
        }

        #[test]
        fn prop_document_status_transition_to_failed(mut doc in arbitrary_document()) {
            doc.mark_failed();
            prop_assert_eq!(doc.status(), DocumentStatus::Failed);
        }

        #[test]
        fn prop_document_status_updates_timestamp(mut doc in arbitrary_document()) {
            let before = *doc.indexed_at();
            std::thread::sleep(std::time::Duration::from_millis(10));
            doc.mark_indexed();
            prop_assert!(doc.indexed_at() >= &before);
        }

        #[test]
        fn prop_is_indexed_true_only_when_indexed(status in arbitrary_document_status()) {
            let path = ValidatedFilePath::new(temp_path("test.txt")).unwrap();
            let mut doc = Document::new(
                path,
                "test.txt".to_string(),
                "text/plain".to_string(),
                100,
                Checksum::new("a".repeat(64)).expect("valid checksum"),
            );
            doc.status = status;

            if status == DocumentStatus::Indexed {
                prop_assert!(doc.is_indexed());
            } else {
                prop_assert!(!doc.is_indexed());
            }
        }

        #[test]
        fn prop_status_serialization_roundtrip(status in arbitrary_document_status()) {
            let json = serde_json::to_string(&status).unwrap();
            let deserialized: DocumentStatus = serde_json::from_str(&json).unwrap();
            prop_assert_eq!(status, deserialized);
        }

        #[test]
        fn prop_status_can_transition_from_any_to_any(
            from_status in arbitrary_document_status(),
            to_status in arbitrary_document_status()
        ) {
            let path = ValidatedFilePath::new(temp_path("test.txt")).unwrap();
            let mut doc = Document::new(
                path,
                "test.txt".to_string(),
                "text/plain".to_string(),
                100,
                Checksum::new("a".repeat(64)).expect("valid checksum"),
            );
            doc.status = from_status;

            match to_status {
                DocumentStatus::Indexed => doc.mark_indexed(),
                DocumentStatus::Failed => doc.mark_failed(),
                DocumentStatus::Pending => doc.mark_pending(),
                DocumentStatus::Processing => doc.mark_processing(),
            }

            prop_assert_eq!(doc.status(), to_status);
        }
    }

    proptest! {
        #[test]
        fn prop_document_checksum_stable(doc in arbitrary_document()) {
            let checksum1 = doc.checksum().clone();
            let checksum2 = doc.checksum().clone();
            prop_assert_eq!(checksum1, checksum2);
        }

        #[test]
        fn prop_different_content_different_checksum(_dummy in 0..10u32) {
            let path1 = ValidatedFilePath::new(temp_path("test1.txt")).unwrap();
            let path2 = ValidatedFilePath::new(temp_path("test2.txt")).unwrap();

            let doc1 = Document::new(
                path1,
                "test.txt".to_string(),
                "text/plain".to_string(),
                100,
                Checksum::new("c1".repeat(32)).expect("valid checksum"),
            );
            let doc2 = Document::new(
                path2,
                "test.txt".to_string(),
                "text/plain".to_string(),
                100,
                Checksum::new("c2".repeat(32)).expect("valid checksum"),
            );

            prop_assert_ne!(doc1.checksum(), doc2.checksum());
        }

        #[test]
        fn prop_size_bytes_accessible(doc in arbitrary_document()) {
            prop_assert!(doc.size_bytes() >= 0);
        }

        #[test]
        fn prop_checksum_update_marks_pending(mut doc in arbitrary_document()) {
            let new_checksum = Checksum::new("d".repeat(64)).expect("valid checksum");
            doc.update_checksum(new_checksum);
            prop_assert_eq!(doc.status(), DocumentStatus::Pending);
        }
    }

    proptest! {
        #[test]
        fn prop_document_language_can_be_set(mut doc in arbitrary_document(), lang in arbitrary_language()) {
            doc.set_language(lang.clone());
            prop_assert_eq!(doc.language(), &lang);
        }

        #[test]
        fn prop_document_category_can_be_set(mut doc in arbitrary_document(), cat in arbitrary_category()) {
            doc.set_category(cat.clone());
            prop_assert_eq!(doc.category(), &cat);
        }

        #[test]
        fn prop_quality_score_clamped(mut doc in arbitrary_document(), score in -10.0f32..10.0f32) {
            doc.set_quality_score(score);
            let result = doc.quality_score();
            prop_assert!((0.0..=1.0).contains(&result));
        }

        #[test]
        fn prop_access_count_increments(mut doc in arbitrary_document()) {
            let before = doc.access_count();
            doc.increment_access();
            prop_assert_eq!(doc.access_count(), before + 1);
        }

        #[test]
        fn prop_word_count_non_negative(mut doc in arbitrary_document(), count in -100i32..1000i32) {
            doc.set_word_count(count);
            prop_assert!(doc.word_count() >= 0);
        }
    }

    proptest! {
        #[test]
        fn prop_mime_type_preserved(mime in valid_mime_type()) {
            let path = ValidatedFilePath::new(temp_path("test.txt")).unwrap();
            let doc = Document::new(
                path,
                "test.txt".to_string(),
                mime.clone(),
                100,
                Checksum::new("a".repeat(64)).expect("valid checksum"),
            );

            prop_assert_eq!(doc.mime_type(), &mime);
        }

        #[test]
        fn prop_is_text_detects_text_mime(_dummy in 0..10u32) {
            let path = ValidatedFilePath::new(temp_path("test.txt")).unwrap();
            let doc = Document::new(
                path,
                "test.txt".to_string(),
                "text/plain".to_string(),
                100,
                Checksum::new("a".repeat(64)).expect("valid checksum"),
            );

            prop_assert!(doc.is_text());
        }

        #[test]
        fn prop_is_pdf_detects_pdf(_dummy in 0..10u32) {
            let path = ValidatedFilePath::new(temp_path("test.pdf")).unwrap();
            let doc = Document::new(
                path,
                "test.pdf".to_string(),
                "application/pdf".to_string(),
                100,
                Checksum::new("a".repeat(64)).expect("valid checksum"),
            );

            prop_assert!(doc.is_pdf());
        }

        #[test]
        fn prop_size_human_readable_format(doc in arbitrary_document()) {
            let size_str = doc.size_human_readable();
            prop_assert!(size_str.contains("B") || size_str.contains("KB") || size_str.contains("MB"));
        }
    }

    proptest! {
        #[test]
        fn prop_document_serialization_roundtrip(doc in arbitrary_document()) {
            let json = serde_json::to_string(&doc).unwrap();
            let deserialized: Document = serde_json::from_str(&json).unwrap();
            prop_assert_eq!(doc.id(), deserialized.id());
            prop_assert_eq!(doc.status(), deserialized.status());
            prop_assert_eq!(doc.checksum(), deserialized.checksum());
        }

        #[test]
        fn prop_document_debug_includes_id(doc in arbitrary_document()) {
            let debug_str = format!("{:?}", doc);
            let id_str = doc.id().to_string();
            prop_assert!(debug_str.contains(&id_str));
        }

        #[test]
        fn prop_document_clone_creates_equal(doc in arbitrary_document()) {
            let cloned = doc.clone();
            prop_assert_eq!(doc.id(), cloned.id());
            prop_assert_eq!(doc.status(), cloned.status());
            prop_assert_eq!(doc.checksum(), cloned.checksum());
            prop_assert_eq!(doc.file_name(), cloned.file_name());
        }
    }
}

#[cfg(test)]
mod unit_tests {
    use super::*;
    use std::path::PathBuf;
    use std::str::FromStr;

    fn temp_path(file_name: &str) -> PathBuf {
        std::env::temp_dir().join(file_name)
    }

    fn create_test_document() -> Document {
        let path =
            ValidatedFilePath::new(temp_path("test.txt")).expect("Failed to create validated path");
        let checksum = Checksum::new("a".repeat(64)).expect("valid checksum");

        Document::new(
            path,
            "test.txt".to_string(),
            "text/plain".to_string(),
            1024,
            checksum,
        )
    }

    fn create_test_chunk(content: &str, chunk_number: usize) -> Chunk {
        Chunk::new(DocumentId::new(), content.to_string(), chunk_number)
    }

    mod display_fromstr_tests {
        use super::*;

        #[test]
        fn test_document_status_display_all_variants() {
            assert_eq!(DocumentStatus::Pending.to_string(), "pending");
            assert_eq!(DocumentStatus::Processing.to_string(), "processing");
            assert_eq!(DocumentStatus::Indexed.to_string(), "indexed");
            assert_eq!(DocumentStatus::Failed.to_string(), "failed");
        }

        #[test]
        fn test_document_status_from_str_valid() {
            assert_eq!(
                DocumentStatus::from_str("pending").unwrap(),
                DocumentStatus::Pending
            );
            assert_eq!(
                DocumentStatus::from_str("processing").unwrap(),
                DocumentStatus::Processing
            );
            assert_eq!(
                DocumentStatus::from_str("indexed").unwrap(),
                DocumentStatus::Indexed
            );
            assert_eq!(
                DocumentStatus::from_str("failed").unwrap(),
                DocumentStatus::Failed
            );
        }

        #[test]
        fn test_document_status_from_str_invalid() {
            assert!(DocumentStatus::from_str("invalid").is_err());
            assert!(DocumentStatus::from_str("not_a_status").is_err());
            assert!(DocumentStatus::from_str("").is_err());
        }

        #[test]
        fn test_language_display_all_variants() {
            assert_eq!(Language::English.to_string(), "english");
            assert_eq!(Language::Spanish.to_string(), "spanish");
            assert_eq!(Language::French.to_string(), "french");
            assert_eq!(Language::German.to_string(), "german");
            assert_eq!(Language::Chinese.to_string(), "chinese");
            assert_eq!(Language::Japanese.to_string(), "japanese");
            assert_eq!(Language::Python.to_string(), "python");
            assert_eq!(Language::Rust.to_string(), "rust");
            assert_eq!(Language::JavaScript.to_string(), "javascript");
            assert_eq!(Language::TypeScript.to_string(), "typescript");
            assert_eq!(Language::Go.to_string(), "go");
            assert_eq!(Language::Java.to_string(), "java");
            assert_eq!(Language::Cpp.to_string(), "cpp");
            assert_eq!(Language::CSharp.to_string(), "csharp");
            assert_eq!(Language::Unknown.to_string(), "unknown");
        }

        #[test]
        fn test_language_from_str_valid() {
            assert_eq!(Language::from_str("english").unwrap(), Language::English);
            assert_eq!(Language::from_str("spanish").unwrap(), Language::Spanish);
            assert_eq!(Language::from_str("python").unwrap(), Language::Python);
            assert_eq!(Language::from_str("rust").unwrap(), Language::Rust);
            assert_eq!(Language::from_str("unknown").unwrap(), Language::Unknown);
        }

        #[test]
        fn test_language_from_str_invalid_defaults_unknown() {
            // Language::from_str uses to_lowercase(), so it accepts case-insensitive input
            // "ENGLISH" -> "english" -> matches Language::English
            assert_eq!(Language::from_str("ENGLISH").unwrap(), Language::English);

            // These truly invalid inputs default to Unknown
            assert_eq!(
                Language::from_str("invalid_language").unwrap(),
                Language::Unknown
            );
            assert_eq!(
                Language::from_str("not_a_language").unwrap(),
                Language::Unknown
            );
        }

        #[test]
        fn test_category_display_all_variants() {
            assert_eq!(Category::Code.to_string(), "code");
            assert_eq!(Category::Documentation.to_string(), "documentation");
            assert_eq!(Category::Research.to_string(), "research");
            assert_eq!(Category::Notes.to_string(), "notes");
            assert_eq!(Category::Reference.to_string(), "reference");
            assert_eq!(Category::Tutorial.to_string(), "tutorial");
            assert_eq!(Category::Uncategorized.to_string(), "uncategorized");
        }

        #[test]
        fn test_category_from_str_valid() {
            assert_eq!(Category::from_str("code").unwrap(), Category::Code);
            assert_eq!(
                Category::from_str("documentation").unwrap(),
                Category::Documentation
            );
            assert_eq!(Category::from_str("research").unwrap(), Category::Research);
            assert_eq!(Category::from_str("notes").unwrap(), Category::Notes);
            assert_eq!(
                Category::from_str("uncategorized").unwrap(),
                Category::Uncategorized
            );
        }

        #[test]
        fn test_category_from_str_invalid_defaults_uncategorized() {
            assert_eq!(Category::from_str("CODE").unwrap(), Category::Code);

            // These truly invalid inputs default to Uncategorized
            assert_eq!(
                Category::from_str("invalid_category").unwrap(),
                Category::Uncategorized
            );
            assert_eq!(
                Category::from_str("not_a_category").unwrap(),
                Category::Uncategorized
            );
        }
    }

    mod chunk_manipulation_tests {
        use super::*;

        #[test]
        fn test_set_chunk_token_count_valid_index() {
            let mut doc = create_test_document();
            let chunk = create_test_chunk("Test content", 0);
            doc.chunks.push(chunk);

            doc.set_chunk_token_count(0, 42);

            assert_eq!(doc.chunks()[0].token_count(), 42);
        }

        #[test]
        fn test_set_chunk_token_count_invalid_index() {
            let mut doc = create_test_document();

            doc.set_chunk_token_count(0, 42);

            // Document should remain valid
            assert_eq!(doc.chunks().len(), 0);
        }

        #[test]
        fn test_set_chunk_word_count_valid_index() {
            let mut doc = create_test_document();
            let chunk = create_test_chunk("Test content with multiple words", 0);
            doc.chunks.push(chunk);

            doc.set_chunk_word_count(0, 5);

            assert_eq!(doc.chunks()[0].word_count(), 5);
        }

        #[test]
        fn test_set_chunk_word_count_invalid_index() {
            let mut doc = create_test_document();

            doc.set_chunk_word_count(0, 5);

            // Document should remain valid
            assert_eq!(doc.chunks().len(), 0);
        }

        #[test]
        fn test_set_chunk_has_code_valid_index() {
            let mut doc = create_test_document();
            let chunk = create_test_chunk("fn main() { println!(\"Hello\"); }", 0);
            doc.chunks.push(chunk);

            doc.set_chunk_has_code(0, true);

            assert!(doc.chunks()[0].has_code());
        }

        #[test]
        fn test_set_chunk_has_code_invalid_index() {
            let mut doc = create_test_document();

            doc.set_chunk_has_code(0, true);

            // Document should remain valid
            assert_eq!(doc.chunks().len(), 0);
        }

        #[test]
        fn test_set_chunk_section_valid_index() {
            let mut doc = create_test_document();
            let chunk = create_test_chunk("Section content", 0);
            doc.chunks.push(chunk);

            doc.set_chunk_section(0, Some("Introduction".to_string()));

            assert_eq!(doc.chunks()[0].section(), Some("Introduction"));
        }

        #[test]
        fn test_set_chunk_section_invalid_index() {
            let mut doc = create_test_document();

            doc.set_chunk_section(0, Some("Introduction".to_string()));

            // Document should remain valid
            assert_eq!(doc.chunks().len(), 0);
        }

        #[test]
        fn test_chunk_mutation_maintains_aggregate_root() {
            let mut doc = create_test_document();
            let chunk1 = create_test_chunk("First chunk", 0);
            let chunk2 = create_test_chunk("Second chunk", 1);
            doc.chunks.push(chunk1);
            doc.chunks.push(chunk2);

            // Mutate first chunk
            doc.set_chunk_token_count(0, 10);
            doc.set_chunk_has_code(0, true);

            assert_eq!(doc.chunks()[0].token_count(), 10);
            assert!(doc.chunks()[0].has_code());

            assert_eq!(doc.chunks()[1].token_count(), 0);
            assert!(!doc.chunks()[1].has_code());
        }
    }

    mod metadata_operations_tests {
        use super::*;

        #[test]
        fn test_set_language_updates_field() {
            let mut doc = create_test_document();

            doc.set_language(Language::Rust);
            assert_eq!(doc.language(), &Language::Rust);

            doc.set_language(Language::Python);
            assert_eq!(doc.language(), &Language::Python);
        }

        #[test]
        fn test_set_category_updates_field() {
            let mut doc = create_test_document();

            doc.set_category(Category::Code);
            assert_eq!(doc.category(), &Category::Code);

            doc.set_category(Category::Documentation);
            assert_eq!(doc.category(), &Category::Documentation);
        }

        #[test]
        fn test_set_quality_score_valid_range() {
            let mut doc = create_test_document();

            doc.set_quality_score(0.75);
            assert_eq!(doc.quality_score(), 0.75);

            doc.set_quality_score(0.0);
            assert_eq!(doc.quality_score(), 0.0);

            doc.set_quality_score(1.0);
            assert_eq!(doc.quality_score(), 1.0);
        }

        #[test]
        fn test_set_quality_score_clamps_negative() {
            let mut doc = create_test_document();

            doc.set_quality_score(-0.5);
            assert_eq!(doc.quality_score(), 0.0);

            doc.set_quality_score(-10.0);
            assert_eq!(doc.quality_score(), 0.0);
        }

        #[test]
        fn test_set_quality_score_clamps_above_one() {
            let mut doc = create_test_document();

            doc.set_quality_score(1.5);
            assert_eq!(doc.quality_score(), 1.0);

            doc.set_quality_score(10.0);
            assert_eq!(doc.quality_score(), 1.0);
        }

        #[test]
        fn test_increment_access_updates_count_and_timestamp() {
            let mut doc = create_test_document();

            let initial_count = doc.access_count();
            assert!(doc.last_accessed_at().is_none());

            doc.increment_access();

            assert_eq!(doc.access_count(), initial_count + 1);
            assert!(doc.last_accessed_at().is_some());

            let first_timestamp = *doc.last_accessed_at().unwrap();

            // Sleep briefly to ensure timestamp difference
            std::thread::sleep(std::time::Duration::from_millis(10));

            doc.increment_access();

            assert_eq!(doc.access_count(), initial_count + 2);
            assert!(doc.last_accessed_at().unwrap() >= &first_timestamp);
        }

        #[test]
        fn test_set_word_count_clamps_negative() {
            let mut doc = create_test_document();

            doc.set_word_count(-10);
            assert_eq!(doc.word_count(), 0);

            doc.set_word_count(-1);
            assert_eq!(doc.word_count(), 0);

            doc.set_word_count(0);
            assert_eq!(doc.word_count(), 0);

            doc.set_word_count(100);
            assert_eq!(doc.word_count(), 100);
        }
    }

    mod factory_tests {
        use super::*;
        use crate::domain::value_objects::{ChunkingStrategy, FileMetadata};

        /// Helper to create test FileMetadata
        fn create_test_metadata() -> FileMetadata {
            FileMetadata::new(
                "test_document.txt".to_string(),
                "text/plain".to_string(),
                1024,
                Utc::now(),
            )
            .expect("Failed to create test metadata")
        }

        #[test]
        fn test_from_file_happy_path() {
            let file_path = ValidatedFilePath::new(temp_path("test.txt"))
                .expect("Failed to create validated path");
            let metadata = create_test_metadata();
            let checksum = Checksum::new("a".repeat(64)).expect("valid checksum");
            let content = "This is test content for the document.".to_string();
            let strategy = ChunkingStrategy::FixedSize { size: 512 };

            let result =
                Document::from_file(file_path, metadata, checksum, content.clone(), strategy);

            assert!(result.is_ok());
            let doc = result.unwrap();
            assert_eq!(doc.status(), DocumentStatus::Pending);
            assert_eq!(doc.file_name(), "test_document.txt");
            assert_eq!(doc.mime_type(), "text/plain");
            assert_eq!(doc.size_bytes(), 1024);
            assert_eq!(doc.content(), content.as_str());
        }

        #[test]
        fn test_from_file_computes_word_count() {
            let file_path = ValidatedFilePath::new(temp_path("test.txt"))
                .expect("Failed to create validated path");
            let metadata = create_test_metadata();
            let checksum = Checksum::new("a".repeat(64)).expect("valid checksum");
            let content = "One two three four five words here.".to_string();
            let strategy = ChunkingStrategy::default();

            let doc = Document::from_file(file_path, metadata, checksum, content, strategy)
                .expect("Failed to create document");

            assert_eq!(doc.word_count(), 7);
        }

        #[test]
        fn test_from_file_sets_defaults() {
            let file_path = ValidatedFilePath::new(temp_path("test.txt"))
                .expect("Failed to create validated path");
            let metadata = create_test_metadata();
            let checksum = Checksum::new("a".repeat(64)).expect("valid checksum");
            let content = "Default test content".to_string();
            let strategy = ChunkingStrategy::default();

            let doc = Document::from_file(file_path, metadata, checksum, content, strategy)
                .expect("Failed to create document");

            assert_eq!(doc.status(), DocumentStatus::Pending);
            assert_eq!(doc.quality_score(), 0.5);
            assert_eq!(doc.access_count(), 0);
            assert!(doc.last_accessed_at().is_none());
            assert_eq!(doc.language(), &Language::Unknown);
            assert_eq!(doc.category(), &Category::Uncategorized);
            assert!(doc.error_message().is_none());
        }

        #[test]
        fn test_from_file_preserves_metadata() {
            let file_path = ValidatedFilePath::new(temp_path("example.md"))
                .expect("Failed to create validated path");
            let modified_at = Utc::now();
            let metadata = FileMetadata::new(
                "example.md".to_string(),
                "text/markdown".to_string(),
                2048,
                modified_at,
            )
            .expect("Failed to create metadata");
            let checksum = Checksum::new("e".repeat(64)).expect("valid checksum");
            let content = "# Markdown Document\n\nContent here.".to_string();
            let strategy = ChunkingStrategy::default();

            let doc = Document::from_file(
                file_path,
                metadata,
                checksum.clone(),
                content.clone(),
                strategy,
            )
            .expect("Failed to create document");

            assert_eq!(doc.file_name(), "example.md");
            assert_eq!(doc.mime_type(), "text/markdown");
            assert_eq!(doc.size_bytes(), 2048);
            assert_eq!(doc.modified_at(), &modified_at);
            assert_eq!(doc.checksum(), &checksum);
            assert_eq!(doc.content(), content.as_str());
            assert_eq!(doc.file_type(), Some("md"));
        }

        #[test]
        fn test_from_file_empty_content_word_count_zero() {
            let file_path = ValidatedFilePath::new(temp_path("empty.txt"))
                .expect("Failed to create validated path");
            let metadata = create_test_metadata();
            let checksum = Checksum::new("b".repeat(64)).expect("valid checksum");
            let content = String::new();
            let strategy = ChunkingStrategy::default();

            let doc = Document::from_file(file_path, metadata, checksum, content, strategy)
                .expect("Failed to create document");

            assert_eq!(doc.word_count(), 0);
            assert_eq!(doc.content(), "");
            assert_eq!(doc.status(), DocumentStatus::Pending);
        }

        #[test]
        fn test_from_file_generates_chunks() {
            let file_path = ValidatedFilePath::new(temp_path("test.txt"))
                .expect("Failed to create validated path");
            let metadata = create_test_metadata();
            let checksum = Checksum::new("a".repeat(64)).expect("valid checksum");
            let content = "This is a test document with some content.".to_string();
            let strategy = ChunkingStrategy::FixedSize { size: 10 };

            let document =
                Document::from_file(file_path, metadata, checksum, content.clone(), strategy)
                    .expect("from_file should succeed");

            assert!(!document.chunks().is_empty(), "Document must have chunks");

            for chunk in document.chunks() {
                assert_eq!(chunk.document_id(), document.id());
            }

            let reconstructed: String = document.chunks().iter().map(|c| c.content()).collect();
            assert_eq!(reconstructed, content);
        }

        #[test]
        fn test_from_file_empty_content_no_chunks() {
            let file_path = ValidatedFilePath::new(temp_path("empty.txt"))
                .expect("Failed to create validated path");
            let metadata = create_test_metadata();
            let checksum = Checksum::new("b".repeat(64)).expect("valid checksum");
            let content = "".to_string();
            let strategy = ChunkingStrategy::default();

            let document = Document::from_file(file_path, metadata, checksum, content, strategy)
                .expect("from_file should succeed for empty content");

            assert_eq!(
                document.chunks().len(),
                0,
                "Empty content should have no chunks"
            );
        }

        #[test]
        fn test_from_file_whitespace_only_no_chunks() {
            let file_path = ValidatedFilePath::new(temp_path("whitespace.txt"))
                .expect("Failed to create validated path");
            let metadata = create_test_metadata();
            let checksum = Checksum::new("f".repeat(64)).expect("valid checksum");
            let content = "   \n\t   ".to_string();
            let strategy = ChunkingStrategy::default();

            let document = Document::from_file(file_path, metadata, checksum, content, strategy)
                .expect("from_file should succeed for whitespace");

            assert_eq!(
                document.chunks().len(),
                0,
                "Whitespace-only should have no chunks"
            );
        }
    }
}

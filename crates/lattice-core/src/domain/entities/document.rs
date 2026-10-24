//! Document entity and related classifications.

use crate::domain::entities::chunk::Chunk;
use crate::domain::value_objects::checksum::Checksum;
use crate::shared::error::Result;
use crate::shared::types::{DocumentId, TagId, ValidatedFilePath};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::Path;

mod classification;
mod factory;

pub use classification::{Category, DocumentStatus, Language};
pub use factory::StoredDocument;

/// Document entity representing an indexed file with metadata and chunks.
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
    pub fn set_chunk_token_count(&mut self, index: usize, token_count: i32) {
        if let Some(chunk) = self.chunks.get_mut(index) {
            chunk.set_token_count(token_count);
        }
    }

    /// Set word count for a specific chunk by index.
    pub fn set_chunk_word_count(&mut self, index: usize, word_count: usize) {
        if let Some(chunk) = self.chunks.get_mut(index) {
            chunk.set_word_count(word_count);
        }
    }

    /// Set code detection flag for a specific chunk by index.
    pub fn set_chunk_has_code(&mut self, index: usize, has_code: bool) {
        if let Some(chunk) = self.chunks.get_mut(index) {
            chunk.set_has_code(has_code);
        }
    }

    /// Set section/heading for a specific chunk by index.
    pub fn set_chunk_section(&mut self, index: usize, section: Option<String>) {
        if let Some(chunk) = self.chunks.get_mut(index) {
            chunk.set_section(section);
        }
    }

    /// Mark the chunks cut from `pages`, the one-based pages OCR read, and
    /// clear the mark on every other chunk.
    ///
    /// A chunk never crosses a page, so its page number decides it. A chunk
    /// with no page is never OCR text.
    pub fn mark_ocr_pages(&mut self, pages: &[u32]) {
        for chunk in &mut self.chunks {
            let from_ocr = chunk
                .page_number()
                .is_some_and(|page| pages.contains(&page));
            chunk.set_from_ocr(from_ocr);
        }
    }

    /// Mark document for reindexing with new content.
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
mod property_tests;

#[cfg(test)]
mod unit_tests;

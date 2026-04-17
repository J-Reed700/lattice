//! Content extraction port for the application layer.
//!
//! This port defines the interface for extracting text content from various file types.
//! Infrastructure implementations handle file type detection, binary parsing, and text extraction.

use crate::shared::error::Result;
use async_trait::async_trait;
use std::path::Path;

/// Extracted content with metadata.
#[derive(Debug, Clone)]
pub struct ExtractedContentData {
    /// The extracted text content
    pub text: String,
    /// MIME type of the source document
    pub mime_type: String,
    /// Number of pages (if applicable)
    pub page_count: Option<usize>,
    /// Word count
    pub word_count: usize,
    /// Character count
    pub char_count: usize,
}

/// Port for content extraction operations.
///
/// Implementations must:
/// - Detect file type automatically
/// - Handle binary files (PDF, DOCX) correctly
/// - Extract text from various formats
/// - Be thread-safe (`Send + Sync`)
#[async_trait]
pub trait ContentExtractionPort: Send + Sync {
    /// Extract text content from a file.
    ///
    /// # Arguments
    ///
    /// * `path` - The file path to extract from
    ///
    /// # Returns
    ///
    /// Extracted text content with metadata
    ///
    /// # Errors
    ///
    /// - `AppError::UnsupportedFileType` if file type not supported
    /// - `AppError::FileNotFound` if file does not exist
    /// - `AppError::ContentExtraction` if extraction fails
    async fn extract_content(&self, path: &Path) -> Result<ExtractedContentData>;

    /// Check if a file type is supported.
    fn is_supported(&self, path: &Path) -> bool;
}

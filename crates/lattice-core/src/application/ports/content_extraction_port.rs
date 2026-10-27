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
    /// One-based physical PDF page and UTF-8 byte offsets into text.
    pub page_ranges: Vec<(usize, usize, usize)>,
    /// One-based pages whose text OCR read off a scan; empty for every other
    /// document. Their chunks are marked as OCR text.
    pub ocr_pages: Vec<u32>,
    /// Word count
    pub word_count: usize,
    /// Character count
    pub char_count: usize,
}

/// Text extraction from files of various formats, binary ones (PDF, DOCX)
/// included.
#[async_trait]
pub trait ContentExtractionPort: Send + Sync {
    /// The text content of the file at `path`.
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

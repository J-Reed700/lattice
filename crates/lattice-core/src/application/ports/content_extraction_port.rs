//! Content extraction port for extracting text content from various file types.

use crate::shared::error::Result;
use async_trait::async_trait;
use std::path::Path;

/// Extracted content with metadata.
#[derive(Debug, Clone)]
pub struct ExtractedContentData {
    pub text: String,
    pub mime_type: String,
    pub page_count: Option<usize>,
    /// One-based physical PDF page and UTF-8 byte offsets into text.
    pub page_ranges: Vec<(usize, usize, usize)>,
    /// One-based pages whose text OCR read off a scan; empty for every other
    /// document. Their chunks are marked as OCR text.
    pub ocr_pages: Vec<u32>,
    pub word_count: usize,
    pub char_count: usize,
}

/// Port for content extraction operations.
#[async_trait]
pub trait ContentExtractionPort: Send + Sync {
    /// Extract text content and metadata from a file.
    async fn extract_content(&self, path: &Path) -> Result<ExtractedContentData>;

    /// Check if a file type is supported for extraction.
    fn is_supported(&self, path: &Path) -> bool;
}

//! Shared types for content extraction.

use serde::{Deserialize, Serialize};

/// Extracted content from a document with metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtractedContent {
    /// The extracted text content
    pub text: String,
    /// MIME type of the source document
    pub mime_type: String,
    /// Metadata about the extracted content
    pub metadata: ContentMetadata,
    /// Page ranges: (page_num, start_pos, end_pos) in the text
    pub page_ranges: Vec<(usize, usize, usize)>,
    /// One-based pages that carry an image but no selectable text, and that no
    /// OCR provider recognized. The document is still indexed; these pages are
    /// simply not represented in `text`, so a caller can offer to re-run them
    /// once a vision model is installed. Empty for every non-paged format.
    #[serde(default)]
    pub needs_ocr: Vec<u32>,
}

/// Metadata about extracted content.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContentMetadata {
    /// Number of pages in the document (if applicable)
    pub page_count: Option<usize>,
    /// Number of words in the extracted text
    pub word_count: usize,
    /// Number of characters in the extracted text
    pub char_count: usize,
    /// Detected language (if applicable)
    pub language: Option<String>,
}

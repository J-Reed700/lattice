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

//! Source citations: what an answer cites, as chat, journal pages and the
//! frontend exchange it.
//!
//! A [`SourceDto`] is one cited document (with one or more chunks); a cited
//! web page also carries the [`WebSnapshotDto`] it was read from.

use serde::{Deserialize, Serialize};

/// Source citation for Q&A response.
///
/// Represents a document chunk used as context for the answer.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SourceChunkExcerptDto {
    /// Chunk ID
    pub chunk_id: String,

    /// Excerpt text for this chunk
    pub excerpt: String,

    /// Optional section/heading for the chunk
    #[serde(skip_serializing_if = "Option::is_none")]
    pub section: Option<String>,

    /// Optional chunk index within the document
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chunk_index: Option<usize>,
    /// One-based physical PDF page, independently of printed page labels.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page_number: Option<u32>,

    /// Relevance score for this chunk
    pub score: f32,

    /// Optional highlight terms for excerpt rendering
    #[serde(skip_serializing_if = "Option::is_none")]
    pub highlights: Option<Vec<String>>,
}

/// Source citation for Q&A response.
///
/// Represents a document (with one or more chunks) used as context for the answer.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SourceDto {
    /// Document ID
    pub document_id: String,

    /// Chunk ID
    pub chunk_id: String,

    /// Content of the chunk
    pub content: String,

    /// Relevance score for this source
    pub score: f32,

    /// Optional file path
    pub path: Option<String>,

    /// Optional position within document
    pub position: Option<usize>,

    // Metadata used by citation footnotes
    /// File name
    pub file_name: String,

    /// Full file path
    pub file_path: String,

    /// MIME type (e.g., "application/pdf", "text/markdown")
    pub mime_type: String,

    /// Human-readable category (e.g., "PDF Document", "Markdown")
    pub category: String,

    /// File size in bytes
    pub file_size_bytes: i64,

    /// Last modified timestamp (ISO 8601)
    pub modified_at: String,

    /// Optional excerpt (query-centered snippet)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub excerpt: Option<String>,

    /// Optional highlight terms for excerpt rendering
    #[serde(skip_serializing_if = "Option::is_none")]
    pub highlights: Option<Vec<String>>,

    /// Optional section/heading for the chunk
    #[serde(skip_serializing_if = "Option::is_none")]
    pub section: Option<String>,

    /// Optional chunk index within the document
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chunk_index: Option<usize>,
    /// One-based physical PDF page, independently of printed page labels.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page_number: Option<u32>,
    /// The passage was read off a scanned page by OCR, so odd characters in it
    /// may be recognition errors.
    #[serde(default)]
    pub from_ocr: bool,

    /// Optional chunk excerpts grouped under this source document.
    /// Includes the primary chunk and any additional supporting chunks.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chunk_excerpts: Option<Vec<SourceChunkExcerptDto>>,

    /// The number the model was told to cite this source as, i.e. the `n` in
    /// a `[n]` footnote.
    ///
    /// Assigned once, over this (deduplicated) source list, and then used
    /// verbatim when building the prompt. Previously the two sides numbered
    /// independently: the prompt numbered the **budgeted per-chunk** list
    /// while the UI resolved `[n]` by position in the **per-document,
    /// score-sorted** list. Any answer citing a document that contributed
    /// more than one chunk — or produced after budget-trimming dropped
    /// chunks — therefore rendered footnotes that opened the wrong document.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub citation_id: Option<u32>,

    /// Durable copy of a cited web page as it was read. The short `content`
    /// and `excerpt` fields keep their existing meanings for prompts/UI.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub web_snapshot: Option<WebSnapshotDto>,
}

/// Page evidence captured for a web citation, independent of the live site.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct WebSnapshotDto {
    pub url: String,
    pub title: Option<String>,
    pub text: String,
    pub fetched_at: Option<String>,
    pub truncated: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_source_dto_serialization() {
        let source = SourceDto {
            page_number: None,
            from_ocr: false,
            document_id: "doc-789".to_string(),
            chunk_id: "chunk-101".to_string(),
            content: "Relevant content".to_string(),
            score: 0.92,
            path: Some("/path/to/doc.txt".to_string()),
            position: Some(2),
            file_name: "doc.txt".to_string(),
            file_path: "/path/to/doc.txt".to_string(),
            mime_type: "text/plain".to_string(),
            category: "Text File".to_string(),
            file_size_bytes: 2048,
            modified_at: "2024-01-15T12:00:00Z".to_string(),
            excerpt: None,
            highlights: None,
            section: None,
            chunk_index: None,
            chunk_excerpts: None,
            citation_id: None,

            web_snapshot: None,
        };

        let json = serde_json::to_string(&source).unwrap();
        let deserialized: SourceDto = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.document_id, "doc-789");
        assert_eq!(deserialized.score, 0.92);
    }
}

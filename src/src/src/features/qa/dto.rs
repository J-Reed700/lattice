//! # Q&A DTOs
//!
//! Data Transfer Objects for question-answering operations.
//!
//! These DTOs handle requests and responses for RAG-based Q&A.

use serde::{Deserialize, Serialize};

/// Request for question-answering.
///
/// Contains the question and optional parameters for context retrieval.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct QARequestDto {
    /// The question to answer
    pub question: String,

    /// Maximum number of context chunks to retrieve
    pub context_limit: Option<usize>,

    /// LLM model to use (optional, uses default if not specified)
    pub model: Option<String>,

    /// Optional temperature for LLM generation (0.0 to 1.0)
    pub temperature: Option<f32>,

    /// Optional maximum tokens in response
    pub max_tokens: Option<usize>,

    /// Optional list of base64-encoded images for multimodal models
    pub images: Option<Vec<String>>,
}

/// Response from question-answering.
///
/// Contains the generated answer and source citations.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct QAResponseDto {
    /// The generated answer
    pub answer: String,

    /// Source documents used for context
    pub sources: Vec<SourceDto>,

    /// Optional confidence score (0.0 to 1.0)
    pub confidence: Option<f32>,

    /// Optional metadata about the response
    pub metadata: Option<QAMetadataDto>,
}

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
}

/// Metadata about Q&A response generation.
///
/// Contains statistics and configuration used for generation.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct QAMetadataDto {
    /// Model used for generation
    pub model: String,

    /// Number of tokens in the prompt
    pub prompt_tokens: Option<usize>,

    /// Number of tokens in the response
    pub completion_tokens: Option<usize>,

    /// Total tokens used
    pub total_tokens: Option<usize>,

    /// Time taken to generate response (milliseconds)
    pub generation_time_ms: Option<u64>,
}

/// Stream chunk for streaming Q&A response.
///
/// Represents a partial update during RAG generation.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(tag = "type")] // Flattened structure: { "type": "token", "content": "..." }
#[serde(rename_all = "camelCase")]
pub enum StreamChunkDto {
    /// Source documents retrieved (sent before generation starts)
    Sources { sources: Vec<SourceDto> },

    /// Generated token (sent during generation)
    Token { content: String },

    /// Generation complete
    Done,

    /// Error occurred
    Error { error: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_qa_request_dto_serialization() {
        let request = QARequestDto {
            question: "What is machine learning?".to_string(),
            context_limit: Some(5),
            model: Some("gpt-4".to_string()),
            temperature: Some(0.7),
            max_tokens: Some(500),
            images: None,
        };

        let json = serde_json::to_string(&request).unwrap();
        let deserialized: QARequestDto = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.question, "What is machine learning?");
        assert_eq!(deserialized.context_limit, Some(5));
    }

    #[test]
    fn test_qa_response_dto_serialization() {
        let response = QAResponseDto {
            answer: "Machine learning is...".to_string(),
            sources: vec![SourceDto {
                page_number: None,
                document_id: "doc-123".to_string(),
                chunk_id: "chunk-456".to_string(),
                content: "ML context...".to_string(),
                score: 0.95,
                path: Some("/docs/ml.txt".to_string()),
                position: Some(0),
                file_name: "ml.txt".to_string(),
                file_path: "/docs/ml.txt".to_string(),
                mime_type: "text/plain".to_string(),
                category: "Text File".to_string(),
                file_size_bytes: 1024,
                modified_at: "2024-01-01T00:00:00Z".to_string(),
                excerpt: Some("ML context...".to_string()),
                highlights: Some(vec!["ml".to_string()]),
                section: Some("Introduction".to_string()),
                chunk_index: Some(0),
                chunk_excerpts: None,
                citation_id: None,
            }],
            confidence: Some(0.85),
            metadata: Some(QAMetadataDto {
                model: "gpt-4".to_string(),
                prompt_tokens: Some(100),
                completion_tokens: Some(50),
                total_tokens: Some(150),
                generation_time_ms: Some(1200),
            }),
        };

        let json = serde_json::to_string(&response).unwrap();
        let deserialized: QAResponseDto = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.answer, "Machine learning is...");
        assert_eq!(deserialized.sources.len(), 1);
    }

    #[test]
    fn test_source_dto_serialization() {
        let source = SourceDto {
            page_number: None,
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
        };

        let json = serde_json::to_string(&source).unwrap();
        let deserialized: SourceDto = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.document_id, "doc-789");
        assert_eq!(deserialized.score, 0.92);
    }
}

/// Serialized health response from the QA command.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct LLMHealthStatusDto {
    pub available: bool,
    pub model: String,
    pub max_context_tokens: usize,
    pub backend: String,
}

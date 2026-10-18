//! The rows the dense and keyword search engines return.
//!
//! Never glob-import this module: `domain::entities::SearchResult` is a
//! different type with the same name.

/// One hit from the dense or keyword engine, before it becomes a DTO.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SearchResult {
    pub id: String,
    pub score: f32,
    pub index: usize,

    pub filename: Option<String>,
    pub mime_type: Option<String>,
    pub size_bytes: Option<i64>,
    pub created_at: Option<String>,
    pub content: Option<String>,

    pub file_id: Option<String>,
    pub file_path: Option<String>,
    pub file_name: Option<String>,
    pub file_extension: Option<String>,
    pub file_category: Option<String>,
    pub is_indexed: Option<bool>,

    // Additional fields for search commands
    pub document_id: Option<String>,
    pub snippet: Option<String>,
    pub chunk_index: Option<usize>,
    pub updated_at: Option<String>,
}

/// One keyword (FTS5) hit.
#[derive(Debug, Clone)]
pub struct BM25Result {
    pub chunk_id: String,
    pub document_id: String,
    pub score: f32,
    pub filename: Option<String>,
    pub mime_type: Option<String>,
    pub size_bytes: Option<i64>,
    pub created_at: Option<String>,
    pub content: Option<String>,
}

impl From<BM25Result> for SearchResult {
    fn from(result: BM25Result) -> Self {
        Self {
            id: result.chunk_id.clone(),
            score: result.score,
            index: 0, // BM25 doesn't use index
            filename: result.filename.clone(),
            mime_type: result.mime_type.clone(),
            size_bytes: result.size_bytes,
            created_at: result.created_at.clone(),
            content: result.content.clone(),
            file_id: Some(result.document_id.clone()),
            file_path: None,
            file_name: result.filename,
            file_extension: None,
            file_category: None,
            is_indexed: None,
            document_id: Some(result.document_id),
            snippet: None,
            chunk_index: None,
            updated_at: None,
        }
    }
}

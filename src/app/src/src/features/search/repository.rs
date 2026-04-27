use crate::error::AppError;
use async_trait::async_trait;

/// Search result for BM25 and hybrid search queries
#[derive(Debug, Clone)]
pub struct SearchResult {
    pub document_id: String,
    pub score: f32,
    pub filename: Option<String>,
    pub mime_type: Option<String>,
    pub size_bytes: Option<i64>,
    pub created_at: Option<String>,
    pub content: Option<String>,
}

/// Repository for search operations (BM25, FTS5, hybrid search)
#[async_trait]
pub trait SearchRepository: Send + Sync + std::fmt::Debug {
    /// Perform BM25 full-text search
    async fn search_bm25(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>, AppError>;

    /// Get total count of searchable chunks
    async fn count_searchable_chunks(&self) -> Result<i64, AppError>;

    /// Optimize the search index
    async fn optimize_index(&self) -> Result<(), AppError>;

    /// Rebuild the search index
    async fn rebuild_index(&self) -> Result<(), AppError>;
}

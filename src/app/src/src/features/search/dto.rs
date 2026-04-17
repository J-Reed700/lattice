//! # Search DTOs
//!
//! Data Transfer Objects for search operations across application boundaries.
//!
//! These DTOs provide a flat, serialization-friendly representation of search
//! requests and responses, decoupled from domain models.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub use crate::features::cache::dto::CacheStatsDto;

/// Search result from infrastructure ports.
///
/// This is a simple DTO used at port boundaries to avoid domain dependencies.
/// Infrastructure adapters return this, and use cases map it to domain entities.
///
/// # Design Rationale
///
/// - **Decoupling**: Ports don't depend on domain entities (Clean Architecture)
/// - **Simplicity**: Contains only essential fields needed from search engines
/// - **Flexibility**: Can be returned by various search implementations (vector, text, hybrid)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResultPortDto {
    /// Document identifier (may be chunk ID or document ID)
    pub doc_id: String,

    /// Chunk identifier (empty string if result is a document, not a chunk)
    pub chunk_id: String,

    /// Relevance score (0.0 to 1.0)
    pub score: f32,

    /// Content snippet or text
    pub content: String,
}

/// Search request from the frontend.
///
/// Represents a search query with optional parameters for filtering and ranking.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct SearchRequestDto {
    /// The search query text
    pub query: String,

    /// Maximum number of results to return
    pub limit: Option<usize>,

    /// Minimum similarity threshold (0.0 to 1.0)
    pub threshold: Option<f32>,

    /// Search algorithm mode
    pub mode: SearchModeDto,
}

/// Search mode for algorithm selection.
///
/// Maps to domain SearchMode but uses simple serializable types.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum SearchModeDto {
    /// Vector-based semantic search
    Vector,

    /// Keyword-based BM25 search
    BM25,

    /// Hybrid search with weights
    Hybrid {
        vector_weight: f32,
        bm25_weight: f32,
    },
}

/// Search response to the frontend.
///
/// Contains results and metadata about the search operation.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct SearchResponseDto {
    /// Search results
    pub results: Vec<SearchResultDto>,

    /// Total number of results (before limit)
    pub total: usize,

    /// Query execution time in milliseconds
    pub query_time_ms: u64,
}

/// Individual search result.
///
/// Flat representation of a search result for JSON serialization.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SearchResultDto {
    /// Result identifier (chunk ID or document ID)
    pub id: String,

    /// Document title or filename
    pub title: String,

    /// Content snippet or preview
    pub content: String,

    /// Relevance score (0.0 to 1.0)
    pub score: f32,

    /// Optional file path
    pub path: Option<String>,

    /// Optional parent document ID (for chunk results)
    pub document_id: Option<String>,

    /// Optional position within document
    pub position: Option<usize>,

    /// Optional vector similarity score
    pub vector_score: Option<f32>,

    /// Optional BM25 keyword score
    pub bm25_score: Option<f32>,

    /// Optional vector search rank
    pub vector_rank: Option<usize>,

    /// Optional BM25 search rank
    pub bm25_rank: Option<usize>,

    /// Document metadata (filename, file_type, size, etc.)
    #[specta(skip)]
    pub metadata: HashMap<String, serde_json::Value>,
}

/// Search options for advanced search.
///
/// Provides filtering, pagination, and mode selection.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SearchOptions {
    /// The search query text
    pub query: String,

    /// Maximum number of results
    pub limit: Option<usize>,

    /// Optional metadata filters (JSON string)
    pub filter: Option<std::collections::HashMap<String, String>>,

    /// Search mode ("semantic", "keyword", "hybrid")
    pub search_mode: Option<String>,
}

/// Recency search options for time-aware search.
///
/// Boosts recent documents in search results.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RecencySearchOptions {
    /// The search query text
    pub query: String,

    /// Maximum number of results
    pub limit: Option<usize>,

    /// Weight for recency boost (0.0 to 1.0)
    pub recency_weight: Option<f32>,

    /// Maximum age in days to consider
    pub max_age_days: Option<i64>,
}

/// Enhanced search response with caching metadata.
///
/// Includes cache statistics and execution metrics.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct EnhancedSearchResponse {
    /// Search results
    pub results: Vec<SearchResultDto>,

    /// Whether results came from cache
    pub from_cache: bool,

    /// Query execution time in milliseconds
    pub execution_time_ms: u64,

    /// Optional cache statistics
    pub cache_stats: Option<CacheStatsDto>,
}

// CacheStatsDto is defined in cache_dto.rs and re-exported from dtos/mod.rs

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_search_result_port_dto_serialization() {
        let port_dto = SearchResultPortDto {
            doc_id: "doc-123".to_string(),
            chunk_id: "chunk-456".to_string(),
            score: 0.95,
            content: "Test content from port".to_string(),
        };

        let json = serde_json::to_string(&port_dto).unwrap();
        let deserialized: SearchResultPortDto = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.doc_id, "doc-123");
        assert_eq!(deserialized.chunk_id, "chunk-456");
        assert_eq!(deserialized.score, 0.95);
        assert_eq!(deserialized.content, "Test content from port");
    }

    #[test]
    fn test_search_request_dto_serialization() {
        let request = SearchRequestDto {
            query: "test query".to_string(),
            limit: Some(10),
            threshold: Some(0.7),
            mode: SearchModeDto::Vector,
        };

        let json = serde_json::to_string(&request).unwrap();
        let deserialized: SearchRequestDto = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.query, "test query");
        assert_eq!(deserialized.limit, Some(10));
    }

    #[test]
    fn test_search_mode_dto_serialization() {
        let modes = vec![
            SearchModeDto::Vector,
            SearchModeDto::BM25,
            SearchModeDto::Hybrid {
                vector_weight: 0.7,
                bm25_weight: 0.3,
            },
        ];

        for mode in modes {
            let json = serde_json::to_string(&mode).unwrap();
            let _deserialized: SearchModeDto = serde_json::from_str(&json).unwrap();
        }
    }

    #[test]
    fn test_search_response_dto_serialization() {
        let mut metadata = HashMap::new();
        metadata.insert("filename".to_string(), serde_json::json!("doc.txt"));
        metadata.insert("file_type".to_string(), serde_json::json!("text/plain"));

        let response = SearchResponseDto {
            results: vec![SearchResultDto {
                id: "chunk-123".to_string(),
                title: "Test Document".to_string(),
                content: "Test content".to_string(),
                score: 0.95,
                path: Some("/path/to/doc.txt".to_string()),
                document_id: Some("doc-456".to_string()),
                position: Some(0),
                vector_score: Some(0.92),
                bm25_score: Some(0.88),
                vector_rank: Some(1),
                bm25_rank: Some(2),
                metadata,
            }],
            total: 1,
            query_time_ms: 42,
        };

        let json = serde_json::to_string(&response).unwrap();
        let deserialized: SearchResponseDto = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.results.len(), 1);
        assert_eq!(deserialized.total, 1);
        assert_eq!(deserialized.query_time_ms, 42);
    }
}

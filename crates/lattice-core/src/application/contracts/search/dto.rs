//! Search requests and responses as the frontend, chat and the tool executor
//! exchange them: a flat, serialization-friendly shape, decoupled from the
//! domain's search models.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Search request from the frontend.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct SearchRequestDto {
    pub query: String,
    pub limit: Option<usize>,
    pub threshold: Option<f32>,
    pub mode: SearchModeDto,
}

impl SearchRequestDto {
    /// A hybrid search at the application's fusion weights.
    pub fn hybrid(query: impl Into<String>, limit: usize) -> Self {
        Self {
            query: query.into(),
            limit: Some(limit),
            threshold: None,
            mode: SearchModeDto::Hybrid {
                vector_weight: crate::shared::constants::DEFAULT_VECTOR_FUSION_WEIGHT,
                bm25_weight: crate::shared::constants::DEFAULT_KEYWORD_FUSION_WEIGHT,
            },
        }
    }
}

/// Search mode for algorithm selection.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum SearchModeDto {
    Vector,
    BM25,
    Hybrid {
        vector_weight: f32,
        bm25_weight: f32,
    },
}

/// Search response to the frontend.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct SearchResponseDto {
    pub results: Vec<SearchResultDto>,
    pub total: usize,
    pub query_time_ms: u64,
}

/// Individual search result for serialization.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SearchResultDto {
    pub id: String,
    pub title: String,
    pub content: String,
    pub score: f32,
    pub path: Option<String>,
    pub document_id: Option<String>,
    pub position: Option<usize>,
    pub vector_score: Option<f32>,
    pub bm25_score: Option<f32>,
    pub vector_rank: Option<usize>,
    pub bm25_rank: Option<usize>,
    #[specta(skip)]
    pub metadata: HashMap<String, serde_json::Value>,
}

/// Search options with filtering, pagination, and mode selection.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SearchOptions {
    pub query: String,
    pub limit: Option<usize>,
    pub filter: Option<std::collections::HashMap<String, String>>,
    pub search_mode: Option<String>,
}

/// Recency search options for time-aware search boosting recent documents.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RecencySearchOptions {
    pub query: String,
    pub limit: Option<usize>,
    pub recency_weight: Option<f32>,
    pub max_age_days: Option<i64>,
}

/// Enhanced search response with caching metadata.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct EnhancedSearchResponse {
    pub results: Vec<SearchResultDto>,
    pub from_cache: bool,
    pub execution_time_ms: u64,
    pub cache_stats: Option<CacheStatsDto>,
}

/// The query cache's statistics, reported with each search response.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CacheStatsDto {
    pub size: usize,
    pub capacity: usize,
    pub hits: u64,
    pub misses: u64,
    pub total_time_saved_ms: u64,
    pub hit_rate: f64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::contracts::search::SearchResultRecord;

    #[test]
    fn test_search_result_port_dto_serialization() {
        let port_dto = SearchResultRecord {
            doc_id: "doc-123".to_string(),
            chunk_id: "chunk-456".to_string(),
            score: 0.95,
            content: "Test content from port".to_string(),
        };

        let json = serde_json::to_string(&port_dto).unwrap();
        let deserialized: SearchResultRecord = serde_json::from_str(&json).unwrap();

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

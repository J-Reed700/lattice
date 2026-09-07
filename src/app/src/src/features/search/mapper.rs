//! # Search Mapper
//!
//! Converts between domain search models and DTOs.
//!
//! This mapper handles bidirectional conversion between:
//! - `SearchResult` (domain) ↔ `SearchResultDto` (DTO)
//! - `SearchMode` (domain) ↔ `SearchModeDto` (DTO)
//!
//! Also provides utility functions for metadata enrichment.

use crate::domain::entities::search_result::SearchResult;
use crate::domain::value_objects::search_mode::SearchMode;
use crate::domain::value_objects::search_query::SearchQuery;
use crate::features::search::dto::{
    SearchModeDto, SearchRequestDto, SearchResponseDto, SearchResultDto, SearchResultPortDto,
};
use crate::shared::error::Result;

/// Mapper for search-related conversions.
pub struct SearchMapper;

/// Infer human-readable category from MIME type.
///
/// # Arguments
///
/// * `mime_type` - MIME type string (e.g., "application/pdf", "text/markdown")
///
/// # Returns
///
/// Human-readable category string
///
/// # Examples
///
/// ```
/// use lattice::application::mappers::search_mapper::infer_category;
///
/// assert_eq!(infer_category("application/pdf"), "PDF Document");
/// assert_eq!(infer_category("text/markdown"), "Markdown");
/// assert_eq!(infer_category("text/x-rust"), "Code");
/// ```
pub fn infer_category(mime_type: &str) -> String {
    match mime_type {
        "application/pdf" => "PDF Document",
        "text/markdown" | "text/x-markdown" => "Markdown",
        "text/plain" => "Text File",
        t if t.starts_with("text/x-") => "Code",
        t if t.starts_with("image/") => "Image",
        t if t.starts_with("video/") => "Video",
        t if t.starts_with("audio/") => "Audio",
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document" => {
            "Word Document"
        }
        "application/msword" => "Word Document",
        "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet" => "Excel Spreadsheet",
        "application/vnd.ms-excel" => "Excel Spreadsheet",
        "application/json" => "JSON",
        "application/xml" | "text/xml" => "XML",
        "text/html" => "HTML",
        "text/css" => "CSS",
        "application/javascript" | "text/javascript" => "JavaScript",
        _ => "Document",
    }
    .to_string()
}

impl SearchMapper {
    /// Convert domain SearchResult to DTO.
    ///
    /// # Arguments
    ///
    /// * `result` - Domain search result
    ///
    /// # Returns
    ///
    /// Flat DTO representation for JSON serialization
    pub fn to_dto(result: SearchResult) -> SearchResultDto {
        SearchResultDto {
            id: result.id().to_string(),
            title: result
                .file_path()
                .and_then(|p| {
                    std::path::Path::new(p)
                        .file_name()
                        .and_then(|n| n.to_str())
                        .map(|s| s.to_string())
                })
                .unwrap_or_else(|| result.id().to_string()),
            content: result.snippet().unwrap_or("").to_string(),
            score: result.score(),
            path: result.file_path().map(|s| s.to_string()),
            document_id: result.document_id().map(|s| s.to_string()),
            position: result.position(),
            vector_score: None,
            bm25_score: None,
            vector_rank: None,
            bm25_rank: None,
            metadata: std::collections::HashMap::new(),
        }
    }

    /// Convert DTO SearchResult to domain model.
    ///
    /// # Arguments
    ///
    /// * `dto` - DTO search result
    ///
    /// # Returns
    ///
    /// Domain SearchResult entity
    ///
    /// # Returns
    ///
    /// Domain SearchResult entity. If score validation fails, returns result with
    /// score clamped to 0.0.
    pub fn to_domain(dto: SearchResultDto) -> SearchResult {
        // Clamp invalid scores to 0.0 rather than panicking
        let score = if dto.score.is_nan() || dto.score.is_infinite() {
            0.0
        } else {
            dto.score
        };
        SearchResult::with_metadata(
            dto.id,
            score,
            Some(dto.content),
            dto.document_id,
            dto.path,
            dto.position,
        )
        .unwrap_or_else(|_| SearchResult::default_invalid())
    }

    /// Convert port DTO to domain SearchResult.
    ///
    /// This is used at the port boundary to map infrastructure results
    /// to domain entities.
    ///
    /// # Arguments
    ///
    /// * `port_dto` - Port DTO from infrastructure layer
    ///
    /// # Returns
    ///
    /// Domain SearchResult entity
    ///
    /// # Design Rationale
    ///
    /// This method bridges the port boundary, ensuring infrastructure
    /// does not depend on domain entities (Clean Architecture principle).
    ///
    /// # Returns
    ///
    /// Domain SearchResult entity. If score validation fails, returns result with
    /// score clamped to 0.0.
    pub fn port_dto_to_domain(port_dto: SearchResultPortDto) -> SearchResult {
        // Clamp invalid scores to 0.0 rather than panicking
        let score = if port_dto.score.is_nan() || port_dto.score.is_infinite() {
            0.0
        } else {
            port_dto.score
        };
        SearchResult::with_metadata(
            port_dto.chunk_id.clone(),
            score,
            Some(port_dto.content),
            Some(port_dto.doc_id),
            None, // File path not available from port DTO
            None, // Position not available from port DTO
        )
        .unwrap_or_else(|_| SearchResult::default_invalid())
    }

    /// Convert multiple port DTOs to domain entities.
    ///
    /// # Arguments
    ///
    /// * `port_dtos` - Vector of port DTOs from infrastructure
    ///
    /// # Returns
    ///
    /// Vector of domain SearchResult entities
    pub fn port_dtos_to_domain(port_dtos: Vec<SearchResultPortDto>) -> Vec<SearchResult> {
        port_dtos
            .into_iter()
            .map(Self::port_dto_to_domain)
            .collect()
    }

    /// Convert multiple search results to response DTO.
    ///
    /// # Arguments
    ///
    /// * `results` - Vector of domain search results
    /// * `query_time_ms` - Query execution time in milliseconds
    ///
    /// # Returns
    ///
    /// Response DTO with results and metadata
    pub fn to_response_dto(results: Vec<SearchResult>, query_time_ms: u64) -> SearchResponseDto {
        let total = results.len();
        let result_dtos = results.into_iter().map(Self::to_dto).collect();

        SearchResponseDto {
            results: result_dtos,
            total,
            query_time_ms,
        }
    }

    /// Convert SearchMode domain to DTO.
    ///
    /// # Arguments
    ///
    /// * `mode` - Domain search mode
    ///
    /// # Returns
    ///
    /// DTO representation of search mode
    pub fn mode_to_dto(mode: SearchMode) -> SearchModeDto {
        match mode {
            SearchMode::Vector => SearchModeDto::Vector,
            SearchMode::BM25 => SearchModeDto::BM25,
            SearchMode::Hybrid {
                vector_weight,
                bm25_weight,
            } => SearchModeDto::Hybrid {
                vector_weight,
                bm25_weight,
            },
        }
    }

    /// Convert SearchModeDto to domain.
    ///
    /// # Arguments
    ///
    /// * `dto` - DTO search mode
    ///
    /// # Returns
    ///
    /// Domain SearchMode value object
    pub fn mode_to_domain(dto: SearchModeDto) -> SearchMode {
        match dto {
            SearchModeDto::Vector => SearchMode::Vector,
            SearchModeDto::BM25 => SearchMode::BM25,
            SearchModeDto::Hybrid {
                vector_weight,
                bm25_weight,
            } => SearchMode::Hybrid {
                vector_weight,
                bm25_weight,
            },
        }
    }

    /// Convert SearchRequestDto to domain SearchQuery.
    ///
    /// # Arguments
    ///
    /// * `dto` - Search request DTO
    ///
    /// # Returns
    ///
    /// Domain SearchQuery value object
    ///
    /// # Errors
    ///
    /// Returns error if query validation fails
    pub fn request_to_query(dto: SearchRequestDto) -> Result<SearchQuery> {
        SearchQuery::new(dto.query)
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_search_result_to_dto() {
        let result = SearchResult::with_metadata(
            "chunk-123".to_string(),
            0.95,
            Some("Test content".to_string()),
            Some("doc-456".to_string()),
            Some("/path/to/test.txt".to_string()),
            Some(0),
        )
        .expect("Test score is valid");

        let dto = SearchMapper::to_dto(result);

        assert_eq!(dto.id, "chunk-123");
        assert_eq!(dto.score, 0.95);
        assert_eq!(dto.content, "Test content");
        assert_eq!(dto.document_id, Some("doc-456".to_string()));
        assert_eq!(dto.path, Some("/path/to/test.txt".to_string()));
    }

    #[test]
    fn test_search_result_to_domain() {
        let dto = SearchResultDto {
            id: "chunk-789".to_string(),
            title: "Test".to_string(),
            content: "Content here".to_string(),
            score: 0.87,
            path: Some("/docs/file.txt".to_string()),
            document_id: Some("doc-101".to_string()),
            position: Some(2),
            vector_score: None,
            bm25_score: None,
            vector_rank: None,
            bm25_rank: None,
            metadata: std::collections::HashMap::new(),
        };

        let result = SearchMapper::to_domain(dto);

        assert_eq!(result.id(), "chunk-789");
        assert_eq!(result.score(), 0.87);
        assert_eq!(result.snippet(), Some("Content here"));
    }

    #[test]
    fn test_to_response_dto() {
        let results = vec![
            SearchResult::new("id-1".to_string(), 0.9, Some("content 1".to_string())).unwrap(),
            SearchResult::new("id-2".to_string(), 0.8, Some("content 2".to_string())).unwrap(),
        ];

        let response = SearchMapper::to_response_dto(results, 42);

        assert_eq!(response.results.len(), 2);
        assert_eq!(response.total, 2);
        assert_eq!(response.query_time_ms, 42);
    }

    #[test]
    fn test_mode_to_dto() {
        let vector = SearchMapper::mode_to_dto(SearchMode::Vector);
        assert!(matches!(vector, SearchModeDto::Vector));

        let bm25 = SearchMapper::mode_to_dto(SearchMode::BM25);
        assert!(matches!(bm25, SearchModeDto::BM25));

        let hybrid = SearchMapper::mode_to_dto(SearchMode::Hybrid {
            vector_weight: 0.7,
            bm25_weight: 0.3,
        });
        assert!(matches!(
            hybrid,
            SearchModeDto::Hybrid {
                vector_weight: 0.7,
                bm25_weight: 0.3
            }
        ));
    }

    #[test]
    fn test_mode_to_domain() {
        let vector = SearchMapper::mode_to_domain(SearchModeDto::Vector);
        assert!(matches!(vector, SearchMode::Vector));

        let bm25 = SearchMapper::mode_to_domain(SearchModeDto::BM25);
        assert!(matches!(bm25, SearchMode::BM25));

        let hybrid = SearchMapper::mode_to_domain(SearchModeDto::Hybrid {
            vector_weight: 0.6,
            bm25_weight: 0.4,
        });
        assert!(matches!(
            hybrid,
            SearchMode::Hybrid {
                vector_weight: 0.6,
                bm25_weight: 0.4
            }
        ));
    }

    #[test]
    fn test_request_to_query() {
        let request = SearchRequestDto {
            query: "test query".to_string(),
            limit: Some(5),
            threshold: Some(0.8),
            mode: SearchModeDto::Vector,
        };

        let query = SearchMapper::request_to_query(request).unwrap();

        assert_eq!(query.as_str(), "test query");
    }

    #[test]
    fn test_port_dto_to_domain() {
        let port_dto = SearchResultPortDto {
            doc_id: "doc-123".to_string(),
            chunk_id: "chunk-456".to_string(),
            score: 0.92,
            content: "Test content from port".to_string(),
        };

        let result = SearchMapper::port_dto_to_domain(port_dto);

        assert_eq!(result.id(), "chunk-456");
        assert_eq!(result.score(), 0.92);
        assert_eq!(result.snippet(), Some("Test content from port"));
        assert_eq!(result.document_id(), Some("doc-123"));
    }

    #[test]
    fn test_port_dtos_to_domain() {
        let port_dtos = vec![
            SearchResultPortDto {
                doc_id: "doc-1".to_string(),
                chunk_id: "chunk-1".to_string(),
                score: 0.95,
                content: "Content 1".to_string(),
            },
            SearchResultPortDto {
                doc_id: "doc-2".to_string(),
                chunk_id: "chunk-2".to_string(),
                score: 0.85,
                content: "Content 2".to_string(),
            },
        ];

        let results = SearchMapper::port_dtos_to_domain(port_dtos);

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].id(), "chunk-1");
        assert_eq!(results[0].score(), 0.95);
        assert_eq!(results[1].id(), "chunk-2");
        assert_eq!(results[1].score(), 0.85);
    }

    #[test]
    fn test_infer_category() {
        use super::infer_category;

        // Document types
        assert_eq!(infer_category("application/pdf"), "PDF Document");
        assert_eq!(infer_category("text/plain"), "Text File");
        assert_eq!(infer_category("text/markdown"), "Markdown");
        assert_eq!(infer_category("text/x-markdown"), "Markdown");

        // Office documents
        assert_eq!(
            infer_category(
                "application/vnd.openxmlformats-officedocument.wordprocessingml.document"
            ),
            "Word Document"
        );
        assert_eq!(infer_category("application/msword"), "Word Document");
        assert_eq!(
            infer_category("application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"),
            "Excel Spreadsheet"
        );

        // Code files
        assert_eq!(infer_category("text/x-rust"), "Code");
        assert_eq!(infer_category("text/x-python"), "Code");
        assert_eq!(infer_category("application/javascript"), "JavaScript");

        // Web formats
        assert_eq!(infer_category("text/html"), "HTML");
        assert_eq!(infer_category("text/css"), "CSS");
        assert_eq!(infer_category("application/json"), "JSON");
        assert_eq!(infer_category("application/xml"), "XML");

        // Media
        assert_eq!(infer_category("image/png"), "Image");
        assert_eq!(infer_category("video/mp4"), "Video");
        assert_eq!(infer_category("audio/mpeg"), "Audio");

        // Unknown
        assert_eq!(infer_category("application/octet-stream"), "Document");
        assert_eq!(infer_category("unknown/type"), "Document");
    }
}

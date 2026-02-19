//! # Search Result Entity
//!
//! Domain entity representing a search result.
//!
//! This is a pure domain entity that represents the result of a search operation,
//! containing the matched item's information and relevance score.

use serde::{Deserialize, Serialize};

use crate::application::dtos::search_dto::SearchResultPortDto;
use crate::shared::error::AppError;

type Result<T> = std::result::Result<T, AppError>;

/// Search result entity.
///
/// Represents a single result from a search operation, containing:
/// - The matched item's identifier
/// - Relevance score (0.0 to 1.0, higher is better)
/// - Optional metadata about the matched item
///
/// ## Pure Domain Model
///
/// This entity focuses on the business concept of a "search result" without
/// infrastructure concerns. The actual search implementation, ranking algorithms,
/// and data retrieval are handled by the infrastructure layer.
///
/// ## Example
///
/// ```rust,no_run
/// use vault_desktop::domain::entities::search_result::SearchResult;
///
/// let result = SearchResult::new(
///     "chunk-123".to_string(),
///     0.95,
///     Some("Important document content...".to_string()),
/// );
///
/// assert!(result.score() > 0.9);
/// assert!(result.is_relevant(0.8));
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResult {
    /// Unique identifier of the matched item (chunk, document, etc.)
    id: String,

    /// Relevance score (0.0 to 1.0, where 1.0 is perfect match)
    score: f32,

    /// Optional snippet or preview of the matched content
    snippet: Option<String>,

    /// Optional document ID if the result is a chunk
    document_id: Option<String>,

    /// Optional file path of the source document
    file_path: Option<String>,

    /// Optional position/index within the parent document
    position: Option<usize>,

    // Content metadata from chunks
    language: Option<String>,
    word_count: Option<usize>,
    has_code: Option<bool>,
    section: Option<String>,
    token_count: Option<i32>,
}

impl SearchResult {
    /// Create a new search result.
    ///
    /// # Arguments
    ///
    /// * `id` - Unique identifier of the matched item
    /// * `score` - Relevance score (must be finite, will be clamped to [0.0, 1.0])
    /// * `snippet` - Optional content preview
    ///
    /// # Returns
    ///
    /// Returns `Ok(SearchResult)` if score is finite (not NaN or infinity).
    ///
    /// # Errors
    ///
    /// Returns `AppError::InvalidInput` if score is NaN or infinite.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use vault_desktop::domain::entities::search_result::SearchResult;
    ///
    /// let result = SearchResult::new(
    ///     "doc-456".to_string(),
    ///     0.87,
    ///     Some("Matched content here...".to_string()),
    /// )?;
    /// # Ok::<(), vault_desktop::shared::error::AppError>(())
    /// ```
    ///
    /// ```rust,no_run
    /// // NaN is rejected
    /// let result = SearchResult::new("id".to_string(), f32::NAN, None);
    /// assert!(result.is_err());
    /// ```
    pub fn new(id: String, score: f32, snippet: Option<String>) -> Result<Self> {
        // CRITICAL: Reject NaN (violates Ord/Eq contracts)
        if score.is_nan() {
            return Err(AppError::InvalidInput(
                "Search score cannot be NaN".to_string(),
            ));
        }

        // Reject infinity (invalid score)
        if score.is_infinite() {
            return Err(AppError::InvalidInput(
                "Search score cannot be infinite".to_string(),
            ));
        }

        // Now safe to clamp - NaN and infinity rejected above
        Ok(Self {
            id,
            score: score.clamp(0.0, 1.0),
            snippet,
            document_id: None,
            file_path: None,
            position: None,
            language: None,
            word_count: None,
            has_code: None,
            section: None,
            token_count: None,
        })
    }

    /// Create a default invalid search result for error fallback cases.
    ///
    /// This is used when we need to return a valid SearchResult but the
    /// conversion/validation failed. The result has an empty ID and zero score.
    ///
    /// This bypasses validation since we know 0.0 is always valid.
    #[must_use]
    pub fn default_invalid() -> Self {
        Self {
            id: String::new(),
            score: 0.0,
            snippet: None,
            document_id: None,
            file_path: None,
            position: None,
            language: None,
            word_count: None,
            has_code: None,
            section: None,
            token_count: None,
        }
    }

    /// Create a search result with full metadata.
    ///
    /// # Arguments
    ///
    /// * `id` - Unique identifier
    /// * `score` - Relevance score (must be finite)
    /// * `snippet` - Content preview
    /// * `document_id` - Parent document ID
    /// * `file_path` - Source file path
    /// * `position` - Position within document
    ///
    /// # Errors
    ///
    /// Returns `AppError::InvalidInput` if score is NaN or infinite.
    pub fn with_metadata(
        id: String,
        score: f32,
        snippet: Option<String>,
        document_id: Option<String>,
        file_path: Option<String>,
        position: Option<usize>,
    ) -> Result<Self> {
        // Same validation as new()
        if score.is_nan() {
            return Err(AppError::InvalidInput(
                "Search score cannot be NaN".to_string(),
            ));
        }

        if score.is_infinite() {
            return Err(AppError::InvalidInput(
                "Search score cannot be infinite".to_string(),
            ));
        }

        Ok(Self {
            id,
            score: score.clamp(0.0, 1.0),
            snippet,
            document_id,
            file_path,
            position,
            language: None,
            word_count: None,
            has_code: None,
            section: None,
            token_count: None,
        })
    }

    /// Create a search result with chunk metadata.
    ///
    /// # Errors
    ///
    /// Returns `AppError::InvalidInput` if score is NaN or infinite.
    #[allow(clippy::too_many_arguments)]
    pub fn with_chunk_metadata(
        id: String,
        score: f32,
        snippet: Option<String>,
        document_id: Option<String>,
        file_path: Option<String>,
        position: Option<usize>,
        language: Option<String>,
        word_count: Option<usize>,
        has_code: Option<bool>,
        section: Option<String>,
        token_count: Option<i32>,
    ) -> Result<Self> {
        // Same validation
        if score.is_nan() {
            return Err(AppError::InvalidInput(
                "Search score cannot be NaN".to_string(),
            ));
        }

        if score.is_infinite() {
            return Err(AppError::InvalidInput(
                "Search score cannot be infinite".to_string(),
            ));
        }

        Ok(Self {
            id,
            score: score.clamp(0.0, 1.0),
            snippet,
            document_id,
            file_path,
            position,
            language,
            word_count,
            has_code,
            section,
            token_count,
        })
    }

    /// Get the result ID.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Get the relevance score.
    ///
    /// Score is guaranteed to be in range [0.0, 1.0].
    pub fn score(&self) -> f32 {
        self.score
    }

    /// Get the content snippet, if available.
    pub fn snippet(&self) -> Option<&str> {
        self.snippet.as_deref()
    }

    /// Get the document ID, if this result is from a chunk.
    pub fn document_id(&self) -> Option<&str> {
        self.document_id.as_deref()
    }

    /// Get the source file path, if available.
    pub fn file_path(&self) -> Option<&str> {
        self.file_path.as_deref()
    }

    /// Get the position within the document, if available.
    pub fn position(&self) -> Option<usize> {
        self.position
    }

    /// Check if this result meets a relevance threshold.
    ///
    /// # Arguments
    ///
    /// * `threshold` - Minimum score to be considered relevant (0.0 to 1.0)
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use vault_desktop::domain::entities::search_result::SearchResult;
    ///
    /// let result = SearchResult::new("id".to_string(), 0.75, None);
    /// assert!(result.is_relevant(0.7));
    /// assert!(!result.is_relevant(0.8));
    /// ```
    pub fn is_relevant(&self, threshold: f32) -> bool {
        self.score >= threshold.clamp(0.0, 1.0)
    }

    /// Compare relevance with another search result.
    ///
    /// Returns `true` if this result is more relevant than the other.
    pub fn is_more_relevant_than(&self, other: &SearchResult) -> bool {
        self.score > other.score
    }

    /// Get a normalized score as a percentage (0-100).
    pub fn score_percentage(&self) -> u8 {
        (self.score * 100.0).round() as u8
    }

    /// Check if the result has a content snippet.
    pub fn has_snippet(&self) -> bool {
        self.snippet.is_some()
    }

    /// Get the language of the chunk.
    pub fn language(&self) -> Option<&str> {
        self.language.as_deref()
    }

    /// Get the word count.
    pub fn word_count(&self) -> Option<usize> {
        self.word_count
    }

    /// Check if the chunk contains code.
    pub fn has_code(&self) -> Option<bool> {
        self.has_code
    }

    /// Get the section/heading.
    pub fn section(&self) -> Option<&str> {
        self.section.as_deref()
    }

    /// Get the token count.
    pub fn token_count(&self) -> Option<i32> {
        self.token_count
    }
}

// ============================================================================
// Conversions from DTOs
// ============================================================================

impl From<SearchResultPortDto> for SearchResult {
    fn from(dto: SearchResultPortDto) -> Self {
        // Convert SearchResultPortDto from ports to domain SearchResult
        // Map doc_id to id, and chunk_id to document_id (if not empty)
        let document_id = if dto.chunk_id.is_empty() {
            None
        } else {
            Some(dto.doc_id.clone())
        };

        let id = if dto.chunk_id.is_empty() {
            dto.doc_id
        } else {
            dto.chunk_id
        };

        Self {
            id,
            score: dto.score.clamp(0.0, 1.0),
            snippet: Some(dto.content),
            document_id,
            file_path: None,
            position: None,
            language: None,
            word_count: None,
            has_code: None,
            section: None,
            token_count: None,
        }
    }
}

// ============================================================================
// Ordering - Higher scores come first
// ============================================================================

impl Eq for SearchResult {}

impl PartialOrd for SearchResult {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for SearchResult {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // Reverse ordering: higher scores should come first
        other
            .score
            .partial_cmp(&self.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_search_result_creation() {
        let result = SearchResult::new("id-123".to_string(), 0.95, Some("snippet".to_string()))
            .expect("Valid score should succeed");

        assert_eq!(result.id(), "id-123");
        assert_eq!(result.score(), 0.95);
        assert_eq!(result.snippet(), Some("snippet"));
    }

    #[test]
    fn test_score_clamping() {
        let too_high =
            SearchResult::new("id".to_string(), 1.5, None).expect("Finite score should succeed");
        assert_eq!(too_high.score(), 1.0);

        let too_low =
            SearchResult::new("id".to_string(), -0.5, None).expect("Finite score should succeed");
        assert_eq!(too_low.score(), 0.0);

        let valid =
            SearchResult::new("id".to_string(), 0.75, None).expect("Finite score should succeed");
        assert_eq!(valid.score(), 0.75);
    }

    #[test]
    fn test_is_relevant() {
        let result =
            SearchResult::new("id".to_string(), 0.8, None).expect("Valid score should succeed");

        assert!(result.is_relevant(0.7));
        assert!(result.is_relevant(0.8));
        assert!(!result.is_relevant(0.9));
    }

    #[test]
    fn test_is_more_relevant_than() {
        let result1 = SearchResult::new("id1".to_string(), 0.9, None).unwrap();
        let result2 = SearchResult::new("id2".to_string(), 0.7, None).unwrap();

        assert!(result1.is_more_relevant_than(&result2));
        assert!(!result2.is_more_relevant_than(&result1));
    }

    #[test]
    fn test_score_percentage() {
        let result1 = SearchResult::new("id".to_string(), 0.856, None).unwrap();
        assert_eq!(result1.score_percentage(), 86);

        let result2 = SearchResult::new("id".to_string(), 0.5, None).unwrap();
        assert_eq!(result2.score_percentage(), 50);

        let result3 = SearchResult::new("id".to_string(), 1.0, None).unwrap();
        assert_eq!(result3.score_percentage(), 100);
    }

    #[test]
    fn test_ordering() {
        let mut results = [
            SearchResult::new("id1".to_string(), 0.5, None).unwrap(),
            SearchResult::new("id2".to_string(), 0.9, None).unwrap(),
            SearchResult::new("id3".to_string(), 0.7, None).unwrap(),
        ];

        results.sort();

        // Higher scores should come first
        assert_eq!(results[0].score(), 0.9);
        assert_eq!(results[1].score(), 0.7);
        assert_eq!(results[2].score(), 0.5);
    }

    #[test]
    fn test_with_metadata() {
        let result = SearchResult::with_metadata(
            "chunk-456".to_string(),
            0.85,
            Some("Content".to_string()),
            Some("doc-789".to_string()),
            Some("/path/to/file.txt".to_string()),
            Some(3),
        )
        .expect("Valid score should succeed");

        assert_eq!(result.id(), "chunk-456");
        assert_eq!(result.document_id(), Some("doc-789"));
        assert_eq!(result.file_path(), Some("/path/to/file.txt"));
        assert_eq!(result.position(), Some(3));
    }

    #[test]
    fn test_has_snippet() {
        let with_snippet =
            SearchResult::new("id".to_string(), 0.5, Some("content".to_string())).unwrap();
        assert!(with_snippet.has_snippet());

        let without_snippet = SearchResult::new("id".to_string(), 0.5, None).unwrap();
        assert!(!without_snippet.has_snippet());
    }

    #[test]
    fn test_serialization() {
        let result = SearchResult::with_metadata(
            "id".to_string(),
            0.75,
            Some("snippet".to_string()),
            Some("doc-id".to_string()),
            Some("/path".to_string()),
            Some(5),
        )
        .unwrap();

        let json = serde_json::to_string(&result).unwrap();
        let deserialized: SearchResult = serde_json::from_str(&json).unwrap();

        assert_eq!(result, deserialized);
    }

    // NEW TESTS: NaN and infinity rejection

    #[test]
    fn test_nan_score_rejected() {
        let result = SearchResult::new("id".to_string(), f32::NAN, None);

        assert!(result.is_err(), "NaN score must be rejected");

        match result {
            Err(AppError::InvalidInput(msg)) => {
                assert!(msg.contains("NaN"), "Error should mention NaN");
            }
            _ => panic!("Expected InvalidInput error"),
        }
    }

    #[test]
    fn test_infinity_score_rejected() {
        let pos_inf = SearchResult::new("id".to_string(), f32::INFINITY, None);
        assert!(pos_inf.is_err(), "Positive infinity must be rejected");

        let neg_inf = SearchResult::new("id".to_string(), f32::NEG_INFINITY, None);
        assert!(neg_inf.is_err(), "Negative infinity must be rejected");
    }

    #[test]
    fn test_with_metadata_validates_score() {
        let result = SearchResult::with_metadata("id".into(), f32::NAN, None, None, None, None);
        assert!(result.is_err(), "with_metadata must validate score");
    }

    #[test]
    fn test_with_chunk_metadata_validates_score() {
        let result = SearchResult::with_chunk_metadata(
            "id".into(),
            f32::NAN,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        );
        assert!(result.is_err(), "with_chunk_metadata must validate score");
    }
}

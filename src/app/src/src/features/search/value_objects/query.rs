//! # Search Query Value Object
//!
//! Validated search query string following DDD value object patterns.

use crate::shared::error::{AppError, Result};
use serde::{Deserialize, Serialize};

const MAX_QUERY_LENGTH: usize = 1000;
const MIN_QUERY_LENGTH: usize = 1;

/// Validated search query value object.
///
/// Represents a user's search query with validation and normalization.
///
/// ## Invariants
///
/// - Query is non-empty after trimming whitespace
/// - Query length is between 1 and 1000 characters
/// - Query is normalized (trimmed, no excessive whitespace)
///
/// ## Example
///
/// ```rust,no_run
/// use vault_desktop::domain::value_objects::search_query::SearchQuery;
///
/// let query = SearchQuery::new("machine learning algorithms")?;
/// assert_eq!(query.as_str(), "machine learning algorithms");
/// assert_eq!(query.word_count(), 3);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SearchQuery(String);

impl SearchQuery {
    /// Create a new validated search query.
    ///
    /// # Arguments
    ///
    /// * `query` - The search query string
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Query is empty after trimming
    /// - Query is too long (> 1000 characters)
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use vault_desktop::domain::value_objects::search_query::SearchQuery;
    ///
    /// let query = SearchQuery::new("rust programming")?;
    /// assert!(query.contains("rust"));
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn new(query: impl Into<String>) -> Result<Self> {
        let normalized = Self::normalize(query.into())?;
        Ok(Self(normalized))
    }

    /// Normalize and validate the query string.
    ///
    /// Normalization includes:
    /// - Trimming leading/trailing whitespace
    /// - Collapsing multiple spaces into single spaces
    fn normalize(query: String) -> Result<String> {
        // Trim whitespace
        let trimmed = query.trim();

        // Check if empty
        if trimmed.is_empty() {
            return Err(AppError::InvalidInput(
                "Search query cannot be empty".into(),
            ));
        }

        // Check length constraints
        if trimmed.len() < MIN_QUERY_LENGTH {
            return Err(AppError::InvalidInput(format!(
                "Search query must be at least {} character(s)",
                MIN_QUERY_LENGTH
            )));
        }

        if trimmed.len() > MAX_QUERY_LENGTH {
            return Err(AppError::InvalidInput(format!(
                "Search query too long: {} characters (max: {})",
                trimmed.len(),
                MAX_QUERY_LENGTH
            )));
        }

        // Collapse multiple spaces
        let normalized = trimmed.split_whitespace().collect::<Vec<_>>().join(" ");

        Ok(normalized)
    }

    /// Get the query as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Get the length of the query in characters.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Check if the query is empty (always false due to validation).
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Get the number of words in the query.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use vault_desktop::domain::value_objects::search_query::SearchQuery;
    ///
    /// let query = SearchQuery::new("machine learning")?;
    /// assert_eq!(query.word_count(), 2);
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn word_count(&self) -> usize {
        self.0.split_whitespace().count()
    }

    /// Check if the query contains a given substring (case-insensitive).
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use vault_desktop::domain::value_objects::search_query::SearchQuery;
    ///
    /// let query = SearchQuery::new("Rust Programming")?;
    /// assert!(query.contains_ignore_case("rust"));
    /// assert!(query.contains_ignore_case("PROGRAMMING"));
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn contains_ignore_case(&self, substring: &str) -> bool {
        self.0.to_lowercase().contains(&substring.to_lowercase())
    }

    /// Check if the query contains a given substring (case-sensitive).
    pub fn contains(&self, substring: &str) -> bool {
        self.0.contains(substring)
    }

    /// Get the lowercase version of the query.
    ///
    /// Useful for case-insensitive matching.
    pub fn to_lowercase(&self) -> String {
        self.0.to_lowercase()
    }

    /// Split the query into individual words.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use vault_desktop::domain::value_objects::search_query::SearchQuery;
    ///
    /// let query = SearchQuery::new("machine learning algorithms")?;
    /// let words = query.words();
    /// assert_eq!(words, vec!["machine", "learning", "algorithms"]);
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn words(&self) -> Vec<&str> {
        self.0.split_whitespace().collect()
    }
}

impl std::fmt::Display for SearchQuery {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl AsRef<str> for SearchQuery {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_search_query_creation() {
        let query = SearchQuery::new("test query").unwrap();
        assert_eq!(query.as_str(), "test query");
    }

    #[test]
    fn test_query_normalization() {
        // Trim whitespace
        let query = SearchQuery::new("  test query  ").unwrap();
        assert_eq!(query.as_str(), "test query");

        // Collapse multiple spaces
        let query = SearchQuery::new("test    multiple    spaces").unwrap();
        assert_eq!(query.as_str(), "test multiple spaces");
    }

    #[test]
    fn test_empty_query_rejected() {
        assert!(SearchQuery::new("").is_err());
        assert!(SearchQuery::new("   ").is_err());
        assert!(SearchQuery::new("\n\t  ").is_err());
    }

    #[test]
    fn test_query_too_long_rejected() {
        let long_query = "a".repeat(MAX_QUERY_LENGTH + 1);
        assert!(SearchQuery::new(long_query).is_err());
    }

    #[test]
    fn test_max_length_accepted() {
        let max_query = "a".repeat(MAX_QUERY_LENGTH);
        let query = SearchQuery::new(max_query.clone()).unwrap();
        assert_eq!(query.len(), MAX_QUERY_LENGTH);
    }

    #[test]
    fn test_word_count() {
        let query = SearchQuery::new("machine learning algorithms").unwrap();
        assert_eq!(query.word_count(), 3);

        let single = SearchQuery::new("hello").unwrap();
        assert_eq!(single.word_count(), 1);
    }

    #[test]
    fn test_contains() {
        let query = SearchQuery::new("Rust Programming Language").unwrap();

        assert!(query.contains("Rust"));
        assert!(query.contains("Programming"));
        assert!(!query.contains("rust")); // Case-sensitive

        assert!(query.contains_ignore_case("rust"));
        assert!(query.contains_ignore_case("PROGRAMMING"));
    }

    #[test]
    fn test_to_lowercase() {
        let query = SearchQuery::new("Rust Programming").unwrap();
        assert_eq!(query.to_lowercase(), "rust programming");
    }

    #[test]
    fn test_words() {
        let query = SearchQuery::new("machine learning algorithms").unwrap();
        let words = query.words();
        assert_eq!(words, vec!["machine", "learning", "algorithms"]);
    }

    #[test]
    fn test_display() {
        let query = SearchQuery::new("test query").unwrap();
        assert_eq!(query.to_string(), "test query");
    }

    #[test]
    fn test_as_ref() {
        let query = SearchQuery::new("test").unwrap();
        let s: &str = query.as_ref();
        assert_eq!(s, "test");
    }

    #[test]
    fn test_serialization() {
        let query = SearchQuery::new("machine learning").unwrap();

        let json = serde_json::to_string(&query).unwrap();
        let deserialized: SearchQuery = serde_json::from_str(&json).unwrap();

        assert_eq!(query, deserialized);
    }

    #[test]
    fn test_equality() {
        let query1 = SearchQuery::new("test").unwrap();
        let query2 = SearchQuery::new("test").unwrap();
        let query3 = SearchQuery::new("different").unwrap();

        assert_eq!(query1, query2);
        assert_ne!(query1, query3);
    }

    #[test]
    fn test_hash() {
        use std::collections::HashSet;

        let mut set = HashSet::new();
        let query1 = SearchQuery::new("test").unwrap();
        let query2 = SearchQuery::new("test").unwrap();

        set.insert(query1.clone());
        set.insert(query2);

        // Should only have one entry (same query)
        assert_eq!(set.len(), 1);
    }
}

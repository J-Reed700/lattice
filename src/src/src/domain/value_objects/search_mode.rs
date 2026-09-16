//! # Search Mode Value Object
//!
//! Defines the search algorithm mode (vector, BM25, or hybrid).

use serde::{Deserialize, Serialize};

/// Search mode enumeration.
///
/// Defines which search algorithm to use:
/// - **Vector**: Semantic search using embeddings and cosine similarity
/// - **BM25**: Keyword-based search using BM25 ranking
/// - **Hybrid**: Combines both vector and BM25 with configurable weights
///
/// ## Example
///
/// ```rust,no_run
/// use lattice::domain::value_objects::search_mode::SearchMode;
///
/// let mode = SearchMode::Vector;
/// assert!(mode.is_vector());
///
/// let hybrid = SearchMode::Hybrid { vector_weight: 0.7, bm25_weight: 0.3 };
/// assert!(hybrid.is_hybrid());
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub enum SearchMode {
    /// Vector-based semantic search using embeddings.
    ///
    /// Best for conceptual/semantic queries like:
    /// - "What is machine learning?"
    /// - "Documents about climate change"
    #[default]
    Vector,

    /// Keyword-based search using BM25 ranking.
    ///
    /// Best for exact keyword matches like:
    /// - "API documentation"
    /// - Specific file names or terms
    BM25,

    /// Hybrid search combining vector and BM25.
    ///
    /// Combines both approaches with configurable weights.
    /// Weights should sum to 1.0 for normalized scores.
    ///
    /// Best for general-purpose search where you want both
    /// semantic understanding and exact keyword matching.
    Hybrid {
        /// Weight for vector similarity score (0.0 to 1.0)
        vector_weight: f32,
        /// Weight for BM25 score (0.0 to 1.0)
        bm25_weight: f32,
    },
}

impl SearchMode {
    /// Create a hybrid search mode with equal weights.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use lattice::domain::value_objects::search_mode::SearchMode;
    ///
    /// let mode = SearchMode::hybrid_balanced();
    /// assert!(mode.is_hybrid());
    /// ```
    pub fn hybrid_balanced() -> Self {
        SearchMode::Hybrid {
            vector_weight: 0.5,
            bm25_weight: 0.5,
        }
    }

    /// Create a hybrid search mode with custom weights.
    ///
    /// # Arguments
    ///
    /// * `vector_weight` - Weight for vector similarity (0.0 to 1.0)
    /// * `bm25_weight` - Weight for BM25 score (0.0 to 1.0)
    ///
    /// # Note
    ///
    /// Weights are not automatically normalized. For best results, they should sum to 1.0.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use lattice::domain::value_objects::search_mode::SearchMode;
    ///
    /// // Favor vector search more heavily
    /// let mode = SearchMode::hybrid(0.8, 0.2);
    /// ```
    pub fn hybrid(vector_weight: f32, bm25_weight: f32) -> Self {
        SearchMode::Hybrid {
            vector_weight,
            bm25_weight,
        }
    }

    /// Check if this is vector mode.
    pub fn is_vector(&self) -> bool {
        matches!(self, SearchMode::Vector)
    }

    /// Check if this is BM25 mode.
    pub fn is_bm25(&self) -> bool {
        matches!(self, SearchMode::BM25)
    }

    /// Check if this is hybrid mode.
    pub fn is_hybrid(&self) -> bool {
        matches!(self, SearchMode::Hybrid { .. })
    }

    /// Get the vector weight (1.0 for Vector mode, 0.0 for BM25, custom for Hybrid).
    pub fn vector_weight(&self) -> f32 {
        match self {
            SearchMode::Vector => 1.0,
            SearchMode::BM25 => 0.0,
            SearchMode::Hybrid { vector_weight, .. } => *vector_weight,
        }
    }

    /// Get the BM25 weight (0.0 for Vector mode, 1.0 for BM25, custom for Hybrid).
    pub fn bm25_weight(&self) -> f32 {
        match self {
            SearchMode::Vector => 0.0,
            SearchMode::BM25 => 1.0,
            SearchMode::Hybrid { bm25_weight, .. } => *bm25_weight,
        }
    }

    /// Check if the weights are normalized (sum to approximately 1.0).
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use lattice::domain::value_objects::search_mode::SearchMode;
    ///
    /// let balanced = SearchMode::hybrid_balanced();
    /// assert!(balanced.is_normalized());
    ///
    /// let unnormalized = SearchMode::hybrid(0.8, 0.8);
    /// assert!(!unnormalized.is_normalized());
    /// ```
    pub fn is_normalized(&self) -> bool {
        let sum = self.vector_weight() + self.bm25_weight();
        (sum - 1.0).abs() < 0.01 // Allow small floating-point error
    }

    /// Combine two scores according to this search mode.
    ///
    /// # Arguments
    ///
    /// * `vector_score` - Vector similarity score (0.0 to 1.0)
    /// * `bm25_score` - BM25 relevance score (0.0 to 1.0)
    ///
    /// # Returns
    ///
    /// Combined score according to the mode:
    /// - Vector mode: returns vector_score
    /// - BM25 mode: returns bm25_score
    /// - Hybrid mode: weighted combination
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use lattice::domain::value_objects::search_mode::SearchMode;
    ///
    /// let mode = SearchMode::hybrid(0.7, 0.3);
    /// let combined = mode.combine_scores(0.9, 0.5);
    /// assert_eq!(combined, 0.7 * 0.9 + 0.3 * 0.5);
    /// ```
    pub fn combine_scores(&self, vector_score: f32, bm25_score: f32) -> f32 {
        match self {
            SearchMode::Vector => vector_score,
            SearchMode::BM25 => bm25_score,
            SearchMode::Hybrid {
                vector_weight,
                bm25_weight,
            } => vector_weight * vector_score + bm25_weight * bm25_score,
        }
    }
}

impl std::fmt::Display for SearchMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SearchMode::Vector => write!(f, "Vector"),
            SearchMode::BM25 => write!(f, "BM25"),
            SearchMode::Hybrid {
                vector_weight,
                bm25_weight,
            } => write!(
                f,
                "Hybrid (vector: {:.2}, bm25: {:.2})",
                vector_weight, bm25_weight
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_search_mode_vector() {
        let mode = SearchMode::Vector;
        assert!(mode.is_vector());
        assert!(!mode.is_bm25());
        assert!(!mode.is_hybrid());
        assert_eq!(mode.vector_weight(), 1.0);
        assert_eq!(mode.bm25_weight(), 0.0);
    }

    #[test]
    fn test_search_mode_bm25() {
        let mode = SearchMode::BM25;
        assert!(!mode.is_vector());
        assert!(mode.is_bm25());
        assert!(!mode.is_hybrid());
        assert_eq!(mode.vector_weight(), 0.0);
        assert_eq!(mode.bm25_weight(), 1.0);
    }

    #[test]
    fn test_search_mode_hybrid() {
        let mode = SearchMode::Hybrid {
            vector_weight: 0.7,
            bm25_weight: 0.3,
        };
        assert!(!mode.is_vector());
        assert!(!mode.is_bm25());
        assert!(mode.is_hybrid());
        assert_eq!(mode.vector_weight(), 0.7);
        assert_eq!(mode.bm25_weight(), 0.3);
    }

    #[test]
    fn test_hybrid_balanced() {
        let mode = SearchMode::hybrid_balanced();
        assert!(mode.is_hybrid());
        assert_eq!(mode.vector_weight(), 0.5);
        assert_eq!(mode.bm25_weight(), 0.5);
        assert!(mode.is_normalized());
    }

    #[test]
    fn test_hybrid_custom() {
        let mode = SearchMode::hybrid(0.8, 0.2);
        assert!(mode.is_hybrid());
        assert_eq!(mode.vector_weight(), 0.8);
        assert_eq!(mode.bm25_weight(), 0.2);
        assert!(mode.is_normalized());
    }

    #[test]
    fn test_is_normalized() {
        assert!(SearchMode::Vector.is_normalized());
        assert!(SearchMode::BM25.is_normalized());
        assert!(SearchMode::hybrid(0.6, 0.4).is_normalized());
        assert!(!SearchMode::hybrid(0.8, 0.8).is_normalized());
    }

    #[test]
    fn test_combine_scores_vector() {
        let mode = SearchMode::Vector;
        let combined = mode.combine_scores(0.9, 0.5);
        assert_eq!(combined, 0.9);
    }

    #[test]
    fn test_combine_scores_bm25() {
        let mode = SearchMode::BM25;
        let combined = mode.combine_scores(0.9, 0.5);
        assert_eq!(combined, 0.5);
    }

    #[test]
    fn test_combine_scores_hybrid() {
        let mode = SearchMode::hybrid(0.7, 0.3);
        let combined = mode.combine_scores(0.9, 0.5);
        let expected = 0.7 * 0.9 + 0.3 * 0.5;
        assert!((combined - expected).abs() < 0.001);
    }

    #[test]
    fn test_default() {
        let mode = SearchMode::default();
        assert!(mode.is_vector());
    }

    #[test]
    fn test_display() {
        assert_eq!(SearchMode::Vector.to_string(), "Vector");
        assert_eq!(SearchMode::BM25.to_string(), "BM25");

        let hybrid = SearchMode::hybrid(0.7, 0.3);
        assert_eq!(hybrid.to_string(), "Hybrid (vector: 0.70, bm25: 0.30)");
    }

    #[test]
    fn test_serialization() {
        let modes = vec![
            SearchMode::Vector,
            SearchMode::BM25,
            SearchMode::hybrid(0.6, 0.4),
        ];

        for mode in modes {
            let json = serde_json::to_string(&mode).unwrap();
            let deserialized: SearchMode = serde_json::from_str(&json).unwrap();
            assert_eq!(mode, deserialized);
        }
    }
}

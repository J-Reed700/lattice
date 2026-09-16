//! Query expansion configuration and data models.
//!
//! This module defines the configuration options and data structures for query expansion.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Configuration for query expansion behavior.
///
/// Controls how queries are expanded with synonyms and related terms.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryExpansionConfig {
    /// Enable/disable query expansion globally.
    pub enable_query_expansion: bool,

    /// Maximum number of synonym expansions per term.
    pub max_expansions_per_term: usize,

    /// Use domain-specific dictionary for expansion.
    pub use_domain_dict: bool,

    /// Whether to expand all query terms, including low-signal terms.
    ///
    /// When `false`, query expansion skips statistically low-information terms.
    pub expand_stopwords: bool,
}

impl Default for QueryExpansionConfig {
    fn default() -> Self {
        Self {
            enable_query_expansion: true,
            max_expansions_per_term: 3,
            use_domain_dict: true,
            expand_stopwords: false,
        }
    }
}

/// Result of query expansion containing original and expanded forms.
///
/// Provides both the expanded query and detailed information about
/// which terms were expanded and how.
#[derive(Debug, Clone)]
pub struct QueryExpansion {
    /// The original user query.
    pub original_query: String,

    /// The expanded query with all synonyms included.
    pub expanded_query: String,

    /// All terms in the expanded query (original + synonyms).
    pub expanded_terms: Vec<String>,

    /// Mapping of original terms to their expansions.
    pub term_expansions: HashMap<String, Vec<String>>,
}

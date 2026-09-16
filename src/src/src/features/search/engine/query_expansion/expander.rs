//! Core query expansion logic.
//!
//! Implements the QueryExpander that expands search queries with synonyms
//! from domain dictionaries and user-defined mappings.

use crate::shared::error::Result;
use std::collections::{HashMap, HashSet};

use super::config::{QueryExpansion, QueryExpansionConfig};
use super::dictionaries::{
    load_domain_dict, load_user_synonyms, save_user_synonyms, select_informative_terms,
};

/// Query expander that enriches queries with synonyms and related terms.
///
/// Uses a combination of:
/// - Domain-specific dictionary (ML, programming, search terms)
/// - User-defined synonyms (takes priority)
/// - Low-signal term filtering (optional)
pub struct QueryExpander {
    /// Domain-specific synonym dictionary.
    domain_dict: HashMap<String, Vec<String>>,

    /// User-defined synonyms (takes priority over domain dict).
    user_synonyms: HashMap<String, Vec<String>>,

    /// Configuration settings.
    config: QueryExpansionConfig,
}

impl QueryExpander {
    /// Create a new QueryExpander with the given configuration.
    ///
    /// Loads all dictionaries from their respective sources:
    /// - Domain dictionary from hardcoded mappings
    /// - User synonyms from `~/.config/lattice-desktop/synonyms.json`
    pub fn new(config: QueryExpansionConfig) -> Result<Self> {
        let domain_dict = load_domain_dict();
        let user_synonyms = load_user_synonyms()?;

        Ok(Self {
            domain_dict,
            user_synonyms,
            config,
        })
    }

    /// Expand a query with synonyms and related terms.
    ///
    /// Returns a QueryExpansion containing:
    /// - Original query
    /// - Expanded query with all synonyms
    /// - List of all terms (original + expansions)
    /// - Mapping of which terms were expanded and how
    ///
    /// # Algorithm
    /// 1. Split query into terms
    /// 2. For each unique term:
    ///    - Skip low-signal terms (unless configured otherwise)
    ///    - Check user synonyms first (takes priority)
    ///    - Fall back to domain dictionary if no user synonyms
    ///    - Limit expansions per term based on config
    /// 3. Deduplicate all terms
    /// 4. Reconstruct expanded query
    pub fn expand(&self, query: &str) -> QueryExpansion {
        let original_query = query.to_string();

        if !self.config.enable_query_expansion {
            return QueryExpansion {
                original_query: original_query.clone(),
                expanded_query: original_query.clone(),
                expanded_terms: vec![original_query.clone()],
                term_expansions: HashMap::new(),
            };
        }

        let terms: Vec<&str> = query.split_whitespace().collect();
        let informative_terms: HashSet<String> = if self.config.expand_stopwords {
            HashSet::new()
        } else {
            let tokens: Vec<String> = terms
                .iter()
                .map(|term| {
                    term.trim_matches(|c: char| !c.is_alphanumeric())
                        .to_ascii_lowercase()
                })
                .collect();
            select_informative_terms(tokens, usize::MAX)
                .into_iter()
                .collect()
        };
        let mut expanded_terms = Vec::new();
        let mut term_expansions = HashMap::new();
        let mut seen = HashSet::new();

        for term in terms {
            let term_lower = term.to_lowercase();

            // Skip duplicates
            if seen.contains(&term_lower) {
                continue;
            }
            seen.insert(term_lower.clone());

            let has_candidate_synonyms = self.user_synonyms.contains_key(&term_lower)
                || (self.config.use_domain_dict && self.domain_dict.contains_key(&term_lower));

            // Skip low-signal terms unless configured to expand all terms or
            // we have explicit synonym knowledge for the term.
            if !self.config.expand_stopwords
                && !informative_terms.contains(&term_lower)
                && !has_candidate_synonyms
            {
                expanded_terms.push(term.to_string());
                continue;
            }

            // Add original term
            expanded_terms.push(term.to_string());

            // Find synonyms (user synonyms take priority)
            let mut synonyms = Vec::new();

            if let Some(user_syns) = self.user_synonyms.get(&term_lower) {
                // User-defined synonyms take priority
                synonyms.extend(
                    user_syns
                        .iter()
                        .take(self.config.max_expansions_per_term)
                        .cloned(),
                );
            } else if self.config.use_domain_dict {
                // Fall back to domain dictionary
                if let Some(domain_syns) = self.domain_dict.get(&term_lower) {
                    synonyms.extend(
                        domain_syns
                            .iter()
                            .take(self.config.max_expansions_per_term)
                            .cloned(),
                    );
                }
            }

            // Add synonyms, avoiding duplicates
            for syn in &synonyms {
                if !seen.contains(syn) {
                    seen.insert(syn.clone());
                    expanded_terms.push(syn.clone());
                }
            }

            // Record expansions if any were added
            if !synonyms.is_empty() {
                term_expansions.insert(term.to_string(), synonyms);
            }
        }

        let expanded_query = expanded_terms.join(" ");

        QueryExpansion {
            original_query,
            expanded_query,
            expanded_terms,
            term_expansions,
        }
    }

    /// Expand a query with a limit on total number of terms.
    ///
    /// Useful for preventing query explosion with many synonyms.
    /// Prioritizes original query terms over synonym expansions.
    ///
    /// # Arguments
    /// * `query` - The query to expand
    /// * `max_total_terms` - Maximum number of terms in the result
    ///
    /// # Strategy
    /// 1. Perform full expansion
    /// 2. If under limit, return as-is
    /// 3. Otherwise:
    ///    - Keep all original query terms
    ///    - Fill remaining slots with synonym expansions
    ///    - Truncate if necessary
    pub fn expand_with_limit(&self, query: &str, max_total_terms: usize) -> QueryExpansion {
        let expansion = self.expand(query);

        if expansion.expanded_terms.len() <= max_total_terms {
            return expansion;
        }

        // Separate original terms from synonyms
        let original_terms: HashSet<String> =
            query.split_whitespace().map(|t| t.to_string()).collect();

        let mut kept_terms: Vec<String> = expansion
            .expanded_terms
            .iter()
            .filter(|t| original_terms.contains(*t))
            .cloned()
            .collect();

        let mut synonym_terms: Vec<String> = expansion
            .expanded_terms
            .iter()
            .filter(|t| !original_terms.contains(*t))
            .cloned()
            .collect();

        // Fill remaining slots with synonyms
        let remaining_slots = max_total_terms.saturating_sub(kept_terms.len());
        synonym_terms.truncate(remaining_slots);

        kept_terms.extend(synonym_terms);

        QueryExpansion {
            original_query: expansion.original_query,
            expanded_query: kept_terms.join(" "),
            expanded_terms: kept_terms.clone(),
            term_expansions: expansion.term_expansions,
        }
    }

    /// Add a user-defined synonym mapping.
    ///
    /// User synonyms take priority over domain dictionary.
    /// Term is automatically lowercased for case-insensitive matching.
    pub fn add_user_synonym(&mut self, term: String, synonyms: Vec<String>) {
        self.user_synonyms.insert(term.to_lowercase(), synonyms);
    }

    /// Remove a user-defined synonym mapping.
    pub fn remove_user_synonym(&mut self, term: &str) {
        self.user_synonyms.remove(&term.to_lowercase());
    }

    /// Get all user-defined synonyms.
    pub fn get_user_synonyms(&self) -> &HashMap<String, Vec<String>> {
        &self.user_synonyms
    }

    /// Save current user synonyms to configuration file.
    pub fn save_user_synonyms(&self) -> Result<()> {
        save_user_synonyms(&self.user_synonyms)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_expansion() {
        let config = QueryExpansionConfig::default();
        let expander = QueryExpander::new(config).unwrap();

        let expansion = expander.expand("ml algorithm");

        assert_eq!(expansion.original_query, "ml algorithm");
        assert!(expansion.expanded_terms.len() > 2);
        assert!(expansion.expanded_terms.contains(&"ml".to_string()));
        assert!(expansion.expanded_terms.contains(&"algorithm".to_string()));
    }

    #[test]
    fn test_synonym_expansion() {
        let config = QueryExpansionConfig::default();
        let expander = QueryExpander::new(config).unwrap();

        let expansion = expander.expand("ml");

        assert!(expansion.term_expansions.contains_key("ml"));
        let ml_synonyms = &expansion.term_expansions["ml"];
        assert!(ml_synonyms.contains(&"machine learning".to_string()));
    }

    #[test]
    fn test_disabled_expansion() {
        let config = QueryExpansionConfig {
            enable_query_expansion: false,
            ..Default::default()
        };
        let expander = QueryExpander::new(config).unwrap();

        let expansion = expander.expand("ml algorithm");

        assert_eq!(expansion.expanded_terms.len(), 1);
        assert_eq!(expansion.expanded_query, "ml algorithm");
    }

    #[test]
    fn test_low_signal_filtering() {
        let config = QueryExpansionConfig {
            expand_stopwords: false,
            ..Default::default()
        };
        let expander = QueryExpander::new(config).unwrap();

        let expansion = expander.expand("the algorithm");

        assert!(expansion.expanded_terms.contains(&"the".to_string()));
        assert!(!expansion.term_expansions.contains_key("the"));
        assert!(expansion.term_expansions.contains_key("algorithm"));
    }

    #[test]
    fn test_max_expansions_limit() {
        let config = QueryExpansionConfig {
            max_expansions_per_term: 2,
            ..Default::default()
        };
        let expander = QueryExpander::new(config).unwrap();

        let expansion = expander.expand("ml");

        if let Some(synonyms) = expansion.term_expansions.get("ml") {
            assert!(synonyms.len() <= 2);
        }
    }

    #[test]
    fn test_user_synonyms_priority() {
        let config = QueryExpansionConfig::default();
        let mut expander = QueryExpander::new(config).unwrap();

        expander.add_user_synonym(
            "test".to_string(),
            vec!["custom1".to_string(), "custom2".to_string()],
        );

        let expansion = expander.expand("test");

        assert!(expansion.term_expansions.contains_key("test"));
        let test_synonyms = &expansion.term_expansions["test"];
        assert!(test_synonyms.contains(&"custom1".to_string()));
        assert!(test_synonyms.contains(&"custom2".to_string()));
    }

    #[test]
    fn test_deduplication() {
        let config = QueryExpansionConfig::default();
        let expander = QueryExpander::new(config).unwrap();

        let expansion = expander.expand("ml machine learning");

        let ml_count = expansion
            .expanded_terms
            .iter()
            .filter(|t| *t == "ml")
            .count();
        let ml_full_count = expansion
            .expanded_terms
            .iter()
            .filter(|t| *t == "machine learning")
            .count();

        assert_eq!(ml_count, 1);
        assert_eq!(ml_full_count, 1);
    }

    #[test]
    fn test_expand_with_limit() {
        let config = QueryExpansionConfig::default();
        let expander = QueryExpander::new(config).unwrap();

        let expansion = expander.expand_with_limit("ml algorithm code test", 10);

        assert!(expansion.expanded_terms.len() <= 10);
        assert!(expansion.expanded_terms.contains(&"ml".to_string()));
        assert!(expansion.expanded_terms.contains(&"algorithm".to_string()));
    }

    #[test]
    fn test_add_and_remove_user_synonym() {
        let config = QueryExpansionConfig::default();
        let mut expander = QueryExpander::new(config).unwrap();

        expander.add_user_synonym("foo".to_string(), vec!["bar".to_string()]);
        assert!(expander.get_user_synonyms().contains_key("foo"));

        expander.remove_user_synonym("foo");
        assert!(!expander.get_user_synonyms().contains_key("foo"));
    }
}

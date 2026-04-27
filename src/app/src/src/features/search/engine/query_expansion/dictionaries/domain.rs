//! Domain-specific synonym dictionary.
//!
//! Provides a comprehensive mapping of technical terms to their synonyms
//! for machine learning, programming, and search-related domains.

use std::collections::HashMap;

/// Load the domain-specific synonym dictionary.
///
/// Returns a mapping of terms (lowercase) to their synonyms.
/// Includes synonyms for:
/// - Machine learning terms (ml, ai, neural, etc.)
/// - Programming concepts (code, bug, function, etc.)
/// - Search/retrieval terms (query, index, similarity, etc.)
/// - Performance terms (latency, throughput, etc.)
pub fn load_domain_dict() -> HashMap<String, Vec<String>> {
    let mut dict = HashMap::new();

    // Machine Learning & AI
    dict.insert(
        "ml".to_string(),
        vec![
            "machine learning".to_string(),
            "artificial intelligence".to_string(),
            "ai".to_string(),
        ],
    );
    dict.insert(
        "ai".to_string(),
        vec![
            "artificial intelligence".to_string(),
            "machine learning".to_string(),
            "ml".to_string(),
        ],
    );
    dict.insert(
        "neural".to_string(),
        vec![
            "network".to_string(),
            "deep learning".to_string(),
            "nn".to_string(),
        ],
    );
    dict.insert(
        "model".to_string(),
        vec![
            "network".to_string(),
            "architecture".to_string(),
            "system".to_string(),
        ],
    );
    dict.insert(
        "train".to_string(),
        vec![
            "training".to_string(),
            "learn".to_string(),
            "optimize".to_string(),
        ],
    );
    dict.insert(
        "training".to_string(),
        vec![
            "train".to_string(),
            "learning".to_string(),
            "optimization".to_string(),
        ],
    );
    dict.insert(
        "transformer".to_string(),
        vec![
            "bert".to_string(),
            "attention".to_string(),
            "encoder".to_string(),
        ],
    );
    dict.insert(
        "classification".to_string(),
        vec![
            "categorization".to_string(),
            "labeling".to_string(),
            "prediction".to_string(),
        ],
    );
    dict.insert(
        "cluster".to_string(),
        vec![
            "group".to_string(),
            "grouping".to_string(),
            "segment".to_string(),
        ],
    );

    // Programming & Code
    dict.insert(
        "algorithm".to_string(),
        vec![
            "method".to_string(),
            "technique".to_string(),
            "procedure".to_string(),
        ],
    );
    dict.insert(
        "algorithms".to_string(),
        vec![
            "methods".to_string(),
            "techniques".to_string(),
            "procedures".to_string(),
        ],
    );
    dict.insert(
        "code".to_string(),
        vec![
            "source".to_string(),
            "program".to_string(),
            "script".to_string(),
        ],
    );
    dict.insert(
        "bug".to_string(),
        vec![
            "issue".to_string(),
            "defect".to_string(),
            "error".to_string(),
        ],
    );
    dict.insert(
        "bugs".to_string(),
        vec![
            "issues".to_string(),
            "defects".to_string(),
            "errors".to_string(),
        ],
    );
    dict.insert(
        "fix".to_string(),
        vec![
            "repair".to_string(),
            "solve".to_string(),
            "resolve".to_string(),
        ],
    );
    dict.insert(
        "function".to_string(),
        vec![
            "method".to_string(),
            "procedure".to_string(),
            "routine".to_string(),
        ],
    );
    dict.insert(
        "class".to_string(),
        vec![
            "object".to_string(),
            "type".to_string(),
            "struct".to_string(),
        ],
    );
    dict.insert(
        "test".to_string(),
        vec![
            "testing".to_string(),
            "validation".to_string(),
            "verification".to_string(),
        ],
    );

    // Documents & Data
    dict.insert(
        "document".to_string(),
        vec![
            "file".to_string(),
            "paper".to_string(),
            "article".to_string(),
        ],
    );
    dict.insert(
        "documents".to_string(),
        vec![
            "files".to_string(),
            "papers".to_string(),
            "articles".to_string(),
        ],
    );
    dict.insert(
        "data".to_string(),
        vec![
            "dataset".to_string(),
            "information".to_string(),
            "records".to_string(),
        ],
    );
    dict.insert(
        "text".to_string(),
        vec![
            "content".to_string(),
            "document".to_string(),
            "passage".to_string(),
        ],
    );
    dict.insert(
        "token".to_string(),
        vec!["word".to_string(), "term".to_string(), "symbol".to_string()],
    );

    // Search & Retrieval
    dict.insert(
        "search".to_string(),
        vec![
            "find".to_string(),
            "lookup".to_string(),
            "query".to_string(),
        ],
    );
    dict.insert(
        "query".to_string(),
        vec![
            "search".to_string(),
            "request".to_string(),
            "lookup".to_string(),
        ],
    );
    dict.insert(
        "index".to_string(),
        vec![
            "indexing".to_string(),
            "catalog".to_string(),
            "registry".to_string(),
        ],
    );
    dict.insert(
        "retrieve".to_string(),
        vec!["fetch".to_string(), "get".to_string(), "find".to_string()],
    );
    dict.insert(
        "result".to_string(),
        vec![
            "output".to_string(),
            "response".to_string(),
            "answer".to_string(),
        ],
    );
    dict.insert(
        "rank".to_string(),
        vec![
            "ranking".to_string(),
            "score".to_string(),
            "order".to_string(),
        ],
    );
    dict.insert(
        "precision".to_string(),
        vec![
            "accuracy".to_string(),
            "exactness".to_string(),
            "correctness".to_string(),
        ],
    );
    // "recall" here is the information-retrieval concept (precision/recall),
    // NOT the brand. Do not rename to "lattice".
    dict.insert(
        "recall".to_string(),
        vec![
            "coverage".to_string(),
            "completeness".to_string(),
            "retrieval".to_string(),
        ],
    );

    // Vectors & Embeddings
    dict.insert(
        "vector".to_string(),
        vec![
            "embedding".to_string(),
            "representation".to_string(),
            "array".to_string(),
        ],
    );
    dict.insert(
        "embedding".to_string(),
        vec![
            "vector".to_string(),
            "representation".to_string(),
            "encoding".to_string(),
        ],
    );
    dict.insert(
        "semantic".to_string(),
        vec![
            "meaning".to_string(),
            "contextual".to_string(),
            "conceptual".to_string(),
        ],
    );
    dict.insert(
        "similarity".to_string(),
        vec![
            "distance".to_string(),
            "closeness".to_string(),
            "relevance".to_string(),
        ],
    );
    dict.insert(
        "dimension".to_string(),
        vec![
            "dimensionality".to_string(),
            "feature".to_string(),
            "size".to_string(),
        ],
    );

    // Infrastructure & Systems
    dict.insert(
        "api".to_string(),
        vec![
            "interface".to_string(),
            "endpoint".to_string(),
            "service".to_string(),
        ],
    );
    dict.insert(
        "database".to_string(),
        vec![
            "db".to_string(),
            "storage".to_string(),
            "datastore".to_string(),
        ],
    );
    dict.insert(
        "db".to_string(),
        vec![
            "database".to_string(),
            "storage".to_string(),
            "datastore".to_string(),
        ],
    );
    dict.insert(
        "memory".to_string(),
        vec![
            "ram".to_string(),
            "storage".to_string(),
            "cache".to_string(),
        ],
    );
    dict.insert(
        "cpu".to_string(),
        vec![
            "processor".to_string(),
            "compute".to_string(),
            "processing".to_string(),
        ],
    );
    dict.insert(
        "gpu".to_string(),
        vec![
            "graphics".to_string(),
            "cuda".to_string(),
            "accelerator".to_string(),
        ],
    );

    // Performance & Optimization
    dict.insert(
        "fast".to_string(),
        vec![
            "quick".to_string(),
            "rapid".to_string(),
            "speedy".to_string(),
        ],
    );
    dict.insert(
        "slow".to_string(),
        vec![
            "sluggish".to_string(),
            "delayed".to_string(),
            "lagging".to_string(),
        ],
    );
    dict.insert(
        "performance".to_string(),
        vec![
            "speed".to_string(),
            "efficiency".to_string(),
            "throughput".to_string(),
        ],
    );
    dict.insert(
        "optimize".to_string(),
        vec![
            "improve".to_string(),
            "enhance".to_string(),
            "tune".to_string(),
        ],
    );
    dict.insert(
        "latency".to_string(),
        vec![
            "delay".to_string(),
            "lag".to_string(),
            "response time".to_string(),
        ],
    );
    dict.insert(
        "throughput".to_string(),
        vec![
            "bandwidth".to_string(),
            "capacity".to_string(),
            "rate".to_string(),
        ],
    );
    dict.insert(
        "batch".to_string(),
        vec![
            "batching".to_string(),
            "bulk".to_string(),
            "group".to_string(),
        ],
    );
    dict.insert(
        "parallel".to_string(),
        vec![
            "concurrent".to_string(),
            "simultaneous".to_string(),
            "async".to_string(),
        ],
    );

    // NLP & Language
    dict.insert(
        "nlp".to_string(),
        vec![
            "natural language processing".to_string(),
            "text processing".to_string(),
            "language".to_string(),
        ],
    );
    dict.insert(
        "feature".to_string(),
        vec![
            "attribute".to_string(),
            "property".to_string(),
            "characteristic".to_string(),
        ],
    );

    dict
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_domain_dict_loaded() {
        let dict = load_domain_dict();
        assert!(!dict.is_empty());
    }

    #[test]
    fn test_ml_synonyms() {
        let dict = load_domain_dict();
        assert!(dict.contains_key("ml"));
        let synonyms = &dict["ml"];
        assert!(synonyms.contains(&"machine learning".to_string()));
        assert!(synonyms.contains(&"artificial intelligence".to_string()));
    }

    #[test]
    fn test_bidirectional_synonyms() {
        let dict = load_domain_dict();

        assert!(dict.contains_key("ml"));
        assert!(dict.contains_key("ai"));

        let ml_synonyms = &dict["ml"];
        let ai_synonyms = &dict["ai"];

        assert!(ml_synonyms.contains(&"ai".to_string()));
        assert!(ai_synonyms.contains(&"ml".to_string()));
    }

    #[test]
    fn test_all_values_lowercase() {
        let dict = load_domain_dict();

        for key in dict.keys() {
            assert_eq!(
                key,
                &key.to_lowercase(),
                "Key '{}' should be lowercase",
                key
            );
        }
    }
}

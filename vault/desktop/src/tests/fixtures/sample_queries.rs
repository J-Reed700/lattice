#![allow(clippy::panic)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::assertions_on_constants)]
#![allow(clippy::useless_vec)]
#![allow(clippy::indexing_slicing)]
#![allow(unused_variables)]
#![allow(unused_imports)]
#![allow(deprecated)]

use serde::{Deserialize, Serialize};
// Test code - allow common test patterns
#![allow(clippy::panic)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::assertions_on_constants)]
#![allow(clippy::useless_vec)]
#![allow(unused_variables)]
#![allow(unused_imports)]


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestQuery {
    pub query: String,
    pub expected_min_results: usize,
    pub expected_tags: Vec<String>,
    pub expected_mentions: Vec<String>,
}

pub fn get_test_queries() -> Vec<TestQuery> {
    vec![
        TestQuery {
            query: "machine learning algorithms".to_string(),
            expected_min_results: 1,
            expected_tags: vec!["machine-learning".to_string(), "ai".to_string()],
            expected_mentions: vec![],
        },
        TestQuery {
            query: "rust programming".to_string(),
            expected_min_results: 1,
            expected_tags: vec!["rust".to_string(), "programming".to_string()],
            expected_mentions: vec![],
        },
        TestQuery {
            query: "systems design patterns".to_string(),
            expected_min_results: 1,
            expected_tags: vec!["systems".to_string(), "design".to_string()],
            expected_mentions: vec!["distributed-systems".to_string()],
        },
        TestQuery {
            query: "neural networks deep learning".to_string(),
            expected_min_results: 1,
            expected_tags: vec!["neural-networks".to_string(), "deep-learning".to_string()],
            expected_mentions: vec!["andrew-ng".to_string()],
        },
    ]
}

pub fn get_query_rewrites() -> Vec<(String, Vec<String>)> {
    vec![
        (
            "ml basics".to_string(),
            vec![
                "machine learning fundamentals".to_string(),
                "introduction to machine learning".to_string(),
                "ml tutorial".to_string(),
            ],
        ),
        (
            "rust concurrency".to_string(),
            vec![
                "rust concurrent programming".to_string(),
                "concurrency in rust".to_string(),
                "rust threading guide".to_string(),
            ],
        ),
        (
            "system scalability".to_string(),
            vec![
                "scaling distributed systems".to_string(),
                "system scalability patterns".to_string(),
                "horizontal scaling strategies".to_string(),
            ],
        ),
    ]
}

pub fn get_obscure_queries() -> Vec<String> {
    vec![
        "quantum entanglement in neural networks".to_string(),
        "non-euclidean geometry in machine learning".to_string(),
        "Byzantine fault tolerance in distributed databases".to_string(),
        "topological data analysis for time series".to_string(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_query_generation() {
        let queries = get_test_queries();
        assert!(!queries.is_empty());
        assert!(queries.len() >= 4);

        for query in &queries {
            assert!(!query.query.is_empty());
            assert!(query.expected_min_results > 0);
        }
    }

    #[test]
    fn test_query_rewrites() {
        let rewrites = get_query_rewrites();
        assert!(!rewrites.is_empty());

        for (original, variants) in &rewrites {
            assert!(!original.is_empty());
            assert_eq!(variants.len(), 3);
        }
    }

    #[test]
    fn test_obscure_queries() {
        let queries = get_obscure_queries();
        assert!(queries.len() >= 4);

        for query in &queries {
            assert!(query.len() > 10);
        }
    }
}

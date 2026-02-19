use crate::infrastructure::search::service::SearchResult;
use std::collections::HashMap;

pub trait FusionStrategy: Send + Sync {
    fn fuse(
        &self,
        vector_results: Vec<SearchResult>,
        keyword_results: Vec<(String, f32)>,
    ) -> Vec<(String, f32)>;

    fn name(&self) -> &'static str;
}

pub struct RrfFusion {
    pub k: f32,
}

impl RrfFusion {
    pub fn new(k: f32) -> Self {
        Self { k }
    }
}

impl Default for RrfFusion {
    fn default() -> Self {
        Self { k: 60.0 }
    }
}

impl FusionStrategy for RrfFusion {
    fn fuse(
        &self,
        vector_results: Vec<SearchResult>,
        keyword_results: Vec<(String, f32)>,
    ) -> Vec<(String, f32)> {
        let mut scores: HashMap<String, f32> = HashMap::new();

        for (rank, result) in vector_results.into_iter().enumerate() {
            let id = result.id.clone();
            *scores.entry(id.clone()).or_insert(0.0) += 1.0 / (self.k + rank as f32 + 1.0);
        }

        for (rank, (id, _score)) in keyword_results.into_iter().enumerate() {
            *scores.entry(id.clone()).or_insert(0.0) += 1.0 / (self.k + rank as f32 + 1.0);
        }

        let mut results: Vec<_> = scores.into_iter().collect();
        results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        results
    }

    fn name(&self) -> &'static str {
        "RRF"
    }
}

pub struct WeightedFusion {
    pub vector_weight: f32,
    pub keyword_weight: f32,
}

impl WeightedFusion {
    pub fn new(vector_weight: f32, keyword_weight: f32) -> Self {
        Self {
            vector_weight,
            keyword_weight,
        }
    }
}

impl Default for WeightedFusion {
    fn default() -> Self {
        Self {
            vector_weight: 0.7,
            keyword_weight: 0.3,
        }
    }
}

impl FusionStrategy for WeightedFusion {
    fn fuse(
        &self,
        vector_results: Vec<SearchResult>,
        keyword_results: Vec<(String, f32)>,
    ) -> Vec<(String, f32)> {
        let mut scores: HashMap<String, f32> = HashMap::new();

        let vector_max = vector_results
            .iter()
            .map(|r| r.score)
            .fold(0.0f32, f32::max);

        let keyword_max = keyword_results
            .iter()
            .map(|(_, s)| *s)
            .fold(0.0f32, f32::max);

        for result in vector_results {
            let id = result.id.clone();
            let normalized = if vector_max > 0.0 {
                result.score / vector_max
            } else {
                0.0
            };
            *scores.entry(id).or_insert(0.0) += normalized * self.vector_weight;
        }

        for (id, score) in keyword_results {
            let normalized = if keyword_max > 0.0 {
                score / keyword_max
            } else {
                0.0
            };
            *scores.entry(id).or_insert(0.0) += normalized * self.keyword_weight;
        }

        let mut results: Vec<_> = scores.into_iter().collect();
        results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        results
    }

    fn name(&self) -> &'static str {
        "Weighted"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_vector_results() -> Vec<SearchResult> {
        vec![
            SearchResult {
                id: "doc1".to_string(),
                score: 0.95,
                index: 0,
                filename: None,
                mime_type: None,
                size_bytes: None,
                created_at: None,
                content: None,
                file_id: None,
                file_path: None,
                file_name: None,
                file_extension: None,
                file_category: None,
                is_indexed: None,
                document_id: Some("doc1".to_string()),
                snippet: None,
                chunk_index: Some(0),
                updated_at: None,
            },
            SearchResult {
                id: "doc2".to_string(),
                score: 0.85,
                index: 1,
                filename: None,
                mime_type: None,
                size_bytes: None,
                created_at: None,
                content: None,
                file_id: None,
                file_path: None,
                file_name: None,
                file_extension: None,
                file_category: None,
                is_indexed: None,
                document_id: Some("doc2".to_string()),
                snippet: None,
                chunk_index: Some(1),
                updated_at: None,
            },
        ]
    }

    #[test]
    fn test_rrf_fusion() {
        let strategy = RrfFusion::default();
        let vector_results = create_test_vector_results();
        let keyword_results = vec![("doc2".to_string(), 10.0), ("doc3".to_string(), 8.0)];

        let fused = strategy.fuse(vector_results, keyword_results);

        assert!(!fused.is_empty());
        assert_eq!(strategy.name(), "RRF");
    }

    #[test]
    fn test_weighted_fusion() {
        let strategy = WeightedFusion::default();
        let vector_results = create_test_vector_results();
        let keyword_results = vec![("doc2".to_string(), 10.0), ("doc3".to_string(), 8.0)];

        let fused = strategy.fuse(vector_results, keyword_results);

        assert!(!fused.is_empty());
        assert_eq!(strategy.name(), "Weighted");
    }

    #[test]
    fn test_weighted_fusion_custom_weights() {
        let strategy = WeightedFusion::new(0.8, 0.2);
        assert_eq!(strategy.vector_weight, 0.8);
        assert_eq!(strategy.keyword_weight, 0.2);
    }
}

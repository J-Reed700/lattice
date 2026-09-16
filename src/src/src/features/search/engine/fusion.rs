use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct FusionResult {
    pub id: String,
    pub score: f32,
    pub vector_rank: Option<usize>,
    pub bm25_rank: Option<usize>,
}

pub struct ReciprocalRankFusion {
    k: f32,
}

impl ReciprocalRankFusion {
    pub fn new(k: f32) -> Self {
        Self { k }
    }

    pub fn with_default() -> Self {
        Self::new(60.0)
    }

    pub fn fuse(
        &self,
        vector_results: Vec<(String, f32)>,
        bm25_results: Vec<(String, f32)>,
    ) -> Vec<FusionResult> {
        let mut scores: HashMap<String, (f32, Option<usize>, Option<usize>)> = HashMap::new();

        for (rank, (id, _score)) in vector_results.iter().enumerate() {
            let rrf_score = 1.0 / (self.k + (rank as f32) + 1.0);
            scores
                .entry(id.clone())
                .and_modify(|(s, v, _b)| {
                    *s += rrf_score;
                    *v = Some(rank);
                })
                .or_insert((rrf_score, Some(rank), None));
        }

        for (rank, (id, _score)) in bm25_results.iter().enumerate() {
            let rrf_score = 1.0 / (self.k + (rank as f32) + 1.0);
            scores
                .entry(id.clone())
                .and_modify(|(s, _v, b)| {
                    *s += rrf_score;
                    *b = Some(rank);
                })
                .or_insert((rrf_score, None, Some(rank)));
        }

        let mut results: Vec<FusionResult> = scores
            .into_iter()
            .map(|(id, (score, vector_rank, bm25_rank))| FusionResult {
                id,
                score,
                vector_rank,
                bm25_rank,
            })
            .collect();

        results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        results
    }

    pub fn fuse_top_k(
        &self,
        vector_results: Vec<(String, f32)>,
        bm25_results: Vec<(String, f32)>,
        top_k: usize,
    ) -> Vec<FusionResult> {
        let mut results = self.fuse(vector_results, bm25_results);
        results.truncate(top_k);
        results
    }

    pub fn fuse_three_sources(
        &self,
        source1: Vec<(String, f32)>,
        source2: Vec<(String, f32)>,
        source3: Vec<(String, f32)>,
        top_k: usize,
    ) -> Vec<FusionResult> {
        let fused_12 = self.fuse(source1, source2);

        let fused_12_tuples: Vec<(String, f32)> =
            fused_12.into_iter().map(|r| (r.id, r.score)).collect();

        self.fuse_top_k(fused_12_tuples, source3, top_k)
    }
}

pub struct WeightedFusion {
    vector_weight: f32,
    bm25_weight: f32,
}

impl WeightedFusion {
    pub fn new(vector_weight: f32, bm25_weight: f32) -> Self {
        Self {
            vector_weight,
            bm25_weight,
        }
    }

    pub fn balanced() -> Self {
        Self::new(0.5, 0.5)
    }

    pub fn fuse(
        &self,
        vector_results: Vec<(String, f32)>,
        bm25_results: Vec<(String, f32)>,
    ) -> Vec<FusionResult> {
        let mut scores: HashMap<String, (f32, Option<usize>, Option<usize>)> = HashMap::new();

        let vector_max = vector_results
            .iter()
            .map(|(_, s)| *s)
            .fold(0.0f32, f32::max);
        let bm25_max = bm25_results.iter().map(|(_, s)| *s).fold(0.0f32, f32::max);

        for (rank, (id, score)) in vector_results.iter().enumerate() {
            let normalized_score = if vector_max > 0.0 {
                score / vector_max
            } else {
                0.0
            };
            let weighted_score = normalized_score * self.vector_weight;
            scores
                .entry(id.clone())
                .and_modify(|(s, v, _b)| {
                    *s += weighted_score;
                    *v = Some(rank);
                })
                .or_insert((weighted_score, Some(rank), None));
        }

        for (rank, (id, score)) in bm25_results.iter().enumerate() {
            let normalized_score = if bm25_max > 0.0 {
                score / bm25_max
            } else {
                0.0
            };
            let weighted_score = normalized_score * self.bm25_weight;
            scores
                .entry(id.clone())
                .and_modify(|(s, _v, b)| {
                    *s += weighted_score;
                    *b = Some(rank);
                })
                .or_insert((weighted_score, None, Some(rank)));
        }

        let mut results: Vec<FusionResult> = scores
            .into_iter()
            .map(|(id, (score, vector_rank, bm25_rank))| FusionResult {
                id,
                score,
                vector_rank,
                bm25_rank,
            })
            .collect();

        results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        results
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rrf_fusion_basic() {
        let rrf = ReciprocalRankFusion::with_default();

        let vector_results = vec![
            ("doc1".to_string(), 0.95),
            ("doc2".to_string(), 0.85),
            ("doc3".to_string(), 0.75),
        ];

        let bm25_results = vec![
            ("doc3".to_string(), 10.0),
            ("doc1".to_string(), 8.0),
            ("doc4".to_string(), 6.0),
        ];

        let results = rrf.fuse(vector_results, bm25_results);

        assert!(results.len() >= 3);
        assert_eq!(results[0].id, "doc1");
        assert!(results[0].vector_rank.is_some());
        assert!(results[0].bm25_rank.is_some());
    }

    #[test]
    fn test_rrf_fusion_scoring() {
        let rrf = ReciprocalRankFusion::new(60.0);

        let vector_results = vec![("doc1".to_string(), 0.95)];
        let bm25_results = vec![("doc1".to_string(), 10.0)];

        let results = rrf.fuse(vector_results, bm25_results);

        assert_eq!(results.len(), 1);
        let expected_score = 1.0 / 61.0 + 1.0 / 61.0;
        assert!((results[0].score - expected_score).abs() < 1e-6);
    }

    #[test]
    fn test_rrf_asymmetric_results() {
        let rrf = ReciprocalRankFusion::with_default();

        let vector_results = vec![("doc1".to_string(), 0.95), ("doc2".to_string(), 0.85)];

        let bm25_results = vec![("doc3".to_string(), 10.0)];

        let results = rrf.fuse(vector_results, bm25_results);

        assert_eq!(results.len(), 3);
    }

    #[test]
    fn test_rrf_top_k() {
        let rrf = ReciprocalRankFusion::with_default();

        let vector_results = vec![
            ("doc1".to_string(), 0.95),
            ("doc2".to_string(), 0.85),
            ("doc3".to_string(), 0.75),
        ];

        let bm25_results = vec![
            ("doc3".to_string(), 10.0),
            ("doc4".to_string(), 8.0),
            ("doc5".to_string(), 6.0),
        ];

        let results = rrf.fuse_top_k(vector_results, bm25_results, 2);

        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_weighted_fusion() {
        let fusion = WeightedFusion::balanced();

        let vector_results = vec![("doc1".to_string(), 0.9), ("doc2".to_string(), 0.6)];

        let bm25_results = vec![("doc2".to_string(), 10.0), ("doc3".to_string(), 5.0)];

        let results = fusion.fuse(vector_results, bm25_results);

        assert_eq!(results.len(), 3);
        assert!(results[0].score > 0.0);
    }

    #[test]
    fn test_rrf_rank_tracking() {
        let rrf = ReciprocalRankFusion::with_default();

        let vector_results = vec![("doc1".to_string(), 0.95), ("doc2".to_string(), 0.85)];

        let bm25_results = vec![("doc2".to_string(), 10.0), ("doc3".to_string(), 8.0)];

        let results = rrf.fuse(vector_results, bm25_results);

        let doc1 = results.iter().find(|r| r.id == "doc1").unwrap();
        assert_eq!(doc1.vector_rank, Some(0));
        assert_eq!(doc1.bm25_rank, None);

        let doc2 = results.iter().find(|r| r.id == "doc2").unwrap();
        assert_eq!(doc2.vector_rank, Some(1));
        assert_eq!(doc2.bm25_rank, Some(0));

        let doc3 = results.iter().find(|r| r.id == "doc3").unwrap();
        assert_eq!(doc3.vector_rank, None);
        assert_eq!(doc3.bm25_rank, Some(1));
    }
}

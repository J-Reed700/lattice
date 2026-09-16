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
        Self::new(crate::shared::constants::DEFAULT_RRF_K)
    }

    pub fn fuse(
        &self,
        vector_results: Vec<(String, f32)>,
        bm25_results: Vec<(String, f32)>,
    ) -> Vec<FusionResult> {
        self.fuse_weighted(vector_results, bm25_results, 1.0, 1.0)
    }

    /// Reciprocal-rank fusion with a per-branch weight.
    ///
    /// Plain RRF at a large `k` is nearly flat: a passage found at rank 7 by one
    /// branch and rank 25 by the other outscores a passage one branch put
    /// first and the other never saw at all (`1/67 + 1/85 > 1/61`). On a
    /// corpus where the branches disagree — a query in one language over a
    /// passage in another, which only the vector branch can match — that
    /// buries the single confident hit under lexical near-misses. Weights let
    /// the caller trust one branch more without abandoning rank fusion; equal
    /// weights of `1.0` reproduce [`Self::fuse`] exactly.
    pub fn fuse_weighted(
        &self,
        vector_results: Vec<(String, f32)>,
        bm25_results: Vec<(String, f32)>,
        vector_weight: f32,
        bm25_weight: f32,
    ) -> Vec<FusionResult> {
        let mut scores: HashMap<String, (f32, Option<usize>, Option<usize>)> = HashMap::new();

        for (rank, (id, _score)) in vector_results.iter().enumerate() {
            let rrf_score = vector_weight / (self.k + (rank as f32) + 1.0);
            scores
                .entry(id.clone())
                .and_modify(|(s, v, _b)| {
                    *s += rrf_score;
                    *v = Some(rank);
                })
                .or_insert((rrf_score, Some(rank), None));
        }

        for (rank, (id, _score)) in bm25_results.iter().enumerate() {
            let rrf_score = bm25_weight / (self.k + (rank as f32) + 1.0);
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
    #[test]
    fn unit_weights_reproduce_plain_rrf() {
        let rrf = super::ReciprocalRankFusion::new(60.0);
        let vector = vec![("a".to_string(), 0.9), ("b".to_string(), 0.8)];
        let bm25 = vec![("b".to_string(), 3.0), ("c".to_string(), 2.0)];
        let plain = rrf.fuse(vector.clone(), bm25.clone());
        let weighted = rrf.fuse_weighted(vector, bm25, 1.0, 1.0);
        let ids = |r: &[super::FusionResult]| r.iter().map(|x| x.id.clone()).collect::<Vec<_>>();
        assert_eq!(ids(&plain), ids(&weighted));
        for (p, w) in plain.iter().zip(&weighted) {
            assert!((p.score - w.score).abs() < 1e-7);
        }
    }

    /// The failure this exists for: one branch's confident first hit versus a
    /// passage both branches merely tolerated. Weights alone are not enough
    /// at `k = 60` — the curve is too flat — which is exactly what the v2
    /// evaluation showed; the combination of a vector-leaning weight and a
    /// small `k` is what lets the sole hit through.
    #[test]
    fn vector_weight_and_small_k_let_a_sole_top_hit_beat_two_mediocre_agreements() {
        let mut vector: Vec<(String, f32)> = vec![("japanese".to_string(), 0.8)];
        vector.extend((0..5).map(|i| (format!("filler{i}"), 0.5)));
        vector.push(("global".to_string(), 0.4)); // vector rank 7
        let mut bm25: Vec<(String, f32)> = vec![("global".to_string(), 5.0)]; // bm25 rank 1
        bm25.extend((0..10).map(|i| (format!("lex{i}"), 1.0)));

        let plain = super::ReciprocalRankFusion::new(60.0).fuse(vector.clone(), bm25.clone());
        assert_eq!(plain.first().map(|r| r.id.as_str()), Some("global"));

        // 0.7/61 < 0.7/67 + 0.3/61: the agreement still wins at k = 60.
        let weighted_flat = super::ReciprocalRankFusion::new(60.0).fuse_weighted(
            vector.clone(),
            bm25.clone(),
            0.7,
            0.3,
        );
        assert_eq!(weighted_flat.first().map(|r| r.id.as_str()), Some("global"));

        // 0.7/6 > 0.7/12 + 0.3/6: a steep curve plus the weight flips it.
        let weighted_steep =
            super::ReciprocalRankFusion::new(5.0).fuse_weighted(vector, bm25, 0.7, 0.3);
        assert_eq!(
            weighted_steep.first().map(|r| r.id.as_str()),
            Some("japanese")
        );
        // The agreement is not discarded, only outranked.
        assert!(weighted_steep.iter().any(|r| r.id == "global"));
    }

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
    fn three_way_fusion_promotes_what_all_three_branches_found() {
        let rrf = ReciprocalRankFusion::new(60.0);

        let vector = vec![
            ("shared".to_string(), 0.9),
            ("vector_only".to_string(), 0.8),
        ];
        let bm25 = vec![("shared".to_string(), 7.0), ("bm25_only".to_string(), 6.0)];
        let sparse = vec![
            ("shared".to_string(), 3.0),
            ("sparse_only".to_string(), 2.0),
        ];

        let fused = rrf.fuse_three_sources(vector, bm25, sparse, 10);

        // Every branch's candidates survive; none is dropped for being unique.
        let ids: Vec<&str> = fused.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(ids.len(), 4);
        for id in ["shared", "vector_only", "bm25_only", "sparse_only"] {
            assert!(ids.contains(&id), "{id} was lost in three-way fusion");
        }
        // The chunk all three branches ranked first wins outright.
        assert_eq!(fused.first().map(|r| r.id.as_str()), Some("shared"));
        let shared = fused.first().map(|r| r.score).unwrap_or_default();
        let runner_up = fused.get(1).map(|r| r.score).unwrap_or_default();
        assert!(shared > runner_up);
    }

    #[test]
    fn three_way_fusion_keeps_a_sparse_only_hit_reachable() {
        let rrf = ReciprocalRankFusion::new(60.0);
        // Nothing overlaps: the learned sparse branch is the only source that
        // found this chunk, and it must still make the fused list.
        let fused = rrf.fuse_three_sources(
            vec![("v".to_string(), 0.5)],
            vec![("b".to_string(), 1.0)],
            vec![("s".to_string(), 1.0)],
            10,
        );
        assert_eq!(fused.len(), 3);
        assert!(fused.iter().any(|r| r.id == "s"));
    }

    #[test]
    fn three_way_fusion_with_an_empty_sparse_branch_matches_two_way() {
        let rrf = ReciprocalRankFusion::new(60.0);
        let vector = vec![("a".to_string(), 0.9), ("b".to_string(), 0.8)];
        let bm25 = vec![("b".to_string(), 7.0), ("c".to_string(), 6.0)];

        let two_way = rrf.fuse(vector.clone(), bm25.clone());
        let three_way = rrf.fuse_three_sources(vector, bm25, Vec::new(), 10);

        assert_eq!(
            two_way.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(),
            three_way.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(),
            "an unavailable sparse branch must not reorder the other two"
        );
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

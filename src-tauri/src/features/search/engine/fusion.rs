//! Weighted reciprocal-rank fusion.
//!
//! This is the one implementation. Direct search, the chat use case and the
//! chat pipeline's two-pass union all fuse ranked lists, and each of them used
//! to carry its own copy of the same six lines — which is how one of them came
//! to ignore its branch weights entirely, and how two of them came to return a
//! nondeterministic order for tied scores.

use std::collections::HashMap;

/// One branch's ranked ids and the weight its ranks carry in the fusion.
///
/// Only rank is fused. Branch scores are on incomparable scales — cosine
/// similarity, BM25, a sparse dot product, a blended reranker score — which is
/// the reason to fuse by rank in the first place, so they are not carried here.
#[derive(Debug, Clone)]
pub struct WeightedRanking {
    pub weight: f32,
    pub ids: Vec<String>,
}

impl WeightedRanking {
    pub fn new(weight: f32, ids: Vec<String>) -> Self {
        Self { weight, ids }
    }

    /// From a branch that reports `(id, score)`; the scores are dropped.
    pub fn from_scored(weight: f32, results: Vec<(String, f32)>) -> Self {
        Self {
            weight,
            ids: results.into_iter().map(|(id, _score)| id).collect(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct FusionResult {
    pub id: String,
    pub score: f32,
    /// The rank this id held in each branch, in the order the branches were
    /// passed; `None` where that branch did not return it. Kept for
    /// diagnostics — it is what tells a regression in one branch apart from a
    /// regression in the fusion.
    pub branch_ranks: Vec<Option<usize>>,
}

impl FusionResult {
    pub fn branch_rank(&self, branch: usize) -> Option<usize> {
        self.branch_ranks.get(branch).copied().flatten()
    }

    /// Branch 0 under the conventional vector-then-lexical branch order.
    pub fn vector_rank(&self) -> Option<usize> {
        self.branch_rank(0)
    }

    /// Branch 1 under the conventional vector-then-lexical branch order.
    pub fn bm25_rank(&self) -> Option<usize> {
        self.branch_rank(1)
    }
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

    /// Fuse any number of weighted ranked branches.
    ///
    /// Plain RRF at a large `k` is nearly flat: a passage found at rank 7 by one
    /// branch and rank 25 by the other outscores a passage one branch put
    /// first and the other never saw at all (`1/67 + 1/85 > 1/61`). On a
    /// corpus where the branches disagree — a query in one language over a
    /// passage in another, which only the vector branch can match — that
    /// buries the single confident hit under lexical near-misses. Weights let
    /// the caller trust one branch more without abandoning rank fusion; equal
    /// weights of `1.0` reproduce unweighted RRF exactly.
    ///
    /// Ties break on id, so the same branches always fuse to the same order.
    pub fn fuse_ranked(&self, branches: Vec<WeightedRanking>) -> Vec<FusionResult> {
        let branch_count = branches.len();
        let mut scores: HashMap<String, (f32, Vec<Option<usize>>)> = HashMap::new();

        for (branch_index, branch) in branches.into_iter().enumerate() {
            let mut seen = std::collections::HashSet::new();
            for (rank, id) in branch.ids.into_iter().enumerate() {
                if !seen.insert(id.clone()) {
                    continue;
                }
                let contribution = branch.weight / (self.k + (rank as f32) + 1.0);
                let entry = scores
                    .entry(id)
                    .or_insert_with(|| (0.0, vec![None; branch_count]));
                entry.0 += contribution;
                if let Some(slot) = entry.1.get_mut(branch_index) {
                    *slot = Some(rank);
                }
            }
        }

        let mut results: Vec<FusionResult> = scores
            .into_iter()
            .map(|(id, (score, branch_ranks))| FusionResult {
                id,
                score,
                branch_ranks,
            })
            .collect();

        results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.id.cmp(&b.id))
        });

        results
    }

    /// [`Self::fuse_ranked`], truncated.
    pub fn fuse_ranked_top_k(
        &self,
        branches: Vec<WeightedRanking>,
        top_k: usize,
    ) -> Vec<FusionResult> {
        let mut results = self.fuse_ranked(branches);
        results.truncate(top_k);
        results
    }

    /// Unweighted two-branch fusion, in vector-then-lexical branch order.
    pub fn fuse(
        &self,
        vector_results: Vec<(String, f32)>,
        bm25_results: Vec<(String, f32)>,
    ) -> Vec<FusionResult> {
        self.fuse_weighted(vector_results, bm25_results, 1.0, 1.0)
    }

    /// Weighted two-branch fusion, in vector-then-lexical branch order.
    pub fn fuse_weighted(
        &self,
        vector_results: Vec<(String, f32)>,
        bm25_results: Vec<(String, f32)>,
        vector_weight: f32,
        bm25_weight: f32,
    ) -> Vec<FusionResult> {
        self.fuse_ranked(vec![
            WeightedRanking::from_scored(vector_weight, vector_results),
            WeightedRanking::from_scored(bm25_weight, bm25_results),
        ])
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

    /// Vector, BM25 and learned-sparse in one fusion, each with its own weight.
    ///
    /// The weights are not optional. An earlier version folded the first two
    /// branches into an intermediate list and fused that against the third,
    /// which threw the configured weights away *and* halved every vector and
    /// BM25 rank contribution relative to sparse — so turning the third branch
    /// on silently gave it about half the vote.
    pub fn fuse_three_sources(
        &self,
        vector_results: Vec<(String, f32)>,
        bm25_results: Vec<(String, f32)>,
        sparse_results: Vec<(String, f32)>,
        weights: ThreeBranchWeights,
        top_k: usize,
    ) -> Vec<FusionResult> {
        self.fuse_ranked_top_k(
            vec![
                WeightedRanking::from_scored(weights.vector, vector_results),
                WeightedRanking::from_scored(weights.bm25, bm25_results),
                WeightedRanking::from_scored(weights.sparse, sparse_results),
            ],
            top_k,
        )
    }
}

/// How the three-branch fusion splits its vote.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ThreeBranchWeights {
    pub vector: f32,
    pub bm25: f32,
    pub sparse: f32,
}

impl ThreeBranchWeights {
    /// Give the learned sparse branch half of the lexical vote rather than a
    /// vote of its own.
    ///
    /// Sparse and BM25 are both lexical signals over the same text — an
    /// expanded-term match and a literal-term match. Handing each of them the
    /// full keyword weight would move the vector/lexical balance from the
    /// tuned `0.7 / 0.3` to `0.7 / 0.6` merely by enabling a branch, which is
    /// not what enabling a branch is supposed to mean. Splitting keeps the
    /// balance and lets the two lexical branches agree or disagree inside it.
    pub fn splitting_keyword_weight(vector_weight: f32, keyword_weight: f32) -> Self {
        Self {
            vector: vector_weight,
            bm25: keyword_weight / 2.0,
            sparse: keyword_weight / 2.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unit_weights_reproduce_plain_rrf() {
        let rrf = ReciprocalRankFusion::new(60.0);
        let vector = vec![("a".to_string(), 0.9), ("b".to_string(), 0.8)];
        let bm25 = vec![("b".to_string(), 3.0), ("c".to_string(), 2.0)];
        let plain = rrf.fuse(vector.clone(), bm25.clone());
        let weighted = rrf.fuse_weighted(vector, bm25, 1.0, 1.0);
        let ids = |r: &[FusionResult]| r.iter().map(|x| x.id.clone()).collect::<Vec<_>>();
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

        let plain = ReciprocalRankFusion::new(60.0).fuse(vector.clone(), bm25.clone());
        assert_eq!(plain.first().map(|r| r.id.as_str()), Some("global"));

        // 0.7/61 < 0.7/67 + 0.3/61: the agreement still wins at k = 60.
        let weighted_flat =
            ReciprocalRankFusion::new(60.0).fuse_weighted(vector.clone(), bm25.clone(), 0.7, 0.3);
        assert_eq!(weighted_flat.first().map(|r| r.id.as_str()), Some("global"));

        // 0.7/6 > 0.7/12 + 0.3/6: a steep curve plus the weight flips it.
        let weighted_steep = ReciprocalRankFusion::new(5.0).fuse_weighted(vector, bm25, 0.7, 0.3);
        assert_eq!(
            weighted_steep.first().map(|r| r.id.as_str()),
            Some("japanese")
        );
        // The agreement is not discarded, only outranked.
        assert!(weighted_steep.iter().any(|r| r.id == "global"));
    }

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
        assert!(results[0].vector_rank().is_some());
        assert!(results[0].bm25_rank().is_some());
    }

    #[test]
    fn test_rrf_fusion_scoring() {
        let rrf = ReciprocalRankFusion::new(60.0);

        let results = rrf.fuse(
            vec![("doc1".to_string(), 0.95)],
            vec![("doc1".to_string(), 10.0)],
        );

        assert_eq!(results.len(), 1);
        let expected_score = 1.0 / 61.0 + 1.0 / 61.0;
        assert!((results[0].score - expected_score).abs() < 1e-6);
    }

    #[test]
    fn test_rrf_asymmetric_results() {
        let rrf = ReciprocalRankFusion::with_default();
        let results = rrf.fuse(
            vec![("doc1".to_string(), 0.95), ("doc2".to_string(), 0.85)],
            vec![("doc3".to_string(), 10.0)],
        );
        assert_eq!(results.len(), 3);
    }

    #[test]
    fn test_rrf_top_k() {
        let rrf = ReciprocalRankFusion::with_default();
        let results = rrf.fuse_top_k(
            vec![
                ("doc1".to_string(), 0.95),
                ("doc2".to_string(), 0.85),
                ("doc3".to_string(), 0.75),
            ],
            vec![
                ("doc3".to_string(), 10.0),
                ("doc4".to_string(), 8.0),
                ("doc5".to_string(), 6.0),
            ],
            2,
        );
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_rrf_rank_tracking() {
        let rrf = ReciprocalRankFusion::with_default();

        let results = rrf.fuse(
            vec![("doc1".to_string(), 0.95), ("doc2".to_string(), 0.85)],
            vec![("doc2".to_string(), 10.0), ("doc3".to_string(), 8.0)],
        );

        let doc1 = results.iter().find(|r| r.id == "doc1").unwrap();
        assert_eq!(doc1.vector_rank(), Some(0));
        assert_eq!(doc1.bm25_rank(), None);

        let doc2 = results.iter().find(|r| r.id == "doc2").unwrap();
        assert_eq!(doc2.vector_rank(), Some(1));
        assert_eq!(doc2.bm25_rank(), Some(0));

        let doc3 = results.iter().find(|r| r.id == "doc3").unwrap();
        assert_eq!(doc3.vector_rank(), None);
        assert_eq!(doc3.bm25_rank(), Some(1));
    }

    /// Per-branch ranks stay truthful however many branches there are, which
    /// the old pairwise folding could not manage.
    #[test]
    fn every_branch_reports_its_own_rank() {
        let fused = ReciprocalRankFusion::new(10.0).fuse_ranked(vec![
            WeightedRanking::new(0.7, vec!["a".into(), "b".into()]),
            WeightedRanking::new(0.15, vec!["b".into(), "c".into()]),
            WeightedRanking::new(0.15, vec!["c".into(), "a".into()]),
        ]);
        let of = |id: &str| {
            fused
                .iter()
                .find(|r| r.id == id)
                .map(|r| r.branch_ranks.clone())
                .unwrap()
        };
        assert_eq!(of("a"), vec![Some(0), None, Some(1)]);
        assert_eq!(of("b"), vec![Some(1), Some(0), None]);
        assert_eq!(of("c"), vec![None, Some(1), Some(0)]);
    }

    #[test]
    fn a_duplicate_inside_one_branch_is_counted_once() {
        let fused = ReciprocalRankFusion::new(10.0).fuse_ranked(vec![WeightedRanking::new(
            1.0,
            vec!["a".into(), "a".into()],
        )]);
        assert_eq!(fused.len(), 1);
        assert!((fused[0].score - 1.0 / 11.0).abs() < 1e-7);
        assert_eq!(fused[0].branch_rank(0), Some(0));
    }

    #[test]
    fn tied_scores_order_deterministically() {
        let rrf = ReciprocalRankFusion::new(10.0);
        let branches = || {
            vec![WeightedRanking::new(
                1.0,
                vec!["zulu".into(), "alpha".into()],
            )]
        };
        let first: Vec<String> = rrf
            .fuse_ranked(branches())
            .into_iter()
            .map(|r| r.id)
            .collect();
        // Equal-weight, same-rank entries: the id is the only stable ordering.
        let tied = rrf.fuse_ranked(vec![
            WeightedRanking::new(1.0, vec!["zulu".into()]),
            WeightedRanking::new(1.0, vec!["alpha".into()]),
        ]);
        assert_eq!(first, vec!["zulu".to_string(), "alpha".to_string()]);
        assert_eq!(
            tied.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(),
            vec!["alpha", "zulu"]
        );
    }

    #[test]
    fn three_way_fusion_promotes_what_all_three_branches_found() {
        let rrf = ReciprocalRankFusion::new(60.0);
        let weights = ThreeBranchWeights::splitting_keyword_weight(0.7, 0.3);

        let fused = rrf.fuse_three_sources(
            vec![
                ("shared".to_string(), 0.9),
                ("vector_only".to_string(), 0.8),
            ],
            vec![("shared".to_string(), 7.0), ("bm25_only".to_string(), 6.0)],
            vec![
                ("shared".to_string(), 3.0),
                ("sparse_only".to_string(), 2.0),
            ],
            weights,
            10,
        );

        // Every branch's candidates survive; none is dropped for being unique.
        let ids: Vec<&str> = fused.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(ids.len(), 4);
        for id in ["shared", "vector_only", "bm25_only", "sparse_only"] {
            assert!(ids.contains(&id), "{id} was lost in three-way fusion");
        }
        assert_eq!(fused.first().map(|r| r.id.as_str()), Some("shared"));
    }

    #[test]
    fn three_way_fusion_keeps_a_sparse_only_hit_reachable() {
        let fused = ReciprocalRankFusion::new(60.0).fuse_three_sources(
            vec![("v".to_string(), 0.5)],
            vec![("b".to_string(), 1.0)],
            vec![("s".to_string(), 1.0)],
            ThreeBranchWeights::splitting_keyword_weight(0.7, 0.3),
            10,
        );
        assert_eq!(fused.len(), 3);
        assert!(fused.iter().any(|r| r.id == "s"));
    }

    /// Turning the third branch on must not move the vector/lexical balance.
    /// With the branch empty, the split lexical weight has to fuse to exactly
    /// the two-way ranking it would have produced on its own.
    #[test]
    fn an_empty_sparse_branch_does_not_reorder_the_other_two() {
        let rrf = ReciprocalRankFusion::new(10.0);
        let vector = vec![
            ("a".to_string(), 0.9),
            ("b".to_string(), 0.8),
            ("c".to_string(), 0.7),
        ];
        let bm25 = vec![("b".to_string(), 7.0), ("d".to_string(), 6.0)];

        let two_way = rrf.fuse_weighted(vector.clone(), bm25.clone(), 0.7, 0.3);
        let three_way = rrf.fuse_three_sources(
            vector,
            bm25,
            Vec::new(),
            ThreeBranchWeights::splitting_keyword_weight(0.7, 0.3),
            10,
        );

        assert_eq!(
            two_way.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(),
            three_way.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(),
        );
    }

    /// The bug the weights argument exists for: the sparse branch used to get
    /// half the total vote because the other two were folded together first.
    #[test]
    fn the_sparse_branch_cannot_outvote_a_confident_vector_hit() {
        let rrf = ReciprocalRankFusion::new(10.0);
        let weights = ThreeBranchWeights::splitting_keyword_weight(0.7, 0.3);
        let fused = rrf.fuse_three_sources(
            vec![("dense_top".to_string(), 0.9)],
            vec![("lexical_top".to_string(), 5.0)],
            vec![("lexical_top".to_string(), 5.0)],
            weights,
            10,
        );
        // 0.7/11 = 0.0636 against 0.15/11 + 0.15/11 = 0.0273.
        assert_eq!(fused.first().map(|r| r.id.as_str()), Some("dense_top"));
    }

    #[test]
    fn splitting_the_keyword_weight_preserves_the_lexical_total() {
        let weights = ThreeBranchWeights::splitting_keyword_weight(0.7, 0.3);
        assert_eq!(weights.vector, 0.7);
        assert!((weights.bm25 + weights.sparse - 0.3).abs() < 1e-7);
    }
}

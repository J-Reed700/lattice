//! Weighted reciprocal-rank fusion.
//!
//! This is the one implementation. The library search orchestrator
//! (`HybridSearchUseCase`) fuses through it, for single queries, multi-query
//! plans and chat's two-pass union alike. Separate copies of the same six
//! lines are how one path once came to ignore its branch weights entirely,
//! and how two came to return a nondeterministic order for tied scores.

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
}

pub struct ReciprocalRankFusion {
    k: f32,
}

impl ReciprocalRankFusion {
    pub fn new(k: f32) -> Self {
        Self { k }
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
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ranking(weight: f32, ids: &[&str]) -> WeightedRanking {
        WeightedRanking::new(weight, ids.iter().map(|id| (*id).to_owned()).collect())
    }

    fn first(fused: &[FusionResult]) -> Option<&str> {
        fused.first().map(|r| r.id.as_str())
    }

    /// The failure this exists for: one branch's confident first hit versus a
    /// passage both branches merely tolerated. Weights alone are not enough
    /// at `k = 60` — the curve is too flat — which is exactly what the v2
    /// evaluation showed; the combination of a vector-leaning weight and a
    /// small `k` is what lets the sole hit through.
    #[test]
    fn vector_weight_and_small_k_let_a_sole_top_hit_beat_two_mediocre_agreements() {
        let mut vector = vec!["japanese".to_owned()];
        vector.extend((0..5).map(|i| format!("filler{i}")));
        vector.push("global".to_owned()); // vector rank 7
        let mut bm25 = vec!["global".to_owned()]; // bm25 rank 1
        bm25.extend((0..10).map(|i| format!("lex{i}")));
        let branches = |vector_weight, bm25_weight| {
            vec![
                WeightedRanking::new(vector_weight, vector.clone()),
                WeightedRanking::new(bm25_weight, bm25.clone()),
            ]
        };

        let plain = ReciprocalRankFusion::new(60.0).fuse_ranked(branches(1.0, 1.0));
        assert_eq!(first(&plain), Some("global"));

        // 0.7/61 < 0.7/67 + 0.3/61: the agreement still wins at k = 60.
        let weighted_flat = ReciprocalRankFusion::new(60.0).fuse_ranked(branches(0.7, 0.3));
        assert_eq!(first(&weighted_flat), Some("global"));

        // 0.7/6 > 0.7/12 + 0.3/6: a steep curve plus the weight flips it.
        let weighted_steep = ReciprocalRankFusion::new(5.0).fuse_ranked(branches(0.7, 0.3));
        assert_eq!(first(&weighted_steep), Some("japanese"));
        // The agreement is not discarded, only outranked.
        assert!(weighted_steep.iter().any(|r| r.id == "global"));
    }

    #[test]
    fn a_passage_both_branches_found_scores_both_contributions() {
        let fused = ReciprocalRankFusion::new(60.0)
            .fuse_ranked(vec![ranking(1.0, &["doc1"]), ranking(1.0, &["doc1"])]);

        assert_eq!(fused.len(), 1);
        let expected_score = 1.0 / 61.0 + 1.0 / 61.0;
        assert!((fused[0].score - expected_score).abs() < 1e-6);
    }

    #[test]
    fn a_passage_only_one_branch_found_is_kept() {
        let fused = ReciprocalRankFusion::new(10.0).fuse_ranked(vec![
            ranking(1.0, &["doc1", "doc2"]),
            ranking(1.0, &["doc3"]),
        ]);
        assert_eq!(fused.len(), 3);
    }

    /// Per-branch ranks stay truthful however many branches there are, which
    /// the old pairwise folding could not manage.
    #[test]
    fn every_branch_reports_its_own_rank() {
        let fused = ReciprocalRankFusion::new(10.0).fuse_ranked(vec![
            ranking(0.7, &["a", "b"]),
            ranking(0.15, &["b", "c"]),
            ranking(0.15, &["c", "a"]),
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
        let fused = ReciprocalRankFusion::new(10.0).fuse_ranked(vec![ranking(1.0, &["a", "a"])]);
        assert_eq!(fused.len(), 1);
        assert!((fused[0].score - 1.0 / 11.0).abs() < 1e-7);
        assert_eq!(fused[0].branch_rank(0), Some(0));
    }

    #[test]
    fn tied_scores_order_deterministically() {
        let rrf = ReciprocalRankFusion::new(10.0);
        let ordered: Vec<String> = rrf
            .fuse_ranked(vec![ranking(1.0, &["zulu", "alpha"])])
            .into_iter()
            .map(|r| r.id)
            .collect();
        // Equal-weight, same-rank entries: the id is the only stable ordering.
        let tied = rrf.fuse_ranked(vec![ranking(1.0, &["zulu"]), ranking(1.0, &["alpha"])]);
        assert_eq!(ordered, vec!["zulu".to_string(), "alpha".to_string()]);
        assert_eq!(
            tied.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(),
            vec!["alpha", "zulu"]
        );
    }
}

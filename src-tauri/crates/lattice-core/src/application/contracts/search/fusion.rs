//! Weighted reciprocal-rank fusion.
//!
//! This is the one implementation. The library search orchestrator
//! (`HybridSearchUseCase`) fuses through it, for single queries, multi-query
//! plans and chat's two-pass union alike, and so do the folder index and the
//! conversation-memory recall. Separate copies of the same six lines are how
//! one path once came to ignore its branch weights entirely, how two came to
//! return a nondeterministic order for tied scores, and how two kept the flat
//! `k = 60` curve after the library search had moved to `DEFAULT_RRF_K`.
//!
//! The lesson reference collection's merge is deliberately not fusion: its
//! library and keyword lists come from disjoint sources, so there is no
//! agreement to reward, and it interleaves them with equal shares.
//!
//! Ids are generic: callers fuse `String` chunk ids, `i64` row ids or anything
//! else that is `Eq + Hash + Clone + Ord` (the order is the tie-break) without
//! converting them.

use std::collections::{HashMap, HashSet};
use std::hash::Hash;

/// One branch's ranked ids and the weight its ranks carry in the fusion.
///
/// Only rank is fused. Branch scores are on incomparable scales — cosine
/// similarity, BM25, a sparse dot product, a blended reranker score — which is
/// the reason to fuse by rank in the first place, so they are not carried here.
#[derive(Debug, Clone)]
pub struct WeightedRanking<Id> {
    pub weight: f32,
    pub ids: Vec<Id>,
}

impl<Id> WeightedRanking<Id> {
    pub fn new(weight: f32, ids: Vec<Id>) -> Self {
        Self { weight, ids }
    }
}

#[derive(Debug, Clone)]
pub struct FusionResult<Id> {
    pub id: Id,
    pub score: f32,
    /// The rank this id held in each branch, in the order the branches were
    /// passed; `None` where that branch did not return it. Kept for
    /// diagnostics — it is what tells a regression in one branch apart from a
    /// regression in the fusion.
    pub branch_ranks: Vec<Option<usize>>,
}

impl<Id> FusionResult<Id> {
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

    /// What one ranked entry adds to its fused score: `weight / (k + rank + 1)`
    /// for a zero-based `rank`. [`Self::fuse_ranked`] sums these per id.
    fn contribution(&self, weight: f32, rank: usize) -> f32 {
        weight / (self.k + (rank as f32) + 1.0)
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
    /// An id repeated inside one branch counts once, at its best rank.
    /// Ties break on id, so the same branches always fuse to the same order.
    pub fn fuse_ranked<Id>(&self, branches: Vec<WeightedRanking<Id>>) -> Vec<FusionResult<Id>>
    where
        Id: Eq + Hash + Clone + Ord,
    {
        let branch_count = branches.len();
        let mut scores: HashMap<Id, (f32, Vec<Option<usize>>)> = HashMap::new();

        for (branch_index, branch) in branches.into_iter().enumerate() {
            let mut seen = HashSet::new();
            for (rank, id) in branch.ids.into_iter().enumerate() {
                if !seen.insert(id.clone()) {
                    continue;
                }
                let contribution = self.contribution(branch.weight, rank);
                let entry = scores
                    .entry(id)
                    .or_insert_with(|| (0.0, vec![None; branch_count]));
                entry.0 += contribution;
                if let Some(slot) = entry.1.get_mut(branch_index) {
                    *slot = Some(rank);
                }
            }
        }

        let mut results: Vec<FusionResult<Id>> = scores
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
mod tests;

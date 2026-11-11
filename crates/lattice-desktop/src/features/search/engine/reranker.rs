//! # Reranking contract
//!
//! [`Reranker`] scores documents against a query; [`fuse_rerank_order`] folds
//! those scores into the first-stage order. The app's implementation is
//! `LlamaReranker`.

use lattice_core::shared::error::{AppError, Result};
use std::collections::HashSet;

/// Per-query reranking result.
#[derive(Debug, Clone)]
pub struct RerankResult {
    /// Index into the original `documents` vec.
    pub index: usize,
    /// Normalized cross-encoder score. Higher = more relevant.
    pub score: f32,
}

/// Shared reranking contract used by every retrieval entry point.
#[async_trait::async_trait]
pub trait Reranker: Send + Sync + std::fmt::Debug {
    /// Whether the backing model artifacts are currently available.
    fn is_available(&self) -> bool;

    /// Score and order the supplied documents for a query.
    async fn rerank(
        &self,
        query: &str,
        documents: Vec<String>,
        top_k: usize,
    ) -> Result<Vec<RerankResult>>;
}

/// Reciprocal-rank fusion's offset: a candidate's share from one order is
/// `1 / (RERANK_FUSION_K + rank)`, ranks counted from 1.
pub const RERANK_FUSION_K: f64 = 60.0;

/// One judged candidate in its fused place.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FusedCandidate {
    /// Index into the candidates as the first stage ordered them.
    pub index: usize,
    /// The cross-encoder's relevance for it, clamped to `[0, 1]`.
    pub relevance: f32,
}

/// Order `candidate_count` candidates, given in first-stage order, by
/// reciprocal-rank fusion of that order and the reranker's. Ties keep the
/// first-stage order.
///
/// The reranker must return exactly one finite score for each candidate. Keeping
/// this policy here prevents chat and direct search from drifting apart.
pub fn fuse_rerank_order(
    candidate_count: usize,
    reranked: &[RerankResult],
) -> Result<Vec<FusedCandidate>> {
    if reranked.len() != candidate_count {
        return Err(AppError::InvalidState(format!(
            "Reranker returned {} scores for {} candidates",
            reranked.len(),
            candidate_count
        )));
    }

    let mut seen = HashSet::with_capacity(reranked.len());
    let mut relevance = vec![0.0_f32; candidate_count];
    for entry in reranked {
        if !entry.score.is_finite() {
            return Err(AppError::InvalidState(
                "Reranker returned a non-finite score".to_string(),
            ));
        }
        let Some(score) = relevance.get_mut(entry.index) else {
            return Err(AppError::InvalidState(format!(
                "Reranker returned out-of-range candidate index {}",
                entry.index
            )));
        };
        if !seen.insert(entry.index) {
            return Err(AppError::InvalidState(format!(
                "Reranker returned duplicate candidate index {}",
                entry.index
            )));
        }
        *score = entry.score;
    }

    let mut by_relevance: Vec<(usize, f32)> = relevance.into_iter().enumerate().collect();
    by_relevance.sort_by(|a, b| b.1.total_cmp(&a.1));
    let share = |rank: usize| 1.0 / (RERANK_FUSION_K + rank as f64);
    let mut fused: Vec<(usize, f64, f32)> = by_relevance
        .into_iter()
        .enumerate()
        .map(|(rerank_rank, (index, relevance))| {
            (index, share(index + 1) + share(rerank_rank + 1), relevance)
        })
        .collect();
    fused.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
    Ok(fused
        .into_iter()
        .map(|(index, _, relevance)| FusedCandidate {
            index,
            relevance: relevance.clamp(0.0, 1.0),
        })
        .collect())
}

#[cfg(test)]
mod tests;

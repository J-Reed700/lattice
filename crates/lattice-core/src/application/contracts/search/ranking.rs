//! The library search's intermediate results: each branch's ranked list
//! before fusion, and a fused list after the cross-encoder stage.

use super::dto::{SearchResponseDto, SearchResultDto};

/// Where a ranked list came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BranchKind {
    /// Dense vector similarity.
    Vector,
    /// BM25 over the chunk text.
    Lexical,
    /// An earlier fused list, unioned with another: chat's corrective retry
    /// fuses its pass with the first. Its results keep the branch
    /// diagnostics they already carry.
    Fused,
}

/// One ranked list and the weight its ranks carry in the fusion.
#[derive(Debug, Clone)]
pub struct RankedBranch {
    pub kind: BranchKind,
    pub weight: f32,
    pub results: Vec<SearchResultDto>,
}

/// Every branch that answered one query, not yet fused.
#[derive(Debug, Default)]
pub struct QueryBranches {
    pub branches: Vec<RankedBranch>,
    /// How many of the vector and BM25 branches answered, so a caller can
    /// tell "nothing matched" (an empty answer) from "search is down" (none).
    pub answered: usize,
}

/// How much wider than the results wanted a reranked search's fused pool is.
const POOL_MULTIPLIER: usize = 3;

/// The most fused candidates a reranked search ever retrieves.
const POOL_MAX: usize = 64;

/// What the cross-encoder judges a fused list against, and how much of it.
#[derive(Debug, Clone)]
pub struct RerankOptions {
    /// The text every passage is scored against.
    pub query: String,
    /// How many of the list's leading results the cross-encoder scores. The
    /// rest keep their fused scores.
    pub max_candidates: usize,
    /// The query is cut to this many characters before scoring.
    pub query_max_chars: usize,
}

impl RerankOptions {
    /// The fused pool to retrieve when a search will be reranked and then cut
    /// to `limit`: wider, so the cross-encoder can promote a passage fusion
    /// left just outside the cut. The widening is bounded, because the
    /// cross-encoder costs a forward pass a candidate; the pool is never
    /// narrower than `limit` itself, which is what the caller can use.
    pub fn candidate_pool(limit: usize) -> usize {
        limit
            .saturating_mul(POOL_MULTIPLIER)
            .min(POOL_MAX)
            .max(limit)
    }
}

/// A list after the rerank stage, and whether cross-encoder scores were
/// actually applied to it.
///
/// Callers need the flag, not just the order: when it is set, each judged
/// result's score is the cross-encoder's relevance, from 0 to 1, and can be
/// thresholded, but the fused weights left behind when the reranker is
/// unavailable or fails are ranks, and thresholding ranks would invent
/// confidence the search does not have.
#[derive(Debug)]
pub struct Reranked {
    pub response: SearchResponseDto,
    pub applied: bool,
}

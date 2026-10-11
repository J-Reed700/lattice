//! The cross-encoder stage: re-order the head of a fused list.

use tracing::{info, warn};

use super::HybridSearchUseCase;
use crate::features::search::dto::SearchResponseDto;
use crate::features::search::engine::reranker::{blend_rerank_scores, RerankResult};
use crate::shared::error::Result;
use crate::shared::text::safe_truncate;

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
    /// left just outside the cut, and bounded, because every candidate costs
    /// the cross-encoder a forward pass.
    pub fn candidate_pool(limit: usize) -> usize {
        limit.saturating_mul(POOL_MULTIPLIER).min(POOL_MAX)
    }
}

/// A list after the rerank stage, and whether cross-encoder scores were
/// actually applied to it.
///
/// Callers need the flag, not just the order: a blended score can be
/// thresholded, but the fused weights left behind when the reranker is
/// unavailable or fails are ranks, and thresholding ranks would invent
/// confidence the search does not have.
#[derive(Debug)]
pub struct Reranked {
    pub response: SearchResponseDto,
    pub applied: bool,
}

impl HybridSearchUseCase {
    /// Re-order the head of a fused list with the cross-encoder.
    ///
    /// The leading [`RerankOptions::max_candidates`] results are scored
    /// against the query as `"{title} {content}"`, their scores blended with
    /// the fused ones ([`blend_rerank_scores`]), and the whole list sorted by
    /// score. With no reranker wired or none available, one result or fewer,
    /// or a failed, empty or malformed cross-encoder answer, the list comes
    /// back in its fused order and unflagged.
    pub async fn rerank(
        &self,
        mut response: SearchResponseDto,
        options: &RerankOptions,
    ) -> Reranked {
        if response.results.len() <= 1 {
            return Reranked {
                response,
                applied: false,
            };
        }
        let Some(reranker) = self
            .reranker
            .as_ref()
            .filter(|reranker| reranker.is_available())
        else {
            return Reranked {
                response,
                applied: false,
            };
        };

        let query = safe_truncate(&options.query, options.query_max_chars);
        let candidate_count = response.results.len().min(options.max_candidates);
        let documents: Vec<String> = response
            .results
            .iter()
            .take(candidate_count)
            .map(|result| format!("{} {}", result.title, result.content))
            .collect();

        match reranker.rerank(&query, documents, candidate_count).await {
            Ok(reranked) if !reranked.is_empty() => {
                match apply_blend(&mut response, &reranked, candidate_count) {
                    Ok(()) => {
                        info!(
                            candidate_count,
                            reranked_count = reranked.len(),
                            "Applied cross-encoder reranking to fused search results"
                        );
                        return Reranked {
                            response,
                            applied: true,
                        };
                    }
                    Err(error) => {
                        warn!(%error, "Invalid cross-encoder output; keeping fused order");
                    }
                }
            }
            Ok(_) => {}
            Err(error) => {
                warn!(%error, "Cross-encoder reranking failed; keeping fused order");
            }
        }

        Reranked {
            response,
            applied: false,
        }
    }
}

/// Blend the cross-encoder's scores into the leading `candidate_count`
/// results and sort the whole list. Nothing is written unless the blend is
/// valid for every candidate.
fn apply_blend(
    response: &mut SearchResponseDto,
    reranked: &[RerankResult],
    candidate_count: usize,
) -> Result<()> {
    if candidate_count == 0 || response.results.is_empty() {
        return Ok(());
    }

    let fused_scores: Vec<f32> = response
        .results
        .iter()
        .take(candidate_count)
        .map(|result| result.score)
        .collect();
    let scores = blend_rerank_scores(&fused_scores, reranked)?;

    for (result, score) in response
        .results
        .iter_mut()
        .take(candidate_count)
        .zip(scores)
    {
        result.score = score;
    }

    response.results.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    response.total = response.results.len();
    Ok(())
}

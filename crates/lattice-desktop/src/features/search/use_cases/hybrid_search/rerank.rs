//! The cross-encoder stage: re-order the head of a fused list.

use tracing::{info, warn};

use super::HybridSearchUseCase;
use crate::features::search::engine::reranker::{fuse_rerank_order, RerankResult};
use lattice_core::application::contracts::search::{
    RerankOptions, Reranked, SearchResponseDto, SearchResultDto,
};
use lattice_core::shared::error::Result;
use lattice_core::shared::text::safe_truncate;

impl HybridSearchUseCase {
    /// Re-order the head of a fused list with the cross-encoder.
    ///
    /// The leading [`RerankOptions::max_candidates`] results are scored
    /// against the query as `"{title} {content}"` and re-ordered by
    /// reciprocal-rank fusion of their fused and cross-encoder ranks
    /// ([`fuse_rerank_order`]); each then carries its cross-encoder relevance,
    /// 0 to 1, as its score. With no reranker wired or none available, one
    /// result or fewer, or a failed, empty or malformed cross-encoder answer,
    /// the list comes back in its fused order and unflagged.
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
                match apply_fusion(&mut response, &reranked, candidate_count) {
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

/// Re-order the leading `candidate_count` results by the fusion of their
/// fused rank and the cross-encoder's ([`fuse_rerank_order`]), each scored
/// with its cross-encoder relevance; the results past them follow in their
/// fused order and keep their fused scores. Nothing is written unless the
/// fusion is valid for every candidate.
fn apply_fusion(
    response: &mut SearchResponseDto,
    reranked: &[RerankResult],
    candidate_count: usize,
) -> Result<()> {
    let judged = candidate_count.min(response.results.len());
    if judged == 0 {
        return Ok(());
    }
    let order = fuse_rerank_order(judged, reranked)?;
    let mut head: Vec<Option<SearchResultDto>> =
        response.results.drain(..judged).map(Some).collect();
    let fused: Vec<SearchResultDto> = order
        .into_iter()
        .filter_map(|candidate| {
            let mut result = head.get_mut(candidate.index)?.take()?;
            result.score = candidate.relevance;
            Some(result)
        })
        .collect();
    response.results.splice(..0, fused);
    response.total = response.results.len();
    Ok(())
}

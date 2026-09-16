use std::collections::HashSet;
use tracing::{info, warn};

use crate::features::search::dto::SearchResponseDto;
use crate::features::search::engine::reranker::{blend_rerank_scores, RerankResult};
use crate::features::settings::dto::RetrievalTuningSettingsDto;
use crate::interfaces::di::Container;
use crate::shared::text_utils::safe_truncate;

/// Rerank the shortlist, reporting whether cross-encoder scores were actually
/// applied.
///
/// The caller needs that second value, not just the reordered list: a blended
/// score can be thresholded, but the RRF weights left behind when the reranker
/// is unavailable or fails are ranks, and thresholding ranks would invent
/// confidence the pipeline does not have.
pub(super) async fn apply_rerank_stage(
    container: &Container,
    validated_message: &str,
    interpretation: &crate::domain::qa::hyde::HyDEInterpretation,
    mut search_response: SearchResponseDto,
    followup_anchor_terms: Option<&HashSet<String>>,
    tuning: &RetrievalTuningSettingsDto,
) -> (SearchResponseDto, bool) {
    if search_response.results.len() <= 1 {
        return (search_response, false);
    }

    let rerank_query = build_rerank_query(
        validated_message,
        interpretation,
        followup_anchor_terms,
        tuning,
    );

    let reranker = container.reranker();
    if reranker.is_available() {
        let rerank_count = search_response
            .results
            .len()
            .min(tuning.rerank_max_candidates as usize);
        let documents: Vec<String> = search_response
            .results
            .iter()
            .take(rerank_count)
            .map(|result| format!("{} {}", result.title, result.content))
            .collect();

        match reranker
            .rerank(&rerank_query, documents, rerank_count)
            .await
        {
            Ok(reranked) if !reranked.is_empty() => {
                match apply_cross_encoder_rerank(&mut search_response, &reranked, rerank_count) {
                    Ok(()) => {
                        info!(
                            candidate_count = rerank_count,
                            reranked_count = reranked.len(),
                            "Applied shared cross-encoder reranking to KB results"
                        );
                        return (search_response, true);
                    }
                    Err(error) => {
                        warn!(error = %error, "Invalid cross-encoder output; keeping fused retrieval order");
                    }
                }
            }
            Ok(_) => {}
            Err(error) => {
                warn!(error = %error, "Cross-encoder reranking failed; keeping fused retrieval order");
            }
        }
    }

    (search_response, false)
}

fn build_rerank_query(
    validated_message: &str,
    interpretation: &crate::domain::qa::hyde::HyDEInterpretation,
    followup_anchor_terms: Option<&HashSet<String>>,
    tuning: &RetrievalTuningSettingsDto,
) -> String {
    let mut query = validated_message.trim().to_string();

    if let Some(hyde_text) = interpretation.hyde_text.as_deref() {
        if !hyde_text.trim().is_empty() {
            query = format!("{}\n{}", query, hyde_text.trim());
        }
    }

    if let Some(anchors) = followup_anchor_terms {
        if !anchors.is_empty() {
            let mut ordered_anchors: Vec<String> = anchors.iter().cloned().collect();
            ordered_anchors.sort();
            let anchor_text = ordered_anchors
                .into_iter()
                .take(10)
                .collect::<Vec<_>>()
                .join(" ");
            if !anchor_text.is_empty() {
                query = format!("{}\n{}", query, anchor_text);
            }
        }
    }

    safe_truncate(&query, tuning.rerank_query_max_chars as usize)
}

fn apply_cross_encoder_rerank(
    search_response: &mut SearchResponseDto,
    reranked: &[RerankResult],
    rerank_count: usize,
) -> crate::shared::error::Result<()> {
    if rerank_count == 0 || search_response.results.is_empty() {
        return Ok(());
    }

    let original_scores: Vec<f32> = search_response
        .results
        .iter()
        .take(rerank_count)
        .map(|result| result.score)
        .collect();
    let scores = blend_rerank_scores(&original_scores, reranked)?;

    for (result, score) in search_response
        .results
        .iter_mut()
        .take(rerank_count)
        .zip(scores)
    {
        result.score = score;
    }

    search_response.results.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    search_response.total = search_response.results.len();
    Ok(())
}

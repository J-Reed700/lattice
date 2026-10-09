use std::collections::HashSet;

use crate::features::conversation::chat::ports::ChatRetrieval;
use crate::features::search::dto::SearchResponseDto;
use crate::features::search::use_cases::RerankOptions;
use crate::features::settings::dto::RetrievalTuningSettingsDto;

/// Rerank the shortlist through the search orchestrator's cross-encoder
/// stage, reporting whether cross-encoder scores were actually applied.
///
/// The caller needs that second value, not just the reordered list: a blended
/// score can be thresholded, but the RRF weights left behind when the reranker
/// is unavailable or fails are ranks, and thresholding ranks would invent
/// confidence the pipeline does not have.
pub(super) async fn apply_rerank_stage(
    container: &dyn ChatRetrieval,
    validated_message: &str,
    interpretation: &crate::domain::qa::hyde::HyDEInterpretation,
    search_response: SearchResponseDto,
    followup_anchor_terms: Option<&HashSet<String>>,
    tuning: &RetrievalTuningSettingsDto,
) -> (SearchResponseDto, bool) {
    let options = RerankOptions {
        query: build_rerank_query(validated_message, interpretation, followup_anchor_terms),
        max_candidates: tuning.rerank_max_candidates as usize,
        query_max_chars: tuning.rerank_query_max_chars as usize,
    };
    let reranked = container
        .hybrid_search_use_case()
        .rerank(search_response, &options)
        .await;
    (reranked.response, reranked.applied)
}

/// What the cross-encoder judges passages against: the message, the HyDE
/// passage when there is one, and up to ten follow-up anchor terms.
fn build_rerank_query(
    validated_message: &str,
    interpretation: &crate::domain::qa::hyde::HyDEInterpretation,
    followup_anchor_terms: Option<&HashSet<String>>,
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

    query
}

use once_cell::sync::Lazy;
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tracing::{debug, info, warn};

use crate::application::dtos::search_dto::SearchResponseDto;
use crate::features::settings::dto::RetrievalTuningSettingsDto;
use crate::infrastructure::search::reranker::{RerankResult, RerankerService};
use crate::interfaces::di::Container;
use crate::shared::text_utils::safe_truncate;

static RERANKER_INSTANCE: Lazy<tokio::sync::Mutex<Option<Arc<RerankerService>>>> =
    Lazy::new(|| tokio::sync::Mutex::new(None));
static RERANKER_INIT_FAILED: AtomicBool = AtomicBool::new(false);

pub(super) async fn apply_rerank_stage(
    container: &Container,
    validated_message: &str,
    interpretation: &crate::domain::qa::hyde::HyDEInterpretation,
    search_response: SearchResponseDto,
    followup_anchor_terms: Option<&HashSet<String>>,
    tuning: &RetrievalTuningSettingsDto,
) -> SearchResponseDto {
    if search_response.results.len() <= 1 {
        return search_response;
    }

    let rerank_query = build_rerank_query(
        validated_message,
        interpretation,
        followup_anchor_terms,
        tuning,
    );

    if let Some(reranker) = get_or_init_reranker(container).await {
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
                info!(
                    candidate_count = rerank_count,
                    reranked_count = reranked.len(),
                    "Applied cross-encoder reranking to KB results"
                );
                return apply_cross_encoder_rerank(search_response, &reranked, rerank_count);
            }
            Ok(_) => {}
            Err(error) => {
                warn!(error = %error, "Cross-encoder reranking failed; falling back to lexical rerank");
            }
        }
    }

    apply_overlap_rerank(search_response, &rerank_query)
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

async fn get_or_init_reranker(container: &Container) -> Option<Arc<RerankerService>> {
    {
        let guard = RERANKER_INSTANCE.lock().await;
        if let Some(service) = guard.as_ref() {
            return Some(Arc::clone(service));
        }
    }

    if RERANKER_INIT_FAILED.load(Ordering::Relaxed) {
        return None;
    }

    let model_path = container.models_path().join("reranker").join("model.onnx");
    if !model_path.exists() {
        return None;
    }

    match RerankerService::new(&model_path).await {
        Ok(service) => {
            let service = Arc::new(service);
            let mut guard = RERANKER_INSTANCE.lock().await;
            *guard = Some(Arc::clone(&service));
            Some(service)
        }
        Err(error) => {
            RERANKER_INIT_FAILED.store(true, Ordering::Relaxed);
            warn!(
                error = %error,
                model_path = %model_path.display(),
                "Failed to initialize reranker model"
            );
            None
        }
    }
}

fn apply_cross_encoder_rerank(
    mut search_response: SearchResponseDto,
    reranked: &[RerankResult],
    rerank_count: usize,
) -> SearchResponseDto {
    if rerank_count == 0 || search_response.results.is_empty() {
        return search_response;
    }

    let max_original = search_response
        .results
        .iter()
        .take(rerank_count)
        .map(|result| result.score)
        .fold(0.0_f32, f32::max)
        .max(f32::EPSILON);

    let mut rerank_scores = vec![0.0_f32; rerank_count];
    for entry in reranked {
        if entry.index < rerank_count {
            rerank_scores[entry.index] = entry.score.clamp(0.0, 1.0);
        }
    }

    for (idx, result) in search_response
        .results
        .iter_mut()
        .take(rerank_count)
        .enumerate()
    {
        let original = (result.score / max_original).clamp(0.0, 1.0);
        let rerank_score = rerank_scores[idx];
        result.score = (0.35 * original + 0.65 * rerank_score).clamp(0.0, 1.0);
    }

    search_response.results.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    search_response.total = search_response.results.len();
    search_response
}

fn apply_overlap_rerank(
    mut search_response: SearchResponseDto,
    rerank_query: &str,
) -> SearchResponseDto {
    let query_terms = super::extract_overlap_query_terms(rerank_query);
    if query_terms.is_empty() || search_response.results.len() <= 1 {
        return search_response;
    }

    let query_term_set: HashSet<String> = query_terms.into_iter().collect();
    let max_original = search_response
        .results
        .iter()
        .map(|result| result.score)
        .fold(0.0_f32, f32::max)
        .max(f32::EPSILON);

    for result in &mut search_response.results {
        let mut haystack = String::new();
        haystack.push_str(&result.title);
        haystack.push(' ');
        haystack.push_str(&result.content);
        if let Some(path) = &result.path {
            haystack.push(' ');
            haystack.push_str(path);
        }
        let haystack_terms = super::tokenize_overlap_terms(&haystack);
        let hits = query_term_set
            .iter()
            .filter(|term| haystack_terms.contains(term.as_str()))
            .count() as f32;
        let overlap_score = if query_term_set.is_empty() {
            0.0
        } else {
            (hits / query_term_set.len() as f32).clamp(0.0, 1.0)
        };
        let original_score = (result.score / max_original).clamp(0.0, 1.0);
        result.score = (0.6 * original_score + 0.4 * overlap_score).clamp(0.0, 1.0);
    }

    search_response.results.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    search_response.total = search_response.results.len();
    debug!(
        result_count = search_response.results.len(),
        "Applied lexical fallback rerank to KB results"
    );
    search_response
}

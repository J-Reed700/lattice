use crate::features::search::dto::{SearchResponseDto, SearchResultDto};
use crate::features::settings::dto::RetrievalTuningSettingsDto;
use std::collections::{HashMap, HashSet};
use tracing::{debug, warn};

pub(super) fn build_document_shortlist(
    results: &[SearchResultDto],
    max_docs: usize,
) -> HashSet<String> {
    if results.is_empty() || max_docs == 0 {
        return HashSet::new();
    }

    #[derive(Default)]
    struct DocStats {
        weighted_score: f32,
        hits: usize,
    }

    let mut support_by_doc: HashMap<String, DocStats> = HashMap::new();
    for (rank, result) in results.iter().enumerate() {
        let doc_key = result
            .document_id
            .as_ref()
            .cloned()
            .unwrap_or_else(|| result.id.clone());
        let rank_boost = 1.0 / ((rank + 1) as f32);
        let entry = support_by_doc.entry(doc_key).or_default();
        entry.weighted_score += result.score.max(0.0) + rank_boost;
        entry.hits += 1;
    }

    let mut ranked_docs: Vec<(String, f32, usize)> = support_by_doc
        .into_iter()
        .map(|(doc_id, stats)| (doc_id, stats.weighted_score, stats.hits))
        .collect();
    ranked_docs.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| b.2.cmp(&a.2))
            .then_with(|| a.0.cmp(&b.0))
    });

    ranked_docs
        .into_iter()
        .take(max_docs)
        .map(|(doc_id, _, _)| doc_id)
        .collect()
}

pub(super) fn filter_results_by_document_shortlist(
    response: SearchResponseDto,
    document_shortlist: &HashSet<String>,
) -> SearchResponseDto {
    if document_shortlist.is_empty() {
        return response;
    }

    let original_total = response.results.len();
    let original_time = response.query_time_ms;
    let original_results = response.results;
    let filtered_results: Vec<SearchResultDto> = original_results
        .iter()
        .filter(|result| {
            let doc_key = result.document_id.as_deref().unwrap_or(result.id.as_str());
            document_shortlist.contains(doc_key)
        })
        .cloned()
        .collect();

    if filtered_results.is_empty() {
        debug!(
            original_total = original_total,
            shortlist_docs = document_shortlist.len(),
            "Document shortlist removed all results; preserving unfiltered candidates"
        );
        SearchResponseDto {
            total: original_results.len(),
            results: original_results,
            query_time_ms: original_time,
        }
    } else {
        // Fail open when a tiny shortlist collapses a broad result set.
        if document_shortlist.len() <= 2 && original_total >= 8 && filtered_results.len() <= 2 {
            warn!(
                original_total = original_total,
                filtered_total = filtered_results.len(),
                shortlist_docs = document_shortlist.len(),
                "Shortlist gate over-pruned broad results; preserving unfiltered candidates"
            );
            return SearchResponseDto {
                total: original_results.len(),
                results: original_results,
                query_time_ms: original_time,
            };
        }
        debug!(
            original_total = original_total,
            filtered_total = filtered_results.len(),
            shortlist_docs = document_shortlist.len(),
            "Applied document shortlist gate to retrieval results"
        );
        SearchResponseDto {
            total: filtered_results.len(),
            results: filtered_results,
            query_time_ms: original_time,
        }
    }
}

pub(super) fn should_apply_document_shortlist(
    shortlist_response: &SearchResponseDto,
    document_shortlist: &HashSet<String>,
) -> bool {
    should_apply_document_shortlist_with_tuning(
        shortlist_response,
        document_shortlist,
        &RetrievalTuningSettingsDto::default(),
    )
}

pub(super) fn should_apply_document_shortlist_with_tuning(
    shortlist_response: &SearchResponseDto,
    document_shortlist: &HashSet<String>,
    tuning: &RetrievalTuningSettingsDto,
) -> bool {
    shortlist_response.results.len() >= tuning.shortlist_gate_min_candidates as usize
        && document_shortlist.len() >= tuning.shortlist_gate_min_docs as usize
}

pub(super) fn filter_results_by_document_support(
    results: Vec<SearchResultDto>,
) -> Vec<SearchResultDto> {
    filter_results_by_document_support_with_tuning(results, &RetrievalTuningSettingsDto::default())
}

pub(super) fn filter_results_by_document_support_with_tuning(
    results: Vec<SearchResultDto>,
    tuning: &RetrievalTuningSettingsDto,
) -> Vec<SearchResultDto> {
    const STRONG_SINGLE_HIT_CHUNK_RATIO: f32 = 0.80;

    if results.len() <= 1 {
        return results;
    }

    #[derive(Debug, Default, Clone, Copy)]
    struct DocSupport {
        count: usize,
        total_score: f32,
        max_chunk_score: f32,
    }

    let mut support_by_doc: HashMap<String, DocSupport> = HashMap::new();

    for result in &results {
        let doc_key = result
            .document_id
            .as_ref()
            .cloned()
            .unwrap_or_else(|| result.id.clone());
        let entry = support_by_doc.entry(doc_key).or_default();
        entry.count += 1;
        let score = result.score.max(0.0);
        entry.total_score += score;
        entry.max_chunk_score = entry.max_chunk_score.max(score);
    }

    let max_support = support_by_doc
        .values()
        .map(|s| s.total_score)
        .fold(0.0_f32, f32::max);
    if max_support <= f32::EPSILON {
        return results;
    }
    let max_chunk_score = support_by_doc
        .values()
        .map(|s| s.max_chunk_score)
        .fold(0.0_f32, f32::max)
        .max(f32::EPSILON);

    let mut multi_hit_ratios: Vec<f32> = support_by_doc
        .values()
        .filter(|stats| stats.count >= 2)
        .map(|stats| stats.total_score / max_support)
        .collect();
    multi_hit_ratios.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let median_multi_ratio = if multi_hit_ratios.is_empty() {
        0.0
    } else {
        multi_hit_ratios[multi_hit_ratios.len() / 2]
    };
    let min_multi_hit_support_ratio =
        (median_multi_ratio * tuning.doc_support_multi_hit_ratio_factor).clamp(
            tuning.doc_support_multi_hit_ratio_min,
            tuning.doc_support_multi_hit_ratio_max,
        );
    let min_single_hit_support_ratio =
        (median_multi_ratio * tuning.doc_support_single_hit_ratio_factor).clamp(
            tuning.doc_support_single_hit_ratio_min,
            tuning.doc_support_single_hit_ratio_max,
        );

    debug!(
        max_support = max_support,
        min_multi_hit_support_ratio = min_multi_hit_support_ratio,
        min_single_hit_support_ratio = min_single_hit_support_ratio,
        median_multi_ratio = median_multi_ratio,
        support_map = ?support_by_doc,
        "Document support filter configuration"
    );

    let mut filtered = Vec::with_capacity(results.len());
    for result in &results {
        let doc_key = result
            .document_id
            .as_ref()
            .cloned()
            .unwrap_or_else(|| result.id.clone());

        let keep = if let Some(stats) = support_by_doc.get(&doc_key) {
            let support_ratio = stats.total_score / max_support;
            let strong_single_hit_ratio = stats.max_chunk_score / max_chunk_score;
            let keep_doc = if stats.count >= 2 {
                support_ratio >= min_multi_hit_support_ratio
            } else {
                support_ratio >= min_single_hit_support_ratio
                    || (strong_single_hit_ratio >= STRONG_SINGLE_HIT_CHUNK_RATIO
                        && support_ratio >= min_multi_hit_support_ratio)
            };
            debug!(
                doc_id = %doc_key,
                result_id = %result.id,
                doc_chunk_count = stats.count,
                doc_total_score = stats.total_score,
                doc_support_ratio = support_ratio,
                strong_single_hit_ratio = strong_single_hit_ratio,
                keep = keep_doc,
                "Document support filter decision"
            );
            keep_doc
        } else {
            true
        };

        if keep {
            filtered.push(result.clone());
        }
    }

    if filtered.is_empty() {
        results
    } else {
        filtered
    }
}

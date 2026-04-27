use std::collections::HashSet;
use std::sync::Arc;
use std::time::Instant;

use tracing::{debug, info, warn};

use crate::features::search::dto::{
    SearchModeDto, SearchRequestDto, SearchResponseDto, SearchResultDto,
};
use crate::features::settings::dto::{RetrievalTuningSettingsDto, ToolOutputSettingsDto};
use crate::interfaces::di::Container;
use crate::shared::text_utils::safe_truncate;

pub(super) async fn execute_kb_search_plan(
    container: &Container,
    validated_message: &str,
    interpretation: &crate::domain::qa::hyde::HyDEInterpretation,
    kb_plan: &super::KbSearchPlan,
    forced_kb: bool,
    kb_search_limit: usize,
    scope: &super::SpaceDocumentScope,
    tuning: &RetrievalTuningSettingsDto,
) -> (SearchResponseDto, super::KbSearchPlanTimingMetrics) {
    let search_plan_start = Instant::now();
    let mut plan_timings = super::KbSearchPlanTimingMetrics::default();
    let search_uc = container.semantic_search_use_case();
    let hybrid_search_uc = container.hybrid_search_use_case();

    let search_request = SearchRequestDto {
        query: kb_plan.query.clone(),
        limit: Some(kb_search_limit),
        threshold: kb_plan.threshold,
        mode: kb_plan.mode.clone(),
    };

    let execute_search = |request: SearchRequestDto| {
        let search_uc = Arc::clone(&search_uc);
        let hybrid_search_uc = Arc::clone(&hybrid_search_uc);
        let space_id = scope.space_id.clone();
        let scoped_document_ids = scope.document_ids.clone();
        async move {
            match request.mode {
                SearchModeDto::Hybrid { .. } | SearchModeDto::BM25 => {
                    hybrid_search_uc
                        .execute_scoped(
                            request,
                            Some(space_id.as_str()),
                            Some(&scoped_document_ids),
                        )
                        .await
                }
                _ => {
                    search_uc
                        .execute_scoped(request, Some(&scoped_document_ids))
                        .await
                }
            }
        }
    };

    let shortlist_planning_start = Instant::now();
    let shortlist_candidate_limit = derive_doc_shortlist_candidate_limit(kb_search_limit, tuning);
    let shortlist_doc_limit = derive_doc_shortlist_doc_limit(kb_search_limit, tuning);
    let mut keyword_plan =
        super::build_keyword_query_plan(validated_message, interpretation.hyde_text.as_deref());
    let shortlist_request = SearchRequestDto {
        query: keyword_plan.query.clone(),
        limit: Some(shortlist_candidate_limit),
        threshold: None,
        mode: SearchModeDto::BM25,
    };
    let shortlist_response = execute_search(shortlist_request).await.unwrap_or_else(|e| {
        warn!(
            error = %e,
            "Document shortlist BM25 search failed; continuing without shortlist"
        );
        super::empty_search_response()
    });
    let shortlist_response = super::apply_hard_space_scope_filter(
        shortlist_response,
        &scope.space_id,
        &scope.document_ids,
    );
    let document_shortlist =
        super::build_document_shortlist(&shortlist_response.results, shortlist_doc_limit);
    if !shortlist_response.results.is_empty() {
        keyword_plan = super::expand_keyword_plan_with_rm3(
            keyword_plan,
            &shortlist_response.results,
            validated_message,
            interpretation.hyde_text.as_deref(),
        );
    }
    plan_timings.shortlist_planning_ms = super::elapsed_ms(shortlist_planning_start);
    info!(
        shortlist_candidates = shortlist_response.results.len(),
        shortlisted_docs = document_shortlist.len(),
        lexical_terms = keyword_plan.terms.len(),
        lexical_query = %keyword_plan.query,
        "Lexical planning complete (BM25 shortlist + RM3 expansion)"
    );
    let apply_shortlist_gate = super::should_apply_document_shortlist_with_tuning(
        &shortlist_response,
        &document_shortlist,
        tuning,
    );
    if !apply_shortlist_gate {
        warn!(
            shortlist_candidates = shortlist_response.results.len(),
            shortlisted_docs = document_shortlist.len(),
            "Shortlist confidence is low; skipping shortlist gate to avoid over-pruning relevant docs"
        );
    }

    if kb_plan.run_parallel_keyword_branch {
        debug!(
            terms = ?keyword_plan.terms,
            source_query = %validated_message,
            "Keyword query plan details"
        );

        let raw_request = SearchRequestDto {
            query: keyword_plan.query.clone(),
            limit: Some(kb_search_limit),
            threshold: None,
            mode: SearchModeDto::BM25,
        };
        let raw_mode = raw_request.mode.clone();

        let query_execution_start = Instant::now();
        let hyde_future = execute_search(search_request.clone());
        let raw_future = execute_search(raw_request);
        let (hyde_result, raw_result) = tokio::join!(hyde_future, raw_future);
        plan_timings.query_execution_ms = super::elapsed_ms(query_execution_start);

        let hyde_response = hyde_result.unwrap_or_else(|e| {
            warn!(error = %e, "RAG search (HyDE) failed, proceeding without context");
            super::empty_search_response()
        });
        let raw_response = raw_result.unwrap_or_else(|e| {
            warn!(error = %e, "RAG search (raw) failed, proceeding without context");
            super::empty_search_response()
        });
        let hyde_response = super::apply_hard_space_scope_filter(
            hyde_response,
            &scope.space_id,
            &scope.document_ids,
        );
        let raw_response = super::apply_hard_space_scope_filter(
            raw_response,
            &scope.space_id,
            &scope.document_ids,
        );

        if hyde_response.results.is_empty() && !raw_response.results.is_empty() {
            warn!(
                raw_results = raw_response.results.len(),
                lexical_query = %keyword_plan.query,
                hyde_query = %kb_plan.query,
                "HyDE branch returned zero results while lexical branch returned matches"
            );
        }

        info!(
            hyde_results = hyde_response.results.len(),
            raw_results = raw_response.results.len(),
            hyde_mode = ?kb_plan.mode,
            raw_mode = ?raw_mode,
            forced_kb = forced_kb,
            limit = kb_search_limit,
            "RAG search completed (parallel)"
        );
        debug!(
            hyde_preview = summarize_search_results(&hyde_response.results, 5),
            raw_preview = summarize_search_results(&raw_response.results, 5),
            "RAG branch results"
        );

        let merge_gate_start = Instant::now();
        let merged = merge_search_results(hyde_response, raw_response);
        let merged = if apply_shortlist_gate {
            super::filter_results_by_document_shortlist(merged, &document_shortlist)
        } else {
            merged
        };
        plan_timings.merge_shortlist_gate_ms = super::elapsed_ms(merge_gate_start);
        debug!(
            merged_preview = summarize_search_results(&merged.results, 8),
            "Merged RAG results before overlap filter"
        );
        plan_timings.total_ms = super::elapsed_ms(search_plan_start);
        (merged, plan_timings)
    } else {
        let query_execution_start = Instant::now();
        let response = execute_search(search_request).await.unwrap_or_else(|e| {
            warn!(error = %e, "RAG search failed, proceeding without context");
            super::empty_search_response()
        });
        plan_timings.query_execution_ms = super::elapsed_ms(query_execution_start);
        let response =
            super::apply_hard_space_scope_filter(response, &scope.space_id, &scope.document_ids);
        let merge_gate_start = Instant::now();
        let response = if apply_shortlist_gate {
            super::filter_results_by_document_shortlist(response, &document_shortlist)
        } else {
            response
        };
        plan_timings.merge_shortlist_gate_ms = super::elapsed_ms(merge_gate_start);
        info!(
            result_count = response.results.len(),
            mode = ?kb_plan.mode,
            forced_kb = forced_kb,
            limit = kb_search_limit,
            "RAG search completed"
        );
        debug!(
            merged_preview = summarize_search_results(&response.results, 8),
            "RAG results before overlap filter"
        );
        plan_timings.total_ms = super::elapsed_ms(search_plan_start);
        (response, plan_timings)
    }
}

pub(super) fn derive_kb_search_limit(
    tool_output_settings: &ToolOutputSettingsDto,
    tuning: &RetrievalTuningSettingsDto,
) -> usize {
    let min_limit = tuning.kb_search_min_limit as usize;
    let max_limit = tuning.kb_search_max_limit as usize;
    (tool_output_settings.max_results as usize)
        .max(min_limit)
        .min(max_limit)
}

pub(super) fn derive_doc_shortlist_candidate_limit(
    kb_search_limit: usize,
    tuning: &RetrievalTuningSettingsDto,
) -> usize {
    (kb_search_limit.saturating_mul(4))
        .max(tuning.doc_shortlist_candidate_min as usize)
        .min(tuning.doc_shortlist_candidate_max as usize)
}

pub(super) fn derive_doc_shortlist_doc_limit(
    kb_search_limit: usize,
    tuning: &RetrievalTuningSettingsDto,
) -> usize {
    (kb_search_limit / 2)
        .max(tuning.doc_shortlist_doc_min as usize)
        .min(tuning.doc_shortlist_doc_max as usize)
}

pub(super) fn apply_rag_post_filters(
    mut search_response: SearchResponseDto,
    validated_message: &str,
    hyde_text: Option<&str>,
    followup_anchor_terms: Option<&HashSet<String>>,
    tuning: &RetrievalTuningSettingsDto,
) -> SearchResponseDto {
    let before_overlap_filter = search_response.results.len();
    search_response.results = super::filter_results_by_query_overlap_with_tuning(
        search_response.results,
        validated_message,
        hyde_text,
        followup_anchor_terms,
        tuning,
    );
    search_response.total = search_response.results.len();
    if search_response.results.len() < before_overlap_filter {
        info!(
            before = before_overlap_filter,
            after = search_response.results.len(),
            "Applied lexical overlap filter to RAG results"
        );
    }
    if search_response.results.is_empty() && before_overlap_filter > 0 {
        warn!(
            before = before_overlap_filter,
            "Overlap filter removed all RAG results"
        );
    } else {
        debug!(
            filtered_preview = summarize_search_results(&search_response.results, 8),
            "RAG results after overlap filter"
        );
    }

    let before_doc_support_filter = search_response.results.len();
    search_response.results =
        super::filter_results_by_document_support_with_tuning(search_response.results, tuning);
    search_response.total = search_response.results.len();
    if search_response.results.len() < before_doc_support_filter {
        info!(
            before = before_doc_support_filter,
            after = search_response.results.len(),
            "Applied document support filter to RAG results"
        );
    }
    debug!(
        doc_filtered_preview = summarize_search_results(&search_response.results, 8),
        "RAG results after document support filter"
    );

    search_response
}

fn merge_search_results(
    primary: SearchResponseDto,
    secondary: SearchResponseDto,
) -> SearchResponseDto {
    let mut seen: HashSet<String> = HashSet::new();
    let mut merged = Vec::new();
    let total_time = primary
        .query_time_ms
        .saturating_add(secondary.query_time_ms);

    for result in primary.results.into_iter() {
        if seen.insert(result.id.clone()) {
            merged.push(result);
        }
    }

    for result in secondary.results.into_iter() {
        if seen.insert(result.id.clone()) {
            merged.push(result);
        }
    }

    SearchResponseDto {
        results: merged,
        total: seen.len(),
        query_time_ms: total_time,
    }
}

fn summarize_search_results(results: &[SearchResultDto], max_items: usize) -> String {
    if results.is_empty() {
        return "<none>".to_string();
    }

    results
        .iter()
        .take(max_items)
        .map(|result| {
            let title = safe_truncate(&result.title, 64);
            let doc_id = result.document_id.as_deref().unwrap_or("-");
            format!(
                "{}|score={:.4}|doc={}|title=\"{}\"",
                result.id, result.score, doc_id, title
            )
        })
        .collect::<Vec<_>>()
        .join("; ")
}

use crate::domain::qa::hyde::QueryType;
use crate::features::function_calling::dto::{WebSearchResult, WikiSearchOutput};
use crate::features::qa::dto::SourceDto;
use crate::features::search::dto::{
    SearchModeDto, SearchRequestDto, SearchResponseDto, SearchResultDto,
};
use crate::features::settings::dto::{
    RetrievalTuningSettingsDto, RouterSettingsDto, SearchSettingsDto, ToolOutputSettingsDto,
};
use crate::infrastructure::search::query_expansion::dictionaries::select_informative_terms;
use crate::infrastructure::services::router::RouterAction;
use crate::interfaces::di::Container;
use crate::shared::error::Result;
use crate::shared::text_utils::safe_truncate;
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Instant;
use tracing::{debug, info, warn};

use super::SearchFlags;
mod conversation_helpers;
mod external_query;
mod followup_context;
mod kb_execution;
mod kb_retrieval;
mod keyword;
mod overlap;
mod pipeline;
mod policy;
mod rerank;
mod result_filters;
mod source_citations;
mod tool_format;
use self::conversation_helpers::{
    apply_hard_space_scope_filter as apply_hard_space_scope_filter_impl,
    build_hyde_context_window_for_conversation as build_hyde_context_window_for_conversation_impl,
    load_recent_document_metadata as load_recent_document_metadata_impl,
    load_space_document_scope as load_space_document_scope_impl,
    persist_document_references as persist_document_references_impl,
    record_tool_document_references as record_tool_document_references_impl,
};
use self::external_query::{
    select_web_search_query, select_web_search_query_with_tuning, select_wiki_search_query,
    select_wiki_search_query_with_tuning,
};
use self::followup_context::{
    build_followup_context as build_followup_context_impl,
    extract_turn_anchor_terms as extract_turn_anchor_terms_impl,
    load_followup_turn_anchor_terms as load_followup_turn_anchor_terms_impl,
    source_excerpt_text as source_excerpt_text_impl,
};
use self::kb_execution::{
    apply_rag_post_filters as apply_rag_post_filters_impl,
    derive_doc_shortlist_candidate_limit as derive_doc_shortlist_candidate_limit_impl,
    derive_doc_shortlist_doc_limit as derive_doc_shortlist_doc_limit_impl,
    derive_kb_search_limit as derive_kb_search_limit_impl,
    execute_kb_search_plan as execute_kb_search_plan_impl,
};
use self::kb_retrieval::run_kb_retrieval as run_kb_retrieval_impl;
pub(super) use self::keyword::{build_keyword_query_plan, KeywordQueryPlan};
use self::keyword::{
    expand_keyword_plan_with_rm3, extract_phrase_terms, normalize_keyword_token, stem_token,
    tokenize_keyword_terms,
};
use self::overlap::{
    extract_overlap_query_terms as extract_overlap_query_terms_impl, extract_query_anchor_terms,
    filter_followup_anchor_terms_by_candidate_coverage,
    filter_results_by_query_overlap as filter_results_by_query_overlap_impl,
    filter_results_by_query_overlap_with_tuning, select_informative_overlap_terms,
    tokenize_overlap_terms,
};
use self::pipeline::run_retrieval_pipeline as run_retrieval_pipeline_impl;
use self::policy::{
    empty_search_response, should_execute_external_lookup, should_use_external_as_fallback,
};
use self::rerank::apply_rerank_stage as apply_rerank_stage_impl;
use self::result_filters::{
    build_document_shortlist as build_document_shortlist_impl,
    filter_results_by_document_shortlist as filter_results_by_document_shortlist_impl,
    filter_results_by_document_support as filter_results_by_document_support_impl,
    filter_results_by_document_support_with_tuning as filter_results_by_document_support_with_tuning_impl,
    should_apply_document_shortlist as should_apply_document_shortlist_impl,
    should_apply_document_shortlist_with_tuning as should_apply_document_shortlist_with_tuning_impl,
};
use self::source_citations::{
    assign_citation_ids as assign_citation_ids_impl,
    build_source_citations as build_source_citations_impl,
    build_web_source_citations as build_web_source_citations_impl,
    citation_ids_by_document as citation_ids_by_document_impl,
    deduplicate_sources as deduplicate_sources_impl, infer_category as infer_category_impl,
};
use self::tool_format::format_tool_result as format_tool_result_impl;

/// The vault-wide space. A conversation in any other space is scoped to the
/// documents linked to it, which is what the retrieval trace reports as
/// `scope: "linked"`.
const DEFAULT_SPACE_ID: &str = "space_general";

const CLARIFY_NO_RECENT_DOCUMENT_PROMPT: &str =
    "I don't see a recent linked document yet. Do you want me to search your documents, or answer this generally?";

pub(super) struct RouterDecisionOutcome {
    pub(super) action: RouterAction,
    pub(super) recent_doc_meta: Option<RecentDocumentMetadata>,
    pub(super) clarify_message: Option<String>,
}

#[derive(Debug)]
pub(super) struct RetrievalPipelineOutcome {
    pub(super) short_circuit_response: Option<String>,
    pub(super) interpretation: crate::domain::qa::hyde::HyDEInterpretation,
    pub(super) search_response: crate::features::search::dto::SearchResponseDto,
    pub(super) followup_context: Option<(String, Vec<SourceDto>)>,
    pub(super) web_context: Option<String>,
    pub(super) web_search_error: Option<String>,
    pub(super) kb_unavailable_reason: Option<String>,
    pub(super) sources: Vec<SourceDto>,
    pub(super) available_for_rag: usize,
    pub(super) sub_timings: RetrievalSubTimingMetrics,
    /// Size of the document set the hard space-scope filter actually allowed.
    /// Not the size of the corpus — a scoped conversation must not claim to
    /// have read the whole vault (BRIEF rank 17, contract §4.6).
    pub(super) searched_documents: usize,
    /// True when the conversation lives outside the general space, i.e. its
    /// retrieval was scoped to the documents linked to it.
    pub(super) scope_is_linked: bool,
}

#[derive(Debug, Serialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct RetrievalSubTimingMetrics {
    pub kb_total_ms: u64,
    pub kb_scope_load_ms: u64,
    pub kb_hyde_interpretation_ms: u64,
    pub kb_search_plan_ms: u64,
    pub kb_shortlist_planning_ms: u64,
    pub kb_query_execution_ms: u64,
    pub kb_merge_shortlist_gate_ms: u64,
    pub kb_post_filters_ms: u64,
    pub kb_rerank_ms: u64,
    pub kb_build_sources_ms: u64,
    pub kb_persist_references_ms: u64,
    pub external_hyde_interpretation_ms: u64,
    pub wiki_search_ms: u64,
    pub web_search_ms: u64,
    pub total_ms: u64,
}

#[derive(Debug, Clone)]
struct RetrievalPlan {
    route_action: RouterAction,
    should_search_kb: bool,
    should_search_web: bool,
    should_search_wiki: bool,
    short_circuit_response: Option<String>,
}

impl RetrievalPlan {
    fn from_router(
        router_settings: &RouterSettingsDto,
        router_decision: &RouterDecisionOutcome,
        validated_message: &str,
        search_flags: SearchFlags,
    ) -> Self {
        let mut route_action = router_decision.action.clone();
        if search_flags.force_web_search {
            route_action = RouterAction::NewSearch;
        }
        if route_action == RouterAction::Clarify && router_decision.recent_doc_meta.is_none() {
            route_action = RouterAction::NewSearch;
        }

        let short_circuit_response = if route_action == RouterAction::Clarify
            && !search_flags.force_kb_search
            && !search_flags.force_web_search
            && router_decision.recent_doc_meta.is_some()
        {
            Some(build_router_clarify_response(
                router_settings,
                &router_decision.recent_doc_meta,
                validated_message,
                router_decision.clarify_message.as_deref(),
            ))
        } else {
            None
        };

        Self {
            route_action: route_action.clone(),
            should_search_kb: search_flags.force_kb_search
                || (!search_flags.force_web_search
                    && matches!(
                        route_action,
                        RouterAction::NewSearch | RouterAction::UseLastDocument
                    )),
            should_search_web: search_flags.force_web_search,
            should_search_wiki: search_flags.force_wiki_search,
            short_circuit_response,
        }
    }

    fn enable_kb_fallback(&mut self, enabled: bool) {
        if enabled {
            self.should_search_kb = true;
        }
    }
}

#[derive(Debug, Clone)]
struct KbSearchPlan {
    query: String,
    mode: SearchModeDto,
    threshold: Option<f32>,
    run_parallel_keyword_branch: bool,
}

impl KbSearchPlan {
    fn from_interpretation(
        validated_message: &str,
        interpretation: &crate::domain::qa::hyde::HyDEInterpretation,
        search_flags: SearchFlags,
        semantic_threshold: f32,
    ) -> Self {
        let hyde_text = interpretation.hyde_text.as_deref();
        let search_query = if let Some(hyde) = hyde_text {
            // Anchor HyDE with high-signal query terms (not full conversational text)
            // so vector retrieval stays in-domain without over-weighting filler words.
            let raw_tokens = tokenize_keyword_terms(validated_message);
            let anchor_terms = {
                let selected = select_informative_terms(raw_tokens.clone(), 14);
                if selected.is_empty() {
                    raw_tokens
                } else {
                    selected
                }
            };
            if anchor_terms.is_empty() {
                format!("{validated_message}\n\n{hyde}")
            } else {
                format!("{}\n\n{hyde}", anchor_terms.join(" "))
            }
        } else {
            validated_message.to_string()
        };
        let has_distinct_hyde_query = hyde_text
            .map(|hyde| !hyde.trim().eq_ignore_ascii_case(validated_message.trim()))
            .unwrap_or(false);
        let run_parallel_keyword_branch = has_distinct_hyde_query;
        let mode = if run_parallel_keyword_branch {
            SearchModeDto::Vector
        } else if search_flags.force_kb_search {
            SearchModeDto::Hybrid {
                vector_weight: 0.7,
                bm25_weight: 0.3,
            }
        } else {
            SearchModeDto::Vector
        };

        Self {
            query: search_query,
            mode,
            threshold: if run_parallel_keyword_branch {
                Some(0.05)
            } else {
                Some(semantic_threshold.clamp(0.0, 1.0))
            },
            run_parallel_keyword_branch,
        }
    }
}

#[derive(Debug)]
struct KbRetrievalOutcome {
    interpretation: crate::domain::qa::hyde::HyDEInterpretation,
    search_response: SearchResponseDto,
    sources: Vec<SourceDto>,
    low_confidence: bool,
    kb_unavailable_reason: Option<String>,
    timings: RetrievalSubTimingMetrics,
    /// How many documents the space scope allowed us to search.
    searched_documents: usize,
    /// Whether that scope was a linked-document space rather than the vault.
    scope_is_linked: bool,
}

#[derive(Debug, Clone, Default)]
struct KbSearchPlanTimingMetrics {
    shortlist_planning_ms: u64,
    query_execution_ms: u64,
    merge_shortlist_gate_ms: u64,
    total_ms: u64,
}

#[derive(Debug)]
struct SpaceDocumentScope {
    space_id: String,
    document_ids: HashSet<String>,
}

// This facade mirrors the implementation's complete per-turn retrieval configuration.
#[allow(clippy::too_many_arguments)]
pub(super) async fn run_retrieval_pipeline(
    container: &Container,
    conv_service: &Arc<dyn crate::features::conversation::ConversationServiceTrait>,
    conversation_id: &str,
    validated_message: &str,
    llm: &Arc<dyn crate::application::ports::LLMPort>,
    router_settings: &RouterSettingsDto,
    router_decision: &RouterDecisionOutcome,
    search_flags: SearchFlags,
    conversation_document_context: &[crate::domain::conversation::DocumentReference],
    highlight_terms: &[String],
    context: &[String],
    max_tokens: usize,
    tool_output_settings: &ToolOutputSettingsDto,
    search_settings: &SearchSettingsDto,
) -> RetrievalPipelineOutcome {
    run_retrieval_pipeline_impl(
        container,
        conv_service,
        conversation_id,
        validated_message,
        llm,
        router_settings,
        router_decision,
        search_flags,
        conversation_document_context,
        highlight_terms,
        context,
        max_tokens,
        tool_output_settings,
        search_settings,
    )
    .await
}

fn elapsed_ms(start: Instant) -> u64 {
    u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX)
}

// This facade mirrors the knowledge-base retrieval policy inputs.
#[allow(clippy::too_many_arguments)]
async fn run_kb_retrieval(
    container: &Container,
    conv_service: &Arc<dyn crate::features::conversation::ConversationServiceTrait>,
    conversation_id: &str,
    validated_message: &str,
    llm: &Arc<dyn crate::application::ports::LLMPort>,
    search_flags: SearchFlags,
    highlight_terms: &[String],
    kb_search_limit: usize,
    enable_reranking: bool,
    semantic_threshold: f32,
    tuning: &RetrievalTuningSettingsDto,
) -> KbRetrievalOutcome {
    run_kb_retrieval_impl(
        container,
        conv_service,
        conversation_id,
        validated_message,
        llm,
        search_flags,
        highlight_terms,
        kb_search_limit,
        enable_reranking,
        semantic_threshold,
        tuning,
    )
    .await
}

async fn apply_rerank_stage(
    container: &Container,
    validated_message: &str,
    interpretation: &crate::domain::qa::hyde::HyDEInterpretation,
    search_response: SearchResponseDto,
    followup_anchor_terms: Option<&HashSet<String>>,
    tuning: &RetrievalTuningSettingsDto,
) -> SearchResponseDto {
    apply_rerank_stage_impl(
        container,
        validated_message,
        interpretation,
        search_response,
        followup_anchor_terms,
        tuning,
    )
    .await
}

fn is_low_confidence_kb_response(search_response: &SearchResponseDto) -> bool {
    if search_response.results.is_empty() {
        return true;
    }

    let top_score = search_response
        .results
        .first()
        .map(|result| result.score)
        .unwrap_or(0.0);
    let second_score = search_response
        .results
        .get(1)
        .map(|result| result.score)
        .unwrap_or(0.0);
    let gap = (top_score - second_score).max(0.0);

    if top_score < 0.12 {
        return true;
    }
    if search_response.results.len() == 1 && top_score < 0.28 {
        return true;
    }
    if search_response.results.len() >= 2 && top_score < 0.22 && gap < 0.03 {
        return true;
    }

    false
}

async fn load_space_document_scope(
    container: &Container,
    conversation_id: &str,
) -> Option<SpaceDocumentScope> {
    load_space_document_scope_impl(container, conversation_id).await
}

fn apply_hard_space_scope_filter(
    response: SearchResponseDto,
    space_id: &str,
    scoped_document_ids: &HashSet<String>,
) -> SearchResponseDto {
    apply_hard_space_scope_filter_impl(response, space_id, scoped_document_ids)
}

async fn build_hyde_context_window_for_conversation(
    conv_service: &Arc<dyn crate::features::conversation::ConversationServiceTrait>,
    conversation_id: &str,
) -> Option<String> {
    build_hyde_context_window_for_conversation_impl(conv_service, conversation_id).await
}

async fn execute_kb_search_plan(
    container: &Container,
    validated_message: &str,
    interpretation: &crate::domain::qa::hyde::HyDEInterpretation,
    kb_plan: &KbSearchPlan,
    forced_kb: bool,
    kb_search_limit: usize,
    scope: &SpaceDocumentScope,
    tuning: &RetrievalTuningSettingsDto,
) -> (SearchResponseDto, KbSearchPlanTimingMetrics) {
    execute_kb_search_plan_impl(
        container,
        validated_message,
        interpretation,
        kb_plan,
        forced_kb,
        kb_search_limit,
        scope,
        tuning,
    )
    .await
}

fn derive_kb_search_limit(
    tool_output_settings: &ToolOutputSettingsDto,
    tuning: &RetrievalTuningSettingsDto,
) -> usize {
    derive_kb_search_limit_impl(tool_output_settings, tuning)
}

fn derive_doc_shortlist_candidate_limit(
    kb_search_limit: usize,
    tuning: &RetrievalTuningSettingsDto,
) -> usize {
    derive_doc_shortlist_candidate_limit_impl(kb_search_limit, tuning)
}

fn derive_doc_shortlist_doc_limit(
    kb_search_limit: usize,
    tuning: &RetrievalTuningSettingsDto,
) -> usize {
    derive_doc_shortlist_doc_limit_impl(kb_search_limit, tuning)
}

fn apply_rag_post_filters(
    search_response: SearchResponseDto,
    validated_message: &str,
    hyde_text: Option<&str>,
    followup_anchor_terms: Option<&HashSet<String>>,
    tuning: &RetrievalTuningSettingsDto,
) -> SearchResponseDto {
    apply_rag_post_filters_impl(
        search_response,
        validated_message,
        hyde_text,
        followup_anchor_terms,
        tuning,
    )
}

async fn persist_document_references(
    conv_service: &Arc<dyn crate::features::conversation::ConversationServiceTrait>,
    conversation_id: &str,
    results: &[crate::features::search::dto::SearchResultDto],
) -> Result<()> {
    persist_document_references_impl(conv_service, conversation_id, results).await
}

pub(super) async fn record_tool_document_references(
    conv_service: &Arc<dyn crate::features::conversation::ConversationServiceTrait>,
    conversation_id: &str,
    tool_name: &str,
    result: &crate::features::function_calling::domain::FunctionResult,
) -> Result<()> {
    record_tool_document_references_impl(conv_service, conversation_id, tool_name, result).await
}

#[derive(Debug, Clone)]
pub(super) struct RecentDocumentMetadata {
    pub(super) document_id: String,
    pub(super) title: String,
}

pub(super) async fn load_recent_document_metadata(
    container: &Container,
    document_context: &[crate::domain::conversation::DocumentReference],
) -> Option<RecentDocumentMetadata> {
    load_recent_document_metadata_impl(container, document_context).await
}

fn build_router_clarify_response(
    settings: &crate::features::settings::dto::RouterSettingsDto,
    recent_doc_meta: &Option<RecentDocumentMetadata>,
    _message: &str,
    override_question: Option<&str>,
) -> String {
    if let Some(custom) = override_question {
        if !custom.trim().is_empty() {
            if recent_doc_meta.is_none() && clarify_mentions_previous_document(custom) {
                return CLARIFY_NO_RECENT_DOCUMENT_PROMPT.to_string();
            }
            return custom.to_string();
        }
    }

    let title = recent_doc_meta.as_ref().map(|m| m.title.as_str());

    let Some(title) = title else {
        return CLARIFY_NO_RECENT_DOCUMENT_PROMPT.to_string();
    };

    settings
        .clarify_prompt_template
        .replace("{recent_document_title}", title)
}

fn clarify_mentions_previous_document(text: &str) -> bool {
    let normalized = text.to_lowercase();
    normalized.contains("previous document") || normalized.contains("recent document")
}

pub(super) fn extract_overlap_query_terms(query: &str) -> Vec<String> {
    extract_overlap_query_terms_impl(query)
}

pub(super) fn filter_results_by_query_overlap(
    results: Vec<SearchResultDto>,
    query: &str,
    hyde_text: Option<&str>,
    followup_anchor_terms: Option<&HashSet<String>>,
) -> Vec<SearchResultDto> {
    filter_results_by_query_overlap_impl(results, query, hyde_text, followup_anchor_terms)
}

fn build_document_shortlist(results: &[SearchResultDto], max_docs: usize) -> HashSet<String> {
    build_document_shortlist_impl(results, max_docs)
}

fn filter_results_by_document_shortlist(
    response: SearchResponseDto,
    document_shortlist: &HashSet<String>,
) -> SearchResponseDto {
    filter_results_by_document_shortlist_impl(response, document_shortlist)
}

fn should_apply_document_shortlist(
    shortlist_response: &SearchResponseDto,
    document_shortlist: &HashSet<String>,
) -> bool {
    should_apply_document_shortlist_impl(shortlist_response, document_shortlist)
}

fn should_apply_document_shortlist_with_tuning(
    shortlist_response: &SearchResponseDto,
    document_shortlist: &HashSet<String>,
    tuning: &RetrievalTuningSettingsDto,
) -> bool {
    should_apply_document_shortlist_with_tuning_impl(shortlist_response, document_shortlist, tuning)
}

pub(super) fn filter_results_by_document_support(
    results: Vec<SearchResultDto>,
) -> Vec<SearchResultDto> {
    filter_results_by_document_support_impl(results)
}

fn filter_results_by_document_support_with_tuning(
    results: Vec<SearchResultDto>,
    tuning: &RetrievalTuningSettingsDto,
) -> Vec<SearchResultDto> {
    filter_results_by_document_support_with_tuning_impl(results, tuning)
}

pub(super) fn extract_acronym_terms(query: &str) -> std::collections::HashSet<String> {
    query
        .split_whitespace()
        .map(|token| {
            token
                .chars()
                .filter(|c| c.is_ascii_alphanumeric())
                .collect::<String>()
        })
        .filter(|token| {
            let len = token.len();
            (2..=5).contains(&len)
                && token.chars().all(|c| c.is_ascii_uppercase())
                && token.chars().any(|c| c.is_ascii_alphabetic())
        })
        .map(|token| token.to_ascii_lowercase())
        .collect()
}

pub(super) fn extract_acronym_context_terms(
    query: &str,
    hyde_text: Option<&str>,
) -> std::collections::HashSet<String> {
    let mut terms = std::collections::HashSet::new();
    if hyde_text.is_none() || extract_acronym_terms(query).is_empty() {
        return terms;
    }

    let query_terms = tokenize_overlap_terms(query);
    if let Some(hyde) = hyde_text {
        for term in tokenize_overlap_terms(hyde) {
            if term.len() < 4
                || query_terms.contains(&term)
                || !term.chars().any(|c| c.is_ascii_alphabetic())
            {
                continue;
            }
            terms.insert(term);
            if terms.len() >= 20 {
                break;
            }
        }
    }

    terms
}
async fn build_followup_context(
    container: &Container,
    conv_service: &Arc<dyn crate::features::conversation::ConversationServiceTrait>,
    conversation_id: &str,
    document_context: &[crate::domain::conversation::DocumentReference],
    highlight_terms: &[String],
    token_budget: usize,
    llm: &Arc<dyn crate::application::ports::LLMPort>,
    excerpt_chars: usize,
) -> Option<(String, Vec<SourceDto>)> {
    build_followup_context_impl(
        container,
        conv_service,
        conversation_id,
        document_context,
        highlight_terms,
        token_budget,
        llm,
        excerpt_chars,
    )
    .await
}

fn extract_turn_anchor_terms(
    user_text: &str,
    assistant_text: &str,
) -> std::collections::HashSet<String> {
    extract_turn_anchor_terms_impl(user_text, assistant_text)
}

async fn load_followup_turn_anchor_terms(
    conv_service: &Arc<dyn crate::features::conversation::ConversationServiceTrait>,
    conversation_id: &str,
) -> std::collections::HashSet<String> {
    load_followup_turn_anchor_terms_impl(conv_service, conversation_id).await
}

fn source_excerpt_text(source: &SourceDto) -> Option<String> {
    source_excerpt_text_impl(source)
}

/// Deduplicate sources by `document_id`, while preserving multiple chunk excerpts per document.
pub(super) fn deduplicate_sources(sources: Vec<SourceDto>) -> Vec<SourceDto> {
    deduplicate_sources_impl(sources)
}

/// Number the final source list so the prompt can cite the same numbers.
pub(super) fn assign_citation_ids(sources: &mut [SourceDto]) {
    assign_citation_ids_impl(sources)
}

/// Map document id -> the citation number the model was given.
pub(super) fn citation_ids_by_document(
    sources: &[SourceDto],
) -> std::collections::HashMap<String, u32> {
    citation_ids_by_document_impl(sources)
}

/// Build source citations from search results with parallel document lookups.
///
/// Fetches document metadata in parallel to avoid N+1 queries, then constructs
/// `SourceDto` entries with full citation metadata for frontend display.
async fn build_source_citations(
    results: &[crate::features::search::dto::SearchResultDto],
    container: &Container,
    highlight_terms: &[String],
) -> Vec<SourceDto> {
    build_source_citations_impl(results, container, highlight_terms).await
}

/// Build source citations from web search results.
pub(super) fn build_web_source_citations(
    results: &[WebSearchResult],
    highlight_terms: &[String],
    excerpt_chars: usize,
) -> Vec<SourceDto> {
    build_web_source_citations_impl(results, highlight_terms, excerpt_chars)
}

/// Infer human-readable category from file path/extension.
fn infer_category(path: &str) -> String {
    infer_category_impl(path)
}

pub(super) fn format_tool_result(
    tool_name: &str,
    result: &crate::features::function_calling::domain::FunctionResult,
    highlight_terms: &[String],
    settings: &ToolOutputSettingsDto,
) -> String {
    format_tool_result_impl(tool_name, result, highlight_terms, settings)
}

#[cfg(test)]
mod tests;

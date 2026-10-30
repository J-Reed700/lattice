use crate::features::conversation::chat::ports::{ChatRuntime, SettingsDto};
use crate::features::conversation::chat::router::RouterAction;
use lattice_core::application::contracts::search::{SearchResponseDto, SearchResultDto};
use lattice_core::application::contracts::settings::{
    RetrievalTuningSettingsDto, RouterSettingsDto, ToolOutputSettingsDto,
};
use lattice_core::application::contracts::sources::SourceDto;
use lattice_core::application::contracts::tools::WikiSearchOutput;
use lattice_core::application::contracts::web::WebSearchResult;
use lattice_core::application::services::context_assembler::EvidenceBudget;
use lattice_core::domain::qa::hyde::QueryType;
use lattice_core::shared::error::Result;
use lattice_core::shared::text::safe_truncate;
use lattice_core::shared::text::select_informative_terms;
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Instant;
use tracing::{debug, info, warn};

use super::{ChatTurn, SearchFlags};
mod conversation_helpers;
mod corpus_plan;
pub(super) use corpus_plan::fused_search;
pub(crate) use corpus_plan::PlannerHealth;
mod external_query;
pub(super) use external_query::search_text_without_url_tracking;
mod followup_context;
mod kb_retrieval;
mod keyword;
mod overlap;
pub(in crate::features::conversation::chat) mod page_budget;
mod pipeline;
mod policy;
mod rerank;
mod source_citations;
pub use self::source_citations::WEB_SOURCE_PREFIX;
pub(super) use self::source_citations::{
    drop_unbudgeted_sources, web_context_entries, WebContextItem,
};
mod sufficiency;
mod summary_tier;
mod tool_format;
use self::conversation_helpers::{
    build_hyde_context_window_for_conversation as build_hyde_context_window_for_conversation_impl,
    load_recent_document_metadata as load_recent_document_metadata_impl,
    load_space_document_scope as load_space_document_scope_impl,
    persist_document_references as persist_document_references_impl,
    record_tool_document_references as record_tool_document_references_impl,
};
#[cfg(test)]
use self::external_query::select_web_search_query;
use self::external_query::{
    select_web_search_query_with_tuning, select_wiki_search_query_with_tuning,
};
#[cfg(test)]
use self::followup_context::extract_turn_anchor_terms;
pub(super) use self::followup_context::FollowupDocument;
use self::followup_context::{
    build_followup_context as build_followup_context_impl,
    load_followup_turn_anchor_terms as load_followup_turn_anchor_terms_impl,
};
use self::kb_retrieval::run_kb_retrieval as run_kb_retrieval_impl;
use self::keyword::{extract_phrase_terms, normalize_keyword_token, tokenize_keyword_terms};
use self::overlap::tokenize_overlap_terms;
use self::pipeline::run_retrieval_pipeline as run_retrieval_pipeline_impl;
use self::policy::{
    empty_search_response, intent_web_fallback, kb_can_run_alongside_external,
    should_execute_external_lookup, should_use_external_as_fallback, WebSearchReason,
};
use self::rerank::apply_rerank_stage as apply_rerank_stage_impl;
use self::source_citations::{
    assign_citation_ids as assign_citation_ids_impl,
    build_source_citations as build_source_citations_impl,
    build_web_source_citations as build_web_source_citations_impl,
    citation_ids_by_chunk as citation_ids_by_chunk_impl,
    deduplicate_sources as deduplicate_sources_impl, infer_category as infer_category_impl,
    merge_tool_sources as merge_tool_sources_impl,
};
use self::sufficiency::assess_sufficiency;
/// The reason codes a [`SufficiencyVerdict`] carries, so the prompt can explain
/// a weak-evidence turn in the same terms the trace records it.
pub(in crate::features::conversation::chat) use self::sufficiency::reason as sufficiency_reason;
pub(super) use self::sufficiency::SufficiencyVerdict;
use self::tool_format::format_tool_result as format_tool_result_impl;

/// The vault-wide space. A conversation in any other space is scoped to the
/// documents linked to it, which is what the retrieval trace reports as
/// `scope: "linked"`.
const DEFAULT_SPACE_ID: &str = "space_general";

const CLARIFY_NO_RECENT_DOCUMENT_PROMPT: &str =
    "I don't see a recent linked document yet. Do you want me to search your documents, or answer this generally?";

/// The post-rerank sufficiency verdict, projected for callers outside the chat
/// feature (the retrieval evaluation harness) and computed by the pipeline's
/// own code.
#[derive(Debug, Clone, PartialEq)]
pub struct RetrievalSufficiency {
    /// False only when a corrective retry could plausibly do better.
    pub sufficient: bool,
    /// Blended score of the best passage. Comparable across runs only when
    /// `reranked` was true for both.
    pub top_score: f32,
    /// `top_score` minus the score at rank five, or minus the last result's
    /// score when fewer came back.
    pub spread: f32,
    /// Fraction of the informative query terms found in the top passages.
    pub term_coverage: f32,
    /// Reason codes, not prose. Empty when nothing was wrong.
    pub reasons: Vec<&'static str>,
}

/// Judges one pass of retrieval from a bare query list, as the keyword-search
/// plan shape (no ordered-reading exemptions). `reranked` must be true only
/// when a cross-encoder rescored the candidates: RRF ranks are not relevance
/// probabilities.
pub fn assess_retrieval_sufficiency(
    results: &[SearchResultDto],
    queries: &[String],
    tuning: &RetrievalTuningSettingsDto,
    reranked: bool,
) -> RetrievalSufficiency {
    let plan = self::corpus_plan::CorpusSearchPlan {
        queries: queries.to_vec(),
        opening_document_ids: Vec::new(),
        start_at_beginning: false,
    };
    let verdict = assess_sufficiency(results, &plan, tuning, reranked, results.len());
    RetrievalSufficiency {
        sufficient: verdict.sufficient,
        top_score: verdict.top_score,
        spread: verdict.spread,
        term_coverage: verdict.term_coverage,
        reasons: verdict.reasons,
    }
}

pub(super) struct RouterDecisionOutcome {
    pub(super) action: RouterAction,
    pub(super) recent_doc_meta: Option<RecentDocumentMetadata>,
    pub(super) clarify_message: Option<String>,
    /// Whether the router's own call (a new search, or the last document) may
    /// start a vault search. False in Explorer turns: the folder is the
    /// subject there, and the library is read when the knowledge-base or web
    /// toggle asks for it.
    pub(super) router_may_search: bool,
}

#[derive(Debug)]
pub(super) struct RetrievalPipelineOutcome {
    pub(super) short_circuit_response: Option<String>,
    pub(super) interpretation: lattice_core::domain::qa::hyde::HyDEInterpretation,
    pub(super) search_response: lattice_core::application::contracts::search::SearchResponseDto,
    pub(super) followup_context: Option<(FollowupDocument, Vec<SourceDto>)>,
    /// Wiki results first, then web, rendered once the sources are numbered.
    pub(super) web_context: Vec<WebContextItem>,
    pub(super) web_search_error: Option<String>,
    pub(super) kb_unavailable_reason: Option<String>,
    /// Whether the knowledge base was actually searched this turn, as distinct
    /// from searched and found nothing.
    pub(super) kb_attempted: bool,
    pub(super) sources: Vec<SourceDto>,
    /// The evidence budget retrieval was handed, unchanged: what retrieval
    /// fetched is charged against it when the prompt is assembled.
    pub(super) evidence: EvidenceBudget,
    pub(super) sub_timings: RetrievalSubTimingMetrics,
    /// Size of the document set the hard space-scope filter actually allowed.
    /// Not the size of the corpus — a scoped conversation must not claim to
    /// have read the whole vault.
    pub(super) searched_documents: usize,
    /// True when the conversation lives outside the general space, i.e. its
    /// retrieval was scoped to the documents linked to it.
    pub(super) scope_is_linked: bool,
    /// The post-rerank sufficiency verdict for the turn, after any corrective
    /// retry. `None` when knowledge-base retrieval did not run.
    pub(super) sufficiency: Option<SufficiencyVerdict>,
    /// Every web page retrieval tried to open before the model ran. The tool
    /// loop starts from this, so it never re-fetches a page the prompt carries
    /// or retries one that is already known to be blocked.
    pub(super) pages_read: super::fetch_memory::FetchMemory,
    /// Every web search retrieval ran before the model did, the first one
    /// first, so a deep-research turn can tell the model what not to repeat.
    pub(super) web_queries: Vec<String>,
}

#[derive(Debug, Serialize, Clone, Default, specta::Type)]
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
    /// Cost of the post-rerank sufficiency check. Local and cheap by design —
    /// if this is ever material, the check is doing too much.
    pub kb_sufficiency_ms: u64,
    /// The whole corrective second pass: replanning, both branches again, and
    /// the second rerank. Zero when no retry ran.
    pub kb_corrective_retry_ms: u64,
    /// 0 or 1. The loop is capped at exactly one replanned retry per turn.
    pub kb_corrective_retries: u32,
    /// The final sufficiency verdict, or `None` when KB retrieval never ran.
    /// Absent is not the same as insufficient.
    pub kb_sufficient: Option<bool>,
    /// True when a short follow-up reused the previous turn's topic and the
    /// planner LLM call was skipped.
    pub kb_planner_skipped: bool,
    pub kb_build_sources_ms: u64,
    pub kb_persist_references_ms: u64,
    /// Wall-clock of external query preparation: the utility model's
    /// interpretation and the web-query rewrite, which run concurrently.
    pub external_hyde_interpretation_ms: u64,
    pub wiki_search_ms: u64,
    /// The web search itself. Query rewriting is counted above, not here.
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
        if search_flags.web_planned() {
            route_action = RouterAction::NewSearch;
        }
        if route_action == RouterAction::Clarify && router_decision.recent_doc_meta.is_none() {
            route_action = RouterAction::NewSearch;
        }

        let short_circuit_response = if route_action == RouterAction::Clarify
            && !search_flags.force_kb_search
            && !search_flags.web_planned()
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
            // Web adds to the vault; it never replaces it. A web search the
            // user asked for runs beside the vault search. One the intent
            // step guessed at (`web_fallback`) runs after it, and only when
            // the vault falls short, so the vault has to be searched here or
            // the fallback has nothing to go by. A closed-book turn never
            // reaches this plan.
            should_search_kb: search_flags.force_kb_search
                || ((router_decision.router_may_search || search_flags.web_planned())
                    && matches!(
                        route_action,
                        RouterAction::NewSearch | RouterAction::UseLastDocument
                    )),
            should_search_web: search_flags.web_planned(),
            should_search_wiki: search_flags.force_wiki_search,
            short_circuit_response,
        }
    }
}

#[derive(Debug)]
struct KbRetrievalOutcome {
    interpretation: lattice_core::domain::qa::hyde::HyDEInterpretation,
    search_response: SearchResponseDto,
    sources: Vec<SourceDto>,
    low_confidence: bool,
    kb_unavailable_reason: Option<String>,
    timings: RetrievalSubTimingMetrics,
    /// How many documents the space scope allowed us to search.
    searched_documents: usize,
    /// Whether that scope was a linked-document space rather than the vault.
    scope_is_linked: bool,
    /// The verdict the corrective loop acted on, kept for the turn's trace.
    sufficiency: Option<SufficiencyVerdict>,
}

#[derive(Debug)]
struct SpaceDocumentScope {
    space_id: String,
    document_ids: HashSet<String>,
}

pub(super) async fn run_retrieval_pipeline(
    container: &dyn ChatRuntime,
    turn: ChatTurn<'_>,
    settings: &SettingsDto,
    router_decision: &RouterDecisionOutcome,
    search_flags: SearchFlags,
    conversation_document_context: &[lattice_core::domain::conversation::DocumentReference],
    evidence: EvidenceBudget,
    attachment_digest: Option<&str>,
) -> RetrievalPipelineOutcome {
    run_retrieval_pipeline_impl(
        container,
        turn,
        settings,
        router_decision,
        search_flags,
        conversation_document_context,
        evidence,
        attachment_digest,
    )
    .await
}

fn elapsed_ms(start: Instant) -> u64 {
    u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX)
}

async fn run_kb_retrieval(
    container: &dyn ChatRuntime,
    turn: ChatTurn<'_>,
    search_flags: SearchFlags,
    evidence_tokens: usize,
    chars_per_token: f64,
    enable_reranking: bool,
    semantic_threshold: f32,
    tuning: &RetrievalTuningSettingsDto,
    cancel: &tokio_util::sync::CancellationToken,
) -> KbRetrievalOutcome {
    run_kb_retrieval_impl(
        container,
        turn,
        search_flags,
        evidence_tokens,
        chars_per_token,
        enable_reranking,
        semantic_threshold,
        tuning,
        cancel,
    )
    .await
}

async fn apply_rerank_stage(
    container: &dyn ChatRuntime,
    validated_message: &str,
    interpretation: &lattice_core::domain::qa::hyde::HyDEInterpretation,
    search_response: SearchResponseDto,
    followup_anchor_terms: Option<&HashSet<String>>,
    tuning: &RetrievalTuningSettingsDto,
) -> (SearchResponseDto, bool) {
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

async fn load_space_document_scope(
    container: &dyn ChatRuntime,
    conversation_id: &str,
) -> Option<SpaceDocumentScope> {
    load_space_document_scope_impl(container, conversation_id).await
}

/// The documents a conversation remembers citing, minus any its space no
/// longer allows. The router and the follow-up path reopen a reference by id
/// with no scope filter in between, so the scope is applied here; an
/// unresolvable scope keeps nothing.
pub(super) async fn confine_document_context(
    container: &dyn ChatRuntime,
    conversation_id: &str,
    references: Vec<lattice_core::domain::conversation::DocumentReference>,
) -> Vec<lattice_core::domain::conversation::DocumentReference> {
    if references.is_empty() {
        return references;
    }
    let Some(scope) = load_space_document_scope(container, conversation_id).await else {
        return Vec::new();
    };
    keep_references_in_scope(references, &scope.document_ids)
}

fn keep_references_in_scope(
    references: Vec<lattice_core::domain::conversation::DocumentReference>,
    allowed: &HashSet<String>,
) -> Vec<lattice_core::domain::conversation::DocumentReference> {
    references
        .into_iter()
        .filter(|reference| allowed.contains(&reference.document_id))
        .collect()
}

async fn build_hyde_context_window_for_conversation(
    conv_service: &Arc<dyn crate::features::conversation::ConversationServiceTrait>,
    conversation_id: &str,
    question: &str,
    planner: &dyn lattice_core::application::ports::LLMPort,
    within_tokens: Option<usize>,
) -> Option<String> {
    build_hyde_context_window_for_conversation_impl(
        conv_service,
        conversation_id,
        question,
        planner,
        within_tokens,
    )
    .await
}

use lattice_core::application::services::grounded_generation::retrieval_depth;

/// The most library passages one turn reads, section neighbours included.
pub(super) const MAX_READ_PASSAGES: usize = 40;

/// How many library passages a turn searches for: as many as its evidence
/// budget holds, at most [`MAX_READ_PASSAGES`].
fn reading_depth(
    evidence_tokens: usize,
    passage_chars: Option<usize>,
    chars_per_token: f64,
) -> usize {
    retrieval_depth(evidence_tokens, passage_chars, chars_per_token).min(MAX_READ_PASSAGES)
}

async fn persist_document_references(
    conv_service: &Arc<dyn crate::features::conversation::ConversationServiceTrait>,
    conversation_id: &str,
    results: &[lattice_core::application::contracts::search::SearchResultDto],
) -> Result<()> {
    persist_document_references_impl(conv_service, conversation_id, results).await
}

pub(super) async fn record_tool_document_references(
    conv_service: &Arc<dyn crate::features::conversation::ConversationServiceTrait>,
    conversation_id: &str,
    tool_name: &str,
    result: &lattice_core::application::contracts::tools::FunctionResult,
) -> Result<()> {
    record_tool_document_references_impl(conv_service, conversation_id, tool_name, result).await
}

#[derive(Debug, Clone)]
pub(super) struct RecentDocumentMetadata {
    pub(super) document_id: String,
    pub(super) title: String,
}

pub(super) async fn load_recent_document_metadata(
    container: &dyn ChatRuntime,
    document_context: &[lattice_core::domain::conversation::DocumentReference],
) -> Option<RecentDocumentMetadata> {
    load_recent_document_metadata_impl(container, document_context).await
}

fn build_router_clarify_response(
    settings: &lattice_core::application::contracts::settings::RouterSettingsDto,
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
    container: &dyn ChatRuntime,
    conv_service: &Arc<dyn crate::features::conversation::ConversationServiceTrait>,
    conversation_id: &str,
    document_context: &[lattice_core::domain::conversation::DocumentReference],
    highlight_terms: &[String],
    token_budget: usize,
    llm: &Arc<dyn lattice_core::application::ports::LLMPort>,
    excerpt_chars: usize,
) -> Option<(FollowupDocument, Vec<SourceDto>)> {
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

async fn load_followup_turn_anchor_terms(
    conv_service: &Arc<dyn crate::features::conversation::ConversationServiceTrait>,
    conversation_id: &str,
) -> std::collections::HashSet<String> {
    load_followup_turn_anchor_terms_impl(conv_service, conversation_id).await
}

/// Deduplicate repeated chunks while retaining distinct passages from each document.
pub(super) fn deduplicate_sources(sources: Vec<SourceDto>) -> Vec<SourceDto> {
    deduplicate_sources_impl(sources)
}

/// Fold tool-supplied sources into the turn's list, collapsing repeat visits to
/// the same web page onto the entry the model was already given.
pub(super) fn merge_tool_sources(
    sources: &mut Vec<SourceDto>,
    incoming: Vec<SourceDto>,
) -> std::collections::HashSet<String> {
    merge_tool_sources_impl(sources, incoming)
}

/// Number the final source list so the prompt can cite the same numbers.
pub(super) fn assign_citation_ids(sources: &mut [SourceDto]) {
    assign_citation_ids_impl(sources)
}

/// Map chunk id -> the citation number the model was given.
pub(super) fn citation_ids_by_chunk(
    sources: &[SourceDto],
) -> std::collections::HashMap<String, u32> {
    citation_ids_by_chunk_impl(sources)
}

/// Build source citations from search results with batched metadata lookups.
///
/// Fetches document and chunk metadata in batches, then constructs
/// `SourceDto` entries with full citation metadata for frontend display.
async fn build_source_citations(
    results: &[lattice_core::application::contracts::search::SearchResultDto],
    container: &dyn ChatRuntime,
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
pub(super) fn infer_category(path: &str) -> String {
    infer_category_impl(path)
}

pub(in crate::features::conversation::chat) use self::tool_format::{
    document_page_room, fetched_page_text_room,
};

pub(super) fn format_tool_result(
    tool_name: &str,
    result: &lattice_core::application::contracts::tools::FunctionResult,
    settings: &ToolOutputSettingsDto,
    allowance: usize,
    page_offset: usize,
    source_text: bool,
) -> String {
    format_tool_result_impl(
        tool_name,
        result,
        settings,
        allowance,
        page_offset,
        source_text,
    )
}

#[cfg(test)]
mod tests;

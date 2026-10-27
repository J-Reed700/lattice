use super::*;
use crate::features::conversation::chat::fetch_memory::{Delivery, FetchMemory};
use crate::features::conversation::chat::turn_record::{
    TurnRecorder, TurnStepKind, TurnStepLinkDto,
};
use crate::features::conversation::chat::web_steps::{self, result_count_line};
use lattice_core::application::services::context_assembler::EvidenceShare;
use std::time::Duration;

use lattice_core::domain::qa::hyde::HyDEInterpretation;

mod external_lookup;
mod web_pages;

/// Retrieval for one turn. Cancellation is checked at phase boundaries rather
/// than by racing the phases, since a search dropped mid-await would abandon
/// work the pipeline cannot clean up; the caller turns a cancelled outcome
/// into the cancellation error.
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
    let retrieval_start = Instant::now();
    let ChatTurn {
        conv_service,
        conversation_id,
        request_id,
        message: validated_message,
        llm,
        highlight_terms,
        recorder,
        ..
    } = turn;
    let router_settings = &settings.llm.router;
    let tool_output_settings = &settings.llm.tool_output;
    let search_settings = &settings.search;

    let mut outcome = RetrievalPipelineOutcome {
        short_circuit_response: None,
        interpretation: HyDEInterpretation::raw_only(
            validated_message.to_string(),
            QueryType::Question,
        ),
        search_response: lattice_core::application::contracts::search::SearchResponseDto {
            results: vec![],
            total: 0,
            query_time_ms: 0,
        },
        followup_context: None,
        web_context: Vec::new(),
        web_search_error: None,
        kb_unavailable_reason: None,
        kb_attempted: false,
        sources: Vec::new(),
        evidence,
        sub_timings: RetrievalSubTimingMetrics::default(),
        searched_documents: 0,
        scope_is_linked: false,
        sufficiency: None,
        pages_read: FetchMemory::default(),
        web_queries: Vec::new(),
    };
    // A closed-book turn stops here, before a model is loaded or a scope is
    // resolved. Gating each search below instead would leave the next path
    // added to this function open by default.
    if search_flags.closed_book {
        return outcome;
    }

    // Held until KB retrieval has reported. This is the single honest source for
    // the "answered without your documents" line: the frontend must never
    // infer it.
    let mut embedding_unavailable_reason: Option<String> = None;
    if let Err(e) = container.get_or_load_embedding().await {
        debug!(error = %e, "Embedding model not available for RAG — search will be skipped");
        embedding_unavailable_reason = Some("the embedding model is not ready".to_string());
    }

    let utility_llm: Arc<dyn lattice_core::application::ports::LLMPort> =
        match (container.utility_llm_loader())().await {
            Ok(Some(util)) => {
                tracing::debug!("Using configured utility LLM for HyDE/query-rewrite");
                util
            }
            Ok(None) => {
                tracing::debug!(
                    "No utility LLM configured — external search rewriting may use the chat LLM \
                 (set one in Settings → Model Catalog to speed up retrieval)"
                );
                Arc::clone(llm)
            }
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    "Utility LLM load failed — external search rewriting may use the chat LLM"
                );
                Arc::clone(llm)
            }
        };

    let tuning = &search_settings.retrieval_tuning;
    // The embedding and utility models may both have been cold.
    let turn_cancellation = container.turn_cancellation();
    if turn_cancellation.is_cancel_requested(request_id) {
        return outcome;
    }

    let mut retrieval_plan = RetrievalPlan::from_router(
        router_settings,
        router_decision,
        validated_message,
        search_flags,
    );
    let mut kb_attempted = false;
    let mut kb_has_results = false;
    let mut kb_low_confidence = true;
    outcome.short_circuit_response = retrieval_plan.short_circuit_response.take();

    if outcome.short_circuit_response.is_none()
        && retrieval_plan.route_action == RouterAction::UseLastDocument
    {
        outcome.followup_context = build_followup_context(
            container,
            conv_service,
            conversation_id,
            conversation_document_context,
            highlight_terms,
            // The document under discussion, like an attachment, outranks
            // what a search turns up but must leave room to search and cite.
            evidence.allowance(EvidenceShare::ATTACHMENTS),
            llm,
            tool_output_settings.excerpt_chars as usize,
        )
        .await;
    }
    // The follow-up document is charged before the passages and pages are sized
    // from what is left.
    let mut evidence = evidence;
    if let Some((document, _)) = &outcome.followup_context {
        evidence.charge(llm.count_tokens(&document.render_for_count()));
    }

    let external = ExternalLookup {
        container,
        cancellation: &turn_cancellation,
        conv_service,
        conversation_id,
        request_id,
        validated_message,
        utility_llm: &utility_llm,
        search_flags,
        highlight_terms,
        excerpt_chars: tool_output_settings.excerpt_chars as usize,
        tuning,
        page_budget_chars: super::page_budget::page_budget_chars(&evidence),
        wiki_planned: retrieval_plan.should_search_wiki,
        web_planned: retrieval_plan.should_search_web,
        attachment_digest,
        recorder,
    };
    let kb_search = async {
        let kb_retrieval_start = Instant::now();
        let kb_outcome = run_kb_retrieval(
            container,
            turn,
            search_flags,
            evidence.remaining(),
            llm.chars_per_token(),
            search_settings.enable_reranking,
            search_settings.similarity_threshold,
            tuning,
            &turn_cancellation.turn_token(request_id),
        )
        .await;
        (kb_outcome, elapsed_ms(kb_retrieval_start))
    };

    // The phase below is the long one — KB retrieval with its corrective
    // retries, and wiki/web lookups over the network.
    if turn_cancellation.is_cancel_requested(request_id) {
        return outcome;
    }

    let run_kb = outcome.short_circuit_response.is_none() && retrieval_plan.should_search_kb;
    // KB and the external phase overlap only when no KB result can close the
    // external-lookup gate. The low-confidence clarify below needs no wiki or
    // web search planned, so it never applies to a turn that takes this path.
    // A web search the intent step guessed at never takes it: the vault is
    // searched first and decides whether the web is asked at all.
    let kb_alongside_external =
        run_kb && external.is_planned() && kb_can_run_alongside_external(search_flags);
    let mut external_phase: Option<ExternalPhaseOutcome> = None;
    let kb_result = if kb_alongside_external {
        // The external phase interprets the turn itself instead of waiting on
        // KB's interpretation, so it starts from the raw message. `allow_lookup`
        // is true on the promise of `kb_can_run_alongside_external`: no KB
        // result can close the gate on a turn that takes this path. The merge
        // below re-checks the gate and complains if that promise ever breaks.
        let external_base = outcome.interpretation.clone();
        let interpret = external_base.hyde_text.is_none();
        let (kb_result, phase) = tokio::join!(
            kb_search,
            external.run(&external_base, interpret, true, WebSearchReason::Requested)
        );
        external_phase = Some(phase);
        Some(kb_result)
    } else if run_kb {
        Some(kb_search.await)
    } else {
        None
    };

    if let Some((kb_outcome, kb_total_ms)) = kb_result {
        kb_attempted = true;
        outcome.kb_attempted = true;
        outcome.interpretation = kb_outcome.interpretation;
        outcome.search_response = kb_outcome.search_response;
        outcome.sources = kb_outcome.sources;
        outcome.kb_unavailable_reason = kb_outcome.kb_unavailable_reason;
        outcome.sub_timings = kb_outcome.timings;
        outcome.searched_documents = kb_outcome.searched_documents;
        outcome.scope_is_linked = kb_outcome.scope_is_linked;
        outcome.sufficiency = kb_outcome.sufficiency;
        outcome.sub_timings.kb_total_ms = kb_total_ms;
        kb_has_results = !outcome.search_response.results.is_empty();
        kb_low_confidence = kb_outcome.low_confidence;

        if kb_outcome.low_confidence
            && !retrieval_plan.should_search_web
            && !retrieval_plan.should_search_wiki
            && outcome.short_circuit_response.is_none()
            && router_decision.recent_doc_meta.is_some()
        {
            outcome.short_circuit_response = Some(build_router_clarify_response(
                router_settings,
                &router_decision.recent_doc_meta,
                validated_message,
                None,
            ));
            outcome.search_response = empty_search_response();
            outcome.sources.clear();
            info!(
                conversation_id = conversation_id,
                "Low-confidence retrieval triggered clarify fallback"
            );
        }
    }

    // Only when the turn really answered without the vault: keyword search can
    // still return passages with the embedder down.
    if let Some(reason) = embedding_unavailable_reason {
        if outcome.sources.is_empty() {
            outcome.kb_unavailable_reason.get_or_insert(reason);
        }
    }

    let should_use_external_as_fallback =
        should_use_external_as_fallback(search_flags, &outcome.interpretation);
    let kb_sufficient = outcome
        .sufficiency
        .as_ref()
        .map(|verdict| verdict.sufficient);
    // A web search the intent step guessed at answers to the vault's verdict as
    // well as to its emptiness: passages too weak to answer from send the turn
    // out, passages that answer keep it on the machine. Every other external
    // search keeps the gate it always had.
    let intent_fallback = search_flags
        .web_fallback
        .then(|| intent_web_fallback(kb_attempted, kb_has_results, kb_sufficient));
    let allow_external_lookup = match intent_fallback {
        Some(reason) => reason.is_some(),
        None => should_execute_external_lookup(
            should_use_external_as_fallback,
            kb_attempted,
            kb_has_results,
            kb_low_confidence,
        ),
    };
    let web_reason = intent_fallback
        .flatten()
        .unwrap_or(WebSearchReason::Requested);
    if (retrieval_plan.should_search_web || retrieval_plan.should_search_wiki)
        && !allow_external_lookup
    {
        info!(
            conversation_id = conversation_id,
            kb_attempted = kb_attempted,
            kb_has_results = kb_has_results,
            kb_low_confidence = kb_low_confidence,
            kb_sufficient = ?kb_sufficient,
            web_fallback = search_flags.web_fallback,
            "Skipping external search because KB is sufficient for this turn"
        );
    } else if search_flags.web_fallback {
        info!(
            conversation_id = conversation_id,
            kb_has_results = kb_has_results,
            kb_sufficient = ?kb_sufficient,
            reason = ?web_reason,
            "Searching the web as the intent fallback: the vault fell short"
        );
    }

    if turn_cancellation.is_cancel_requested(request_id) {
        return outcome;
    }

    if external_phase.is_none() && outcome.short_circuit_response.is_none() && external.is_planned()
    {
        let interpret = outcome.interpretation.hyde_text.is_none();
        external_phase = Some(
            external
                .run(
                    &outcome.interpretation,
                    interpret,
                    allow_external_lookup,
                    web_reason,
                )
                .await,
        );
    }

    if let Some(phase) = external_phase {
        outcome.sub_timings.external_hyde_interpretation_ms = phase.query_prep_ms;
        // The utility model's interpretation replaces KB's unless KB produced
        // HyDE text of its own, which is when the sequential order skipped the
        // external interpretation. KB retrieval never does today, so both
        // orders hand chat the external interpretation whenever it succeeded.
        if outcome.interpretation.hyde_text.is_none() {
            if let Some(interpretation) = phase.interpretation {
                outcome.interpretation = interpretation;
            }
        }
        // Rechecked because a concurrent phase started before KB's
        // interpretation existed. Wiki before web whatever finished first, so
        // the merged sources and context match the sequential order.
        let fetched_anything = phase.wiki.is_some() || phase.web.is_some();
        debug_assert!(
            allow_external_lookup || !fetched_anything,
            "a concurrent external phase fetched results the gate then closed on"
        );
        if !allow_external_lookup && fetched_anything {
            warn!(
                conversation_id = conversation_id,
                "Discarding external results the gate closed on after they were fetched"
            );
        }
        // The short-circuit answer is the whole turn: merging searches onto it
        // would cite sources the reply never used.
        if allow_external_lookup && outcome.short_circuit_response.is_none() {
            if let Some(wiki) = phase.wiki {
                attach_wiki_results(&mut outcome, wiki);
            }
            if let Some(web) = phase.web {
                attach_web_results(&mut outcome, web);
            }
        }
    }
    outcome.sub_timings.total_ms = elapsed_ms(retrieval_start);
    info!(
        kb_total_ms = outcome.sub_timings.kb_total_ms,
        kb_scope_load_ms = outcome.sub_timings.kb_scope_load_ms,
        kb_hyde_interpretation_ms = outcome.sub_timings.kb_hyde_interpretation_ms,
        kb_search_plan_ms = outcome.sub_timings.kb_search_plan_ms,
        kb_shortlist_planning_ms = outcome.sub_timings.kb_shortlist_planning_ms,
        kb_query_execution_ms = outcome.sub_timings.kb_query_execution_ms,
        kb_merge_shortlist_gate_ms = outcome.sub_timings.kb_merge_shortlist_gate_ms,
        kb_post_filters_ms = outcome.sub_timings.kb_post_filters_ms,
        kb_rerank_ms = outcome.sub_timings.kb_rerank_ms,
        kb_sufficiency_ms = outcome.sub_timings.kb_sufficiency_ms,
        kb_corrective_retry_ms = outcome.sub_timings.kb_corrective_retry_ms,
        kb_corrective_retries = outcome.sub_timings.kb_corrective_retries,
        kb_sufficient = ?outcome.sub_timings.kb_sufficient,
        kb_planner_skipped = outcome.sub_timings.kb_planner_skipped,
        kb_build_sources_ms = outcome.sub_timings.kb_build_sources_ms,
        kb_persist_references_ms = outcome.sub_timings.kb_persist_references_ms,
        external_hyde_interpretation_ms = outcome.sub_timings.external_hyde_interpretation_ms,
        wiki_search_ms = outcome.sub_timings.wiki_search_ms,
        web_search_ms = outcome.sub_timings.web_search_ms,
        kb_alongside_external = kb_alongside_external,
        total_ms = outcome.sub_timings.total_ms,
        "retrieval: sub timing metrics"
    );
    outcome
}

/// Below this many characters of extracted text a page is not an article (a
/// cookie wall, a paywall stub or navigation), and the snippet says as much.
const MIN_USEFUL_PAGE_CHARS: usize = 400;

/// One web result's page, as much of it as the prompt had room for.
struct FetchedPage {
    text: String,
    word_count: usize,
    /// How many characters of the page the prompt carries, when that is not
    /// all of them. The rest is held in the turn's fetch memory.
    clipped_at: Option<usize>,
}

/// What came of trying to read one web result.
enum PageOutcome {
    Read(FetchedPage),
    /// Why it could not be read. Said in the prompt, so the model does not
    /// spend a round finding out for itself.
    Unreadable(String),
}

/// A failed or empty page is replaced by the next result down, so that one
/// blocked site does not cost the turn a page. Bounded, because every wave
/// waits on the network.
const MAX_PAGE_FETCH_WAVES: usize = 3;

/// The least of a page worth carrying when it cannot go whole: a few
/// screens of article text, enough to answer from. A page given this or more
/// and cut short says so, and the rest is read by offset.
const USEFUL_PAGE_SHARE_CHARS: usize = 8_000;

/// A useful share of a page within `budget_chars`: the whole budget when it is
/// smaller than the usual share, so a small window still reads one page.
fn useful_page_share(budget_chars: usize) -> usize {
    USEFUL_PAGE_SHARE_CHARS.min(budget_chars)
}

/// How many more pages a budget of `budget_chars` has a useful share for:
/// the pages opened next, in rank order. The pages read are then given room
/// by rank (see [`page_budget::by_rank`]), and a wave that leaves room opens
/// more; one that does not, none.
fn pages_the_budget_carries(budget_chars: usize) -> usize {
    if budget_chars < MIN_USEFUL_PAGE_CHARS {
        return 0;
    }
    (budget_chars / useful_page_share(budget_chars)).max(1)
}

struct ExternalLookup<'a> {
    container: &'a dyn ChatRuntime,
    /// The turn's stop button, resolved once from the runtime above.
    cancellation: &'a crate::features::conversation::chat::cancellation::TurnCancellationRegistry,
    conv_service: &'a Arc<dyn crate::features::conversation::ConversationServiceTrait>,
    conversation_id: &'a str,
    request_id: &'a str,
    validated_message: &'a str,
    utility_llm: &'a Arc<dyn lattice_core::application::ports::LLMPort>,
    search_flags: SearchFlags,
    highlight_terms: &'a [String],
    excerpt_chars: usize,
    tuning: &'a RetrievalTuningSettingsDto,
    /// Characters of fetched page text this turn's prompt has room for.
    page_budget_chars: usize,
    wiki_planned: bool,
    web_planned: bool,
    /// What this turn's attached files are about, when it has any. The web
    /// query is rewritten from the reader's words, and a message that only
    /// points at an attachment ("reference this chat I attached") carries no
    /// subject of its own.
    attachment_digest: Option<&'a str>,
    /// Every lookup below says what it is doing and what came of it.
    recorder: &'a TurnRecorder,
}

/// What the wiki/web phase produced, merged into the outcome by the caller.
struct ExternalPhaseOutcome {
    /// The utility model's reading of the turn. `None` when it was not
    /// requested or failed.
    interpretation: Option<HyDEInterpretation>,
    /// Wall-clock of query preparation, during which the interpretation and
    /// the web-query rewrite run concurrently.
    query_prep_ms: u64,
    wiki: Option<ExternalSearchResult>,
    web: Option<ExternalSearchResult>,
}

/// One external search, ready to merge into the outcome.
#[derive(Debug, Default)]
pub(super) struct ExternalSearchResult {
    pub(super) sources: Vec<SourceDto>,
    /// Prompt context, numbered only once the turn's sources are final.
    pub(super) context: Vec<WebContextItem>,
    /// Surfaced to the prompt. Web search only: wiki failures are just logged.
    pub(super) error: Option<String>,
    pub(super) elapsed_ms: u64,
    /// Every page this search tried to open, readable or not, for the tool
    /// loop to start from.
    pub(super) pages: crate::features::conversation::chat::fetch_memory::FetchMemory,
    /// The queries it ran: the one it was given, then a deep search's own.
    pub(super) queries: Vec<String>,
}

/// Merges wiki results first: citations are appended, and the wiki context is
/// used only while no web context exists.
pub(super) fn attach_wiki_results(
    outcome: &mut RetrievalPipelineOutcome,
    wiki: ExternalSearchResult,
) {
    outcome.sub_timings.wiki_search_ms = wiki.elapsed_ms;
    if !wiki.sources.is_empty() {
        let wiki_source_count = wiki.sources.len();
        outcome.sources.extend(wiki.sources);
        outcome.sources = deduplicate_sources(std::mem::take(&mut outcome.sources));
        info!(
            wiki_source_count = wiki_source_count,
            merged_source_count = outcome.sources.len(),
            "Forced wiki citations attached"
        );
    }
    if outcome.web_context.is_empty() {
        outcome.web_context = wiki.context;
    }
}

/// What the prompt says about one web result's page. It says outright whether
/// the text is the whole page, so the model neither fetches a page it has nor
/// retries one that failed.
fn render_page_body(page: Option<&PageOutcome>) -> String {
    match page {
        Some(PageOutcome::Read(page)) => match page.clipped_at {
            None => format!(
                "\nPage content ({} words — complete; this is the whole page and there is nothing more to fetch):\n{}",
                page.word_count, page.text
            ),
            Some(shown) => format!(
                "\nPage content ({} words; the first {} characters are shown, as much as this prompt has room for — call fetch_url_content with this URL and offset={} for the rest):\n{}",
                page.word_count, shown, shown, page.text
            ),
        },
        Some(PageOutcome::Unreadable(reason)) => format!(
            "\nPage could not be read ({reason}). Only the snippet above is available. Do not fetch this URL; it will fail the same way."
        ),
        None => String::new(),
    }
}

pub(super) fn attach_web_results(
    outcome: &mut RetrievalPipelineOutcome,
    web: ExternalSearchResult,
) {
    outcome.sub_timings.web_search_ms = web.elapsed_ms;
    outcome.pages_read.absorb(web.pages);
    outcome.web_queries.extend(web.queries);
    if web.error.is_some() {
        outcome.web_search_error = web.error;
    }
    if !web.sources.is_empty() {
        let web_source_count = web.sources.len();
        outcome.sources.extend(web.sources);
        outcome.sources = deduplicate_sources(std::mem::take(&mut outcome.sources));
        info!(
            web_source_count = web_source_count,
            merged_source_count = outcome.sources.len(),
            "Forced web search citations attached"
        );
    }
    outcome.web_context.extend(web.context);
}

#[cfg(test)]
mod page_rendering_tests;

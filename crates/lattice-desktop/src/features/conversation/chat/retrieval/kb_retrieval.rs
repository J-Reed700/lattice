use std::collections::HashSet;
use std::sync::Arc;
use std::time::Instant;

use tracing::{info, warn};

use crate::features::conversation::chat::ports::ChatRecords;
use crate::features::conversation::chat::ports::ChatRuntime;
use crate::features::conversation::chat::turn_record::TurnStepKind;
use crate::features::conversation::chat::ChatTurn;
use lattice_core::application::contracts::search::CorpusDocument;
use lattice_core::application::contracts::search::RerankOptions;
use lattice_core::application::contracts::search::{SearchResponseDto, SearchResultDto};
use lattice_core::application::contracts::settings::RetrievalTuningSettingsDto;
use lattice_core::domain::qa::hyde::QueryType;

use super::summary_tier::summary_tier;

pub(super) async fn run_kb_retrieval(
    container: &dyn ChatRuntime,
    turn: ChatTurn<'_>,
    search_flags: super::SearchFlags,
    // The turn's evidence budget in tokens, and the answering model's
    // characters per token: together, how deep the search reads.
    evidence_tokens: usize,
    chars_per_token: f64,
    enable_reranking: bool,
    _semantic_threshold: f32,
    tuning: &RetrievalTuningSettingsDto,
    // The turn's stop button, which the planner's model calls carry.
    cancel: &tokio_util::sync::CancellationToken,
) -> super::KbRetrievalOutcome {
    let ChatTurn {
        conv_service,
        conversation_id,
        message: validated_message,
        highlight_terms,
        focus,
        recorder,
        ..
    } = turn;
    let kb_start = Instant::now();
    let mut timings = super::RetrievalSubTimingMetrics::default();

    let scope_load_start = Instant::now();
    let scope = match super::load_space_document_scope(container, conversation_id).await {
        Some(scope) => scope,
        None => {
            timings.kb_scope_load_ms = super::elapsed_ms(scope_load_start);
            timings.kb_total_ms = super::elapsed_ms(kb_start);
            warn!(
                conversation_id = conversation_id,
                "Unable to resolve hard space scope; failing closed before retrieval"
            );
            return super::KbRetrievalOutcome {
                interpretation: lattice_core::domain::qa::hyde::HyDEInterpretation::raw_only(
                    validated_message.to_string(),
                    QueryType::Question,
                ),
                search_response: super::empty_search_response(),
                sources: Vec::new(),
                low_confidence: true,
                kb_unavailable_reason: Some(
                    "the conversation scope could not be resolved".to_string(),
                ),
                timings,
                searched_documents: 0,
                scope_is_linked: false,
                sufficiency: None,
            };
        }
    };
    // A chat pinned to particular documents searches those and no others. The
    // focus was already intersected with this same space scope, so this can
    // only ever remove ids; when it removes all of them the request named
    // nothing this chat can reach, and the turn searches nothing rather than
    // falling back to the whole space.
    let mut scope = scope;
    focus.confine(&mut scope.document_ids);
    timings.kb_scope_load_ms = super::elapsed_ms(scope_load_start);
    // The exact set the hard filter allows. Reported to the UI verbatim so
    // "Searched N documents" is a fact, not a corpus-sized guess.
    let searched_documents = scope.document_ids.len();
    let scope_is_linked = scope.space_id != super::DEFAULT_SPACE_ID;

    if focus.blocks_everything() {
        timings.kb_total_ms = super::elapsed_ms(kb_start);
        warn!(
            conversation_id = conversation_id,
            space_id = scope.space_id.as_str(),
            "Focus documents are outside this conversation's space; searching nothing"
        );
        return super::KbRetrievalOutcome {
            interpretation: lattice_core::domain::qa::hyde::HyDEInterpretation::raw_only(
                validated_message.to_string(),
                QueryType::Question,
            ),
            search_response: super::empty_search_response(),
            sources: Vec::new(),
            low_confidence: true,
            kb_unavailable_reason: Some(focus.unavailable_reason().to_string()),
            timings,
            searched_documents: 0,
            scope_is_linked,
            sufficiency: None,
        };
    }

    if scope.document_ids.is_empty() {
        timings.kb_total_ms = super::elapsed_ms(kb_start);
        info!(
            conversation_id = conversation_id,
            space_id = scope.space_id.as_str(),
            "Space scope contains no indexed documents; skipping KB retrieval"
        );
        return super::KbRetrievalOutcome {
            interpretation: lattice_core::domain::qa::hyde::HyDEInterpretation::raw_only(
                validated_message.to_string(),
                QueryType::Question,
            ),
            search_response: super::empty_search_response(),
            sources: Vec::new(),
            low_confidence: true,
            kb_unavailable_reason: Some(if scope.space_id == super::DEFAULT_SPACE_ID {
                "the vault has no indexed documents yet".to_string()
            } else {
                "no indexed documents are assigned to this conversation’s space".to_string()
            }),
            timings,
            searched_documents: 0,
            scope_is_linked: scope.space_id != super::DEFAULT_SPACE_ID,
            sufficiency: None,
        };
    }

    let repository = container.chat_records();
    // How deep to read: as many passages as the evidence budget can carry, at
    // what a passage of this library actually measures. The prompt's
    // best-first fill decides what goes in; this only stops the search from
    // handing it less than the budget could hold.
    let passage_chars = match repository
        .retrieval_mean_chunk_chars(&scope.document_ids)
        .await
    {
        Ok(measured) => measured,
        Err(error) => {
            warn!(%error, "Could not measure passage length; assuming the chunker's target");
            None
        }
    };
    let kb_search_limit = super::retrieval_depth(evidence_tokens, passage_chars, chars_per_token);
    let summaries = summary_tier(container, validated_message, &scope).await;
    let planning_start = Instant::now();
    let plan_step = recorder.begin_guarded(
        TurnStepKind::Plan,
        if focus.narrows() {
            "Planning a search of the documents you named"
        } else {
            "Planning what to search for"
        },
        None,
    );
    // One turn's use of the planner: the shortlist search runs once however
    // many times the turn plans.
    let planner_health = container.planner_health();
    let mut planner_turn = super::corpus_plan::PlannerTurn::with_health(&planner_health);
    let (mut plan, catalog) = match repository.retrieval_catalog(&scope.document_ids).await {
        Ok(catalog) => {
            let plan = if let Some(plan) =
                super::corpus_plan::ordered_learning_plan(validated_message, &catalog)
            {
                plan
            } else if let Some((plan, anchors)) =
                planner_skip_plan(conv_service, conversation_id, validated_message, &catalog).await
            {
                timings.kb_planner_skipped = true;
                info!(
                    conversation_id = conversation_id,
                    shared_anchor_terms = ?anchors,
                    queries = ?plan.queries,
                    "retrieval: planner skipped — short message continues the previous turn"
                );
                plan
            } else if let Ok(Some(utility)) = (container.utility_llm_loader())().await {
                // The planner runs only when a utility model is assigned.
                planned(
                    container,
                    conv_service,
                    conversation_id,
                    validated_message,
                    utility.as_ref(),
                    &catalog,
                    &scope,
                    &summaries.summaries,
                    &mut planner_turn,
                    cancel,
                )
                .await
            } else {
                info!("Utility planner unavailable; using immediate local retrieval plan");
                super::corpus_plan::fallback_with_catalog(validated_message, &catalog)
            };
            (plan, catalog)
        }
        Err(error) => {
            warn!(%error, "Could not load retrieval catalog; searching request directly");
            (
                super::corpus_plan::CorpusSearchPlan::fallback(validated_message),
                Vec::new(),
            )
        }
    };
    summaries.apply(&mut plan, validated_message, &catalog);
    timings.kb_hyde_interpretation_ms = super::elapsed_ms(planning_start);
    // The queries themselves: the one thing a reader can judge a search by.
    plan_step.done(Some(plan.queries.join(" · ")));
    info!(queries = ?plan.queries, opening_documents = ?plan.opening_document_ids, "Corpus retrieval plan");
    let interpretation = lattice_core::domain::qa::hyde::HyDEInterpretation::raw_only(
        validated_message.to_string(),
        if search_flags.force_followup_mode {
            QueryType::Followup
        } else {
            QueryType::Question
        },
    );
    let search_start = Instant::now();
    let search_step = recorder.begin_guarded(
        TurnStepKind::SearchDocuments,
        if focus.narrows() {
            "Searching the documents you named"
        } else {
            "Searching your documents"
        },
        Some(format!(
            "{searched_documents} {}",
            if searched_documents == 1 {
                "document"
            } else {
                "documents"
            }
        )),
    );
    // The shortlist a reranker gets to see. The corrective pass reuses it so
    // both passes contribute the same number of candidates to the fusion.
    let candidate_limit = candidate_limit(enable_reranking, kb_search_limit);
    let response = super::corpus_plan::retrieve(
        repository.as_ref(),
        container.library_search().as_ref(),
        validated_message,
        &plan,
        &scope,
        candidate_limit,
    )
    .await;
    timings.kb_search_plan_ms = super::elapsed_ms(search_start);
    timings.kb_query_execution_ms = timings.kb_search_plan_ms;
    let (search_response, kb_unavailable_reason) = match response {
        Ok(response) => {
            search_step.done(Some(passage_count_line(&response.results)));
            (response, None)
        }
        Err(error) => {
            warn!(%error, "Document retrieval failed");
            search_step.failed(Some(error.to_string()));
            (
                super::empty_search_response(),
                Some(unavailable_reason(&error.to_string())),
            )
        }
    };
    let (mut search_response, mut reranked) = rerank_pass(
        container,
        enable_reranking,
        &plan.queries.join(" "),
        &interpretation,
        search_response,
        kb_search_limit,
        tuning,
        &mut timings,
    )
    .await;

    // Corrective retrieval. One local verdict, and at most one replanned retry
    // when it fails — a loop that could run twice is a loop that will run twice
    // on the turn a user is already waiting on.
    let sufficiency_start = Instant::now();
    let mut verdict = super::assess_sufficiency(
        &search_response.results,
        &plan,
        tuning,
        reranked,
        kb_search_limit,
    );
    timings.kb_sufficiency_ms = super::elapsed_ms(sufficiency_start);
    recorder.note(
        TurnStepKind::Sufficiency,
        "Judging whether that is enough",
        None,
        Some(sufficiency_line(&verdict)),
    );
    info!(
        conversation_id = conversation_id,
        sufficient = verdict.sufficient,
        top_score = verdict.top_score,
        spread = verdict.spread,
        term_coverage = verdict.term_coverage,
        reranked = reranked,
        result_count = search_response.results.len(),
        reasons = ?verdict.reasons,
        "retrieval: sufficiency verdict"
    );
    // A first pass that errored outright failed in the search engine, not in the
    // query. Replanning the words would not reach a vector index that is down.
    let can_retry =
        tuning.sufficiency_retry_enabled && kb_unavailable_reason.is_none() && !catalog.is_empty();
    if !verdict.sufficient && can_retry {
        let retry_start = Instant::now();
        let corrective_step = recorder.begin_guarded(
            TurnStepKind::CorrectiveSearch,
            "Searching again with a different plan",
            None,
        );
        let correction = super::corpus_plan::CorrectionRequest {
            queries_already_tried: plan.queries.clone(),
            top_result_titles: top_result_titles(&search_response.results),
            why_insufficient: verdict.reasons.clone(),
            more_titles_not_shown: 0,
        };
        let retry = corrective_pass(
            container,
            turn,
            repository.as_ref(),
            &catalog,
            &scope,
            &correction,
            candidate_limit,
            &mut planner_turn,
            cancel,
        )
        .await;
        if let Some((second_pass, retry_queries)) = retry {
            let first_count = search_response.results.len();
            let second_count = second_pass.results.len();
            let first_results = std::mem::take(&mut search_response.results);
            search_response.results = super::corpus_plan::fuse_passes(
                container.library_search().as_ref(),
                first_results,
                second_pass.results,
                candidate_limit,
            );
            search_response.total = search_response.results.len();
            (search_response, reranked) = rerank_pass(
                container,
                enable_reranking,
                &[plan.queries.clone(), retry_queries.clone()]
                    .concat()
                    .join(" "),
                &interpretation,
                search_response,
                kb_search_limit,
                tuning,
                &mut timings,
            )
            .await;
            // Judged against the original plan: the retry chose different
            // wording, but what the turn owed the user has not changed.
            verdict = super::assess_sufficiency(
                &search_response.results,
                &plan,
                tuning,
                reranked,
                kb_search_limit,
            );
            timings.kb_corrective_retries = 1;
            info!(
                conversation_id = conversation_id,
                first_pass_queries = ?plan.queries,
                retry_queries = ?retry_queries,
                first_pass_results = first_count,
                retry_results = second_count,
                fused_results = search_response.results.len(),
                now_sufficient = verdict.sufficient,
                top_score = verdict.top_score,
                spread = verdict.spread,
                term_coverage = verdict.term_coverage,
                reasons = ?verdict.reasons,
                "retrieval: corrective pass"
            );
            corrective_step.done(Some(format!(
                "{} · {}",
                passage_count_line(&search_response.results),
                sufficiency_line(&verdict)
            )));
        } else {
            // No planner, or a plan that only repeated what had already failed.
            // The first pass's evidence stands, which is not a failure of the
            // corrective step so much as its honest outcome.
            corrective_step.done(Some("no better plan to try".to_string()));
        }
        timings.kb_corrective_retry_ms = super::elapsed_ms(retry_start);
    }
    timings.kb_sufficient = Some(verdict.sufficient);
    // The first `kb_search_limit` passages are the hits, in rank order. Every
    // hit's section neighbours follow them as context, which the prompt's
    // evidence budget carries only in the room the hits leave: a neighbour
    // never displaces a hit.
    if let Err(error) = super::corpus_plan::append_context(
        repository.as_ref(),
        &mut search_response.results,
        &scope.document_ids,
        kb_search_limit,
    )
    .await
    {
        warn!(%error, "Could not read the hits' section context");
    }
    search_response.total = search_response.results.len();
    // RRF scores are ranks, not probabilities. A cosine cutoff must never be
    // applied to them or used to claim the documents are unavailable.
    let low_confidence = search_response.results.is_empty();

    let build_sources_start = Instant::now();
    let sources = {
        let raw =
            super::build_source_citations(&search_response.results, container, highlight_terms)
                .await;
        super::deduplicate_sources(raw)
    };
    timings.kb_build_sources_ms = super::elapsed_ms(build_sources_start);

    if !search_response.results.is_empty() {
        let persist_refs_start = Instant::now();
        if let Err(e) = super::persist_document_references(
            conv_service,
            conversation_id,
            &search_response.results,
        )
        .await
        {
            warn!(error = %e, "Failed to persist conversation document references");
        }
        timings.kb_persist_references_ms = super::elapsed_ms(persist_refs_start);
    }
    timings.kb_total_ms = super::elapsed_ms(kb_start);

    super::KbRetrievalOutcome {
        interpretation,
        search_response,
        sources,
        low_confidence,
        kb_unavailable_reason,
        timings,
        searched_documents,
        scope_is_linked,
        sufficiency: Some(verdict),
    }
}

/// How many candidates a search pass gathers before the cut to
/// `kb_search_limit`: the reranker's wider shortlist whenever one will judge
/// them, so it can promote a passage fusion left just outside the cut.
fn candidate_limit(enable_reranking: bool, kb_search_limit: usize) -> usize {
    if enable_reranking {
        RerankOptions::candidate_pool(kb_search_limit)
    } else {
        kb_search_limit
    }
}

/// Rerank a pass's pool when reranking is on, and whether the cross-encoder's
/// scores were applied.
///
/// Every plan is reranked, ordered reading included. Reading order is one
/// evidence source in the pool, never the whole of it: `finalize_rerank`
/// keeps the leading openings where the plan put them, so a request to read
/// a document still reads it first, and the passages searched for behind
/// them compete as they do on any other turn.
async fn rerank_pass(
    container: &dyn ChatRuntime,
    enable_reranking: bool,
    query: &str,
    interpretation: &lattice_core::domain::qa::hyde::HyDEInterpretation,
    mut response: SearchResponseDto,
    evidence_limit: usize,
    tuning: &RetrievalTuningSettingsDto,
    timings: &mut super::RetrievalSubTimingMetrics,
) -> (SearchResponseDto, bool) {
    if !enable_reranking {
        return (response, false);
    }
    bound_leading_structural(&mut response.results, evidence_limit);
    let rerank_start = Instant::now();
    let reranked =
        super::apply_rerank_stage(container, query, interpretation, response, None, tuning).await;
    timings.kb_rerank_ms += super::elapsed_ms(rerank_start);
    reranked
}

/// Ordered reading earns at most half of the passages the turn keeps. A pool
/// laid out for the reranker's wider shortlist can lead with more section
/// lookups and openings than that, and `finalize_rerank` pins the whole
/// leading run, so the cut to `evidence_limit` would keep nothing searched
/// for. Past the half they lose their pinned place and compete on relevance.
fn bound_leading_structural(results: &mut Vec<SearchResultDto>, evidence_limit: usize) {
    let leading = results
        .iter()
        .take_while(|result| super::rerank::is_structural_result(result))
        .count();
    let pinned = evidence_limit.div_ceil(2);
    if leading > pinned {
        let unpinned: Vec<_> = results.drain(pinned..leading).collect();
        results.extend(unpinned);
    }
}

/// What a search pass came back with, counted the way the trace counts it:
/// passages, and the documents they came from.
/// The retrieval trace carries at most this many characters of the search
/// failure; the frontend schema caps `unavailableReason` at the same length.
const UNAVAILABLE_REASON_MAX_CHARS: usize = 400;

/// Clip a search failure for the retrieval trace. A longer reason would fail
/// the frontend's trace schema and drop the "search unavailable" notice on
/// exactly the turn that needs it.
fn unavailable_reason(error: &str) -> String {
    lattice_core::shared::text::safe_truncate(error, UNAVAILABLE_REASON_MAX_CHARS)
}

fn passage_count_line(results: &[SearchResultDto]) -> String {
    if results.is_empty() {
        return "nothing".to_string();
    }
    let files = results
        .iter()
        .filter_map(|result| result.document_id.as_deref())
        .collect::<HashSet<_>>()
        .len()
        .max(1);
    format!(
        "{} {} from {} {}",
        results.len(),
        if results.len() == 1 {
            "passage"
        } else {
            "passages"
        },
        files,
        if files == 1 { "file" } else { "files" }
    )
}

/// The sufficiency verdict in one line.
///
/// A plain sufficient verdict is the expected case and says so in three words;
/// an insufficient one carries its reasons, which are the only thing that makes
/// the verdict checkable. In words: this line is persisted and read by people.
fn sufficiency_line(verdict: &super::SufficiencyVerdict) -> String {
    if verdict.sufficient {
        return "enough support".to_string();
    }
    if verdict.reasons.is_empty() {
        return "not enough support".to_string();
    }
    let reasons: Vec<&str> = verdict
        .reasons
        .iter()
        .map(|code| super::sufficiency_reason::readable(code))
        .collect();
    format!("not enough support: {}", reasons.join(", "))
}

/// Titles the first pass surfaced, deduplicated and best first, so the
/// corrective planner can see what it already reached and steer somewhere
/// else. All of them: the planner's prompt shows what its room holds and
/// counts the rest.
fn top_result_titles(results: &[SearchResultDto]) -> Vec<String> {
    let mut seen = HashSet::new();
    results
        .iter()
        .filter(|result| seen.insert(result.title.as_str()))
        .map(|result| result.title.clone())
        .collect()
}

/// The planner's plan for the first pass, or the local plan when the planner
/// should not be asked or does not answer.
async fn planned(
    container: &dyn ChatRuntime,
    conv_service: &Arc<dyn crate::features::conversation::ConversationServiceTrait>,
    conversation_id: &str,
    validated_message: &str,
    utility: &dyn lattice_core::application::ports::LLMPort,
    catalog: &[CorpusDocument],
    scope: &super::SpaceDocumentScope,
    summaries: &std::collections::HashMap<String, String>,
    planner_turn: &mut super::corpus_plan::PlannerTurn<'_>,
    cancel: &tokio_util::sync::CancellationToken,
) -> super::corpus_plan::CorpusSearchPlan {
    let history = super::build_hyde_context_window_for_conversation(
        conv_service,
        conversation_id,
        validated_message,
        utility,
        Some(planner_turn.history_tokens(utility)),
    )
    .await;
    super::corpus_plan::shortlist_candidates(
        container.library_search().as_ref(),
        utility,
        validated_message,
        history.as_deref(),
        catalog,
        summaries,
        &scope.document_ids,
        planner_turn,
    )
    .await;
    super::corpus_plan::plan(
        utility,
        validated_message,
        history.as_deref(),
        catalog,
        summaries,
        planner_turn,
        cancel,
    )
    .await
    .unwrap_or_else(|error| {
        warn!(%error, "Corpus planning failed; using local retrieval plan");
        super::corpus_plan::fallback_with_catalog(validated_message, catalog)
    })
}

/// The one replanned retry. Returns `None` whenever it cannot improve on the
/// first pass — no planner, a plan that only repeats failed queries, or a
/// failed search — so the caller keeps the evidence it already has.
async fn corrective_pass(
    container: &dyn ChatRuntime,
    turn: ChatTurn<'_>,
    repository: &dyn ChatRecords,
    catalog: &[CorpusDocument],
    scope: &super::SpaceDocumentScope,
    correction: &super::corpus_plan::CorrectionRequest,
    limit: usize,
    planner_turn: &mut super::corpus_plan::PlannerTurn<'_>,
    cancel: &tokio_util::sync::CancellationToken,
) -> Option<(SearchResponseDto, Vec<String>)> {
    let validated_message = turn.message;
    let Ok(Some(utility)) = (container.utility_llm_loader())().await else {
        info!("Corrective retrieval skipped: no utility planner is available");
        return None;
    };
    let history = super::build_hyde_context_window_for_conversation(
        turn.conv_service,
        turn.conversation_id,
        validated_message,
        utility.as_ref(),
        Some(planner_turn.history_tokens(utility.as_ref())),
    )
    .await;
    // The correction's prompt carries no summaries.
    super::corpus_plan::shortlist_candidates(
        container.library_search().as_ref(),
        utility.as_ref(),
        validated_message,
        history.as_deref(),
        catalog,
        &std::collections::HashMap::new(),
        &scope.document_ids,
        planner_turn,
    )
    .await;
    let retry_plan = match super::corpus_plan::plan_correction(
        utility.as_ref(),
        validated_message,
        history.as_deref(),
        catalog,
        correction,
        planner_turn,
        cancel,
    )
    .await
    {
        Ok(plan) => plan,
        Err(error) => {
            warn!(%error, "Corrective retrieval planning failed; keeping first-pass evidence");
            return None;
        }
    };
    match super::corpus_plan::retrieve(
        repository,
        container.library_search().as_ref(),
        validated_message,
        &retry_plan,
        scope,
        limit,
    )
    .await
    {
        Ok(response) => Some((response, retry_plan.queries)),
        Err(error) => {
            warn!(%error, "Corrective retrieval search failed; keeping first-pass evidence");
            None
        }
    }
}

/// The plan a skipped planner would have produced, when the message is short
/// and still on the previous turn's topic.
async fn planner_skip_plan(
    conv_service: &Arc<dyn crate::features::conversation::ConversationServiceTrait>,
    conversation_id: &str,
    validated_message: &str,
    catalog: &[CorpusDocument],
) -> Option<(super::corpus_plan::CorpusSearchPlan, Vec<String>)> {
    // Cheap test first: the anchor terms cost a conversation read, and a long
    // message can never qualify.
    if !super::corpus_plan::is_short_followup_message(validated_message) {
        return None;
    }
    let anchors = super::load_followup_turn_anchor_terms(conv_service, conversation_id).await;
    super::corpus_plan::followup_plan(validated_message, &anchors, catalog)
}

#[cfg(test)]
#[path = "kb_retrieval_tests.rs"]
mod tests;

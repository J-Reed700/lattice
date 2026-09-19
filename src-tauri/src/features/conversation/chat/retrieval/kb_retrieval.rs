use std::collections::HashSet;
use std::sync::Arc;
use std::time::Instant;

use tracing::{info, warn};

use crate::application::contracts::search::CorpusDocument;
use crate::domain::qa::hyde::QueryType;
use crate::features::conversation::chat::focus::FocusScope;
use crate::features::conversation::chat::turn_record::{TurnRecorder, TurnStepKind};
use crate::features::conversation::repository::ConversationRepository;
use crate::features::search::dto::{SearchResponseDto, SearchResultDto};
use crate::features::settings::dto::RetrievalTuningSettingsDto;
use crate::interfaces::di::Container;

// Retrieval policy inputs stay explicit so call sites cannot silently inherit defaults.
#[allow(clippy::too_many_arguments)]
pub(super) async fn run_kb_retrieval(
    container: &Container,
    conv_service: &Arc<dyn crate::features::conversation::ConversationServiceTrait>,
    conversation_id: &str,
    validated_message: &str,
    _llm: &Arc<dyn crate::application::ports::LLMPort>,
    search_flags: super::SearchFlags,
    highlight_terms: &[String],
    kb_search_limit: usize,
    enable_reranking: bool,
    _semantic_threshold: f32,
    tuning: &RetrievalTuningSettingsDto,
    focus: &FocusScope,
    recorder: &TurnRecorder,
) -> super::KbRetrievalOutcome {
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
                interpretation: crate::domain::qa::hyde::HyDEInterpretation::raw_only(
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
            interpretation: crate::domain::qa::hyde::HyDEInterpretation::raw_only(
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
            interpretation: crate::domain::qa::hyde::HyDEInterpretation::raw_only(
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

    let repository = ConversationRepository::new(container.db_pool().clone());
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
            } else if let Ok(Some(utility)) = container.get_or_load_utility_llm().await {
                let history = super::build_hyde_context_window_for_conversation(
                    conv_service,
                    conversation_id,
                )
                .await;
                super::corpus_plan::plan(
                    utility.as_ref(),
                    validated_message,
                    history.as_deref(),
                    &catalog,
                    &summaries.summaries,
                )
                .await
                .unwrap_or_else(|error| {
                    warn!(%error, "Corpus planning failed; using local retrieval plan");
                    super::corpus_plan::fallback_with_catalog(validated_message, &catalog)
                })
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
    let interpretation = crate::domain::qa::hyde::HyDEInterpretation::raw_only(
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
    let candidate_limit = if enable_reranking && !plan.start_at_beginning {
        kb_search_limit.saturating_mul(3).min(64)
    } else {
        kb_search_limit
    };
    let response = super::corpus_plan::retrieve(
        &repository,
        container.semantic_search_use_case().as_ref(),
        container.hybrid_search_use_case().as_ref(),
        validated_message,
        &plan,
        &scope,
        candidate_limit,
    )
    .await;
    timings.kb_search_plan_ms = super::elapsed_ms(search_start);
    timings.kb_query_execution_ms = timings.kb_search_plan_ms;
    let (mut search_response, kb_unavailable_reason) = match response {
        Ok(response) => {
            search_step.done(Some(passage_count_line(&response.results)));
            (response, None)
        }
        Err(error) => {
            warn!(%error, "Document retrieval failed");
            search_step.failed(Some(error.to_string()));
            (super::empty_search_response(), Some(error.to_string()))
        }
    };
    // Rerank focused searches only. Ordered document reading is intentional
    // evidence selection, not a cosine-similarity contest.
    let mut reranked = false;
    if enable_reranking && plan.opening_document_ids.is_empty() {
        let rerank_start = Instant::now();
        let (response, applied) = super::apply_rerank_stage(
            container,
            &plan.queries.join(" "),
            &interpretation,
            search_response,
            None,
            tuning,
        )
        .await;
        search_response = response;
        reranked = applied;
        timings.kb_rerank_ms = super::elapsed_ms(rerank_start);
    }

    // Corrective retrieval. One local verdict, and at most one replanned retry
    // when it fails — a loop that could run twice is a loop that will run twice
    // on the turn a user is already waiting on.
    let sufficiency_start = Instant::now();
    let mut verdict = super::assess_sufficiency(&search_response.results, &plan, tuning, reranked);
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
        };
        let retry = corrective_pass(
            container,
            conv_service,
            conversation_id,
            validated_message,
            &repository,
            &catalog,
            &scope,
            &correction,
            candidate_limit,
        )
        .await;
        if let Some((second_pass, retry_queries)) = retry {
            let first_count = search_response.results.len();
            let second_count = second_pass.results.len();
            let first_results = std::mem::take(&mut search_response.results);
            search_response.results = super::corpus_plan::fuse_passes(
                first_results,
                second_pass.results,
                candidate_limit,
            );
            search_response.total = search_response.results.len();
            if enable_reranking && plan.opening_document_ids.is_empty() {
                let rerank_start = Instant::now();
                let (response, applied) = super::apply_rerank_stage(
                    container,
                    &[plan.queries.clone(), retry_queries.clone()]
                        .concat()
                        .join(" "),
                    &interpretation,
                    search_response,
                    None,
                    tuning,
                )
                .await;
                search_response = response;
                reranked = applied;
                timings.kb_rerank_ms += super::elapsed_ms(rerank_start);
            }
            // Judged against the original plan: the retry chose different
            // wording, but what the turn owed the user has not changed.
            verdict = super::assess_sufficiency(&search_response.results, &plan, tuning, reranked);
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
    search_response
        .results
        .truncate(kb_search_limit.max(if plan.start_at_beginning { 16 } else { 1 }));
    if let Err(error) = super::corpus_plan::expand_evidence(
        &repository,
        &mut search_response.results,
        &scope.document_ids,
        kb_search_limit + 8,
    )
    .await
    {
        warn!(%error, "Could not expand neighboring section evidence");
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

/// What a search pass came back with, counted the way the trace counts it:
/// passages, and the documents they came from.
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

/// Titles the first pass surfaced, deduplicated, so the corrective planner can
/// see what it already reached and steer somewhere else.
fn top_result_titles(results: &[SearchResultDto]) -> Vec<String> {
    let mut seen = HashSet::new();
    results
        .iter()
        .filter(|result| seen.insert(result.title.as_str()))
        .take(5)
        .map(|result| result.title.clone())
        .collect()
}

/// The one replanned retry. Returns `None` whenever it cannot improve on the
/// first pass — no planner, a plan that only repeats failed queries, or a
/// failed search — so the caller keeps the evidence it already has.
#[allow(clippy::too_many_arguments)]
async fn corrective_pass(
    container: &Container,
    conv_service: &Arc<dyn crate::features::conversation::ConversationServiceTrait>,
    conversation_id: &str,
    validated_message: &str,
    repository: &ConversationRepository,
    catalog: &[CorpusDocument],
    scope: &super::SpaceDocumentScope,
    correction: &super::corpus_plan::CorrectionRequest,
    limit: usize,
) -> Option<(SearchResponseDto, Vec<String>)> {
    let Ok(Some(utility)) = container.get_or_load_utility_llm().await else {
        info!("Corrective retrieval skipped: no utility planner is available");
        return None;
    };
    let history =
        super::build_hyde_context_window_for_conversation(conv_service, conversation_id).await;
    let retry_plan = match super::corpus_plan::plan_correction(
        utility.as_ref(),
        validated_message,
        history.as_deref(),
        catalog,
        correction,
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
        container.semantic_search_use_case().as_ref(),
        container.hybrid_search_use_case().as_ref(),
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

/// How many documents the summary tier may nominate to open with. Matches the
/// planner's own `opening_document_ids` cap, which the plan validator enforces.
const SUMMARY_OPENING_DOCUMENTS: usize = 4;

/// The collection-level summary tier for one turn.
///
/// Empty — and free — whenever summaries are off, which is the default:
/// `Container::summary_search` returns `None` after one indexed `COUNT` and
/// nothing else runs. Everything here is additive; a failure anywhere leaves
/// the turn with exactly the retrieval it would have had without the tier.
#[derive(Default)]
struct SummaryTier {
    /// Document-level summary text keyed by document id, for the planner
    /// catalog.
    summaries: std::collections::HashMap<String, String>,
    /// Documents the tier would open with, best first.
    openings: Vec<String>,
}

impl SummaryTier {
    /// Hand the tier's opening documents to a finished plan.
    fn apply(
        &self,
        plan: &mut super::corpus_plan::CorpusSearchPlan,
        message: &str,
        catalog: &[CorpusDocument],
    ) {
        if super::corpus_plan::apply_summary_openings(plan, message, &self.openings, catalog) {
            info!(
                opening_documents = ?plan.opening_document_ids,
                "retrieval: opening documents chosen by summary similarity"
            );
        }
    }
}

/// Load the summary tier for this turn.
async fn summary_tier(
    container: &Container,
    message: &str,
    scope: &super::SpaceDocumentScope,
) -> SummaryTier {
    use crate::features::summaries::search::SummarySearchPort;

    let Some(search) = container.summary_search().await else {
        return SummaryTier::default();
    };
    let summaries = match search.document_summaries(&scope.document_ids).await {
        Ok(summaries) => summaries,
        Err(error) => {
            warn!(%error, "Could not read document summaries; planning without them");
            std::collections::HashMap::new()
        }
    };
    // Only pay for a summary search when the request is about documents as
    // wholes; a focused question keeps the planner's own opening selection.
    if !super::corpus_plan::whole_document_intent(message) {
        return SummaryTier {
            summaries,
            openings: Vec::new(),
        };
    }
    let openings = match search
        .top_summaries(message, &scope.document_ids, SUMMARY_OPENING_DOCUMENTS)
        .await
    {
        Ok(hits) => crate::features::summaries::openings::select_opening_documents(
            &hits,
            &scope.document_ids,
            SUMMARY_OPENING_DOCUMENTS,
        ),
        Err(error) => {
            warn!(%error, "Summary search failed; keeping the planner's opening documents");
            Vec::new()
        }
    };
    SummaryTier {
        summaries,
        openings,
    }
}

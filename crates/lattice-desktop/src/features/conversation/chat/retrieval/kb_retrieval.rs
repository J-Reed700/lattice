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

pub(super) async fn run_kb_retrieval(
    container: &dyn ChatRuntime,
    turn: ChatTurn<'_>,
    search_flags: super::SearchFlags,
    evidence_tokens: usize,
    chars_per_token: f64,
    enable_reranking: bool,
    _semantic_threshold: f32,
    tuning: &RetrievalTuningSettingsDto,
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
    let empty_outcome = |reason: Option<String>, linked: bool, timings: super::RetrievalSubTimingMetrics| super::KbRetrievalOutcome {
        interpretation: lattice_core::domain::qa::hyde::HyDEInterpretation::raw_only(
            validated_message.to_string(),
            QueryType::Question,
        ),
        search_response: super::empty_search_response(),
        sources: Vec::new(),
        low_confidence: true,
        kb_unavailable_reason: reason,
        timings,
        searched_documents: 0,
        scope_is_linked: linked,
        sufficiency: None,
    };
    let scope = match super::load_space_document_scope(container, conversation_id).await {
        Some(scope) => scope,
        None => {
            timings.kb_scope_load_ms = super::elapsed_ms(scope_load_start);
            timings.kb_total_ms = super::elapsed_ms(kb_start);
            warn!(conversation_id = conversation_id, "Unable to resolve hard space scope; failing closed before retrieval");
            return empty_outcome(Some("the conversation scope could not be resolved".to_string()), false, timings);
        }
    };
    let mut scope = scope;
    focus.confine(&mut scope.document_ids);
    timings.kb_scope_load_ms = super::elapsed_ms(scope_load_start);
    let searched_documents = scope.document_ids.len();
    let scope_is_linked = scope.space_id != super::DEFAULT_SPACE_ID;

    if focus.blocks_everything() {
        timings.kb_total_ms = super::elapsed_ms(kb_start);
        warn!(conversation_id = conversation_id, space_id = scope.space_id.as_str(), "Focus documents are outside this conversation's space; searching nothing");
        return empty_outcome(Some(focus.unavailable_reason().to_string()), scope_is_linked, timings);
    }

    if scope.document_ids.is_empty() {
        timings.kb_total_ms = super::elapsed_ms(kb_start);
        info!(conversation_id = conversation_id, space_id = scope.space_id.as_str(), "Space scope contains no indexed documents; skipping KB retrieval");
        let reason = if scope.space_id == super::DEFAULT_SPACE_ID {
            "the vault has no indexed documents yet".to_string()
        } else {
            "no indexed documents are assigned to this conversation’s space".to_string()
        };
        return empty_outcome(Some(reason), scope_is_linked, timings);
    }

    let repository = container.chat_records();
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
    let candidate_limit = if enable_reranking && !plan.start_at_beginning {
        RerankOptions::candidate_pool(kb_search_limit)
    } else {
        kb_search_limit
    };
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
    let (mut search_response, kb_unavailable_reason) = match response {
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
    let mut reranked = false;
    if enable_reranking && !plan.start_at_beginning {
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
    if !plan.start_at_beginning {
        promote_entity_bridge(&mut search_response.results);
    }

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
            if enable_reranking && !plan.start_at_beginning {
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
            if !plan.start_at_beginning {
                promote_entity_bridge(&mut search_response.results);
            }
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
            corrective_step.done(Some("no better plan to try".to_string()));
        }
        timings.kb_corrective_retry_ms = super::elapsed_ms(retry_start);
    }
    timings.kb_sufficient = Some(verdict.sufficient);
    search_response.results.truncate(kb_search_limit);
    if let Err(error) = super::corpus_plan::expand_evidence(
        repository.as_ref(),
        &mut search_response.results,
        &scope.document_ids,
        kb_search_limit + 8,
    )
    .await
    {
        warn!(%error, "Could not expand neighboring section evidence");
    }
    search_response.total = search_response.results.len();
    // RRF scores are ranks, not probabilities.
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

const UNAVAILABLE_REASON_MAX_CHARS: usize = 400;

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

fn top_result_titles(results: &[SearchResultDto]) -> Vec<String> {
    let mut seen = HashSet::new();
    results
        .iter()
        .filter(|result| seen.insert(result.title.as_str()))
        .map(|result| result.title.clone())
        .collect()
}

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
    shortlist_candidates(
        container,
        validated_message,
        history.as_deref(),
        catalog,
        scope,
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

async fn shortlist_candidates(
    container: &dyn ChatRuntime,
    validated_message: &str,
    history: Option<&str>,
    catalog: &[CorpusDocument],
    scope: &super::SpaceDocumentScope,
    planner_turn: &mut super::corpus_plan::PlannerTurn<'_>,
) {
    if planner_turn.candidates.is_some() {
        return;
    }
    planner_turn.candidates = Some(
        super::corpus_plan::candidate_documents(
            container.library_search().as_ref(),
            validated_message,
            history,
            catalog,
            &scope.document_ids,
        )
        .await,
    );
}

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
    shortlist_candidates(
        container,
        validated_message,
        history.as_deref(),
        catalog,
        scope,
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

async fn planner_skip_plan(
    conv_service: &Arc<dyn crate::features::conversation::ConversationServiceTrait>,
    conversation_id: &str,
    validated_message: &str,
    catalog: &[CorpusDocument],
) -> Option<(super::corpus_plan::CorpusSearchPlan, Vec<String>)> {
    if !super::corpus_plan::is_short_followup_message(validated_message) {
        return None;
    }
    let anchors = super::load_followup_turn_anchor_terms(conv_service, conversation_id).await;
    super::corpus_plan::followup_plan(validated_message, &anchors, catalog)
}

const SUMMARY_OPENING_DOCUMENTS: usize = 4;

#[derive(Default)]
struct SummaryTier {
    summaries: std::collections::HashMap<String, String>,
    openings: Vec<String>,
}

impl SummaryTier {
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

async fn summary_tier(
    container: &dyn ChatRuntime,
    message: &str,
    scope: &super::SpaceDocumentScope,
) -> SummaryTier {
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
        Ok(hits) => lattice_core::application::ports::summary_search::select_opening_documents(
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

fn promote_entity_bridge(
    results: &mut Vec<lattice_core::application::contracts::search::SearchResultDto>,
) {
    let Some(bridge) = results.iter().find(|result| {
        result
            .metadata
            .get("retrievalMethod")
            .and_then(|value| value.as_str())
            == Some("entity_bridge")
    }) else {
        return;
    };
    let Some(source_result_id) = bridge
        .metadata
        .get("bridgeSourceResultId")
        .and_then(|value| value.as_str())
        .map(str::to_owned)
    else {
        return;
    };
    let Some(target_document_id) = bridge.document_id.clone() else {
        return;
    };
    let Some(source_position) = results
        .iter()
        .position(|result| result.id == source_result_id)
    else {
        return;
    };
    if source_position > 0 {
        let source = results.remove(source_position);
        results.insert(0, source);
    }

    let mut linked = Vec::new();
    let mut index = 1;
    while index < results.len() {
        if results[index].document_id.as_deref() == Some(target_document_id.as_str()) {
            linked.push(results.remove(index));
        } else {
            index += 1;
        }
    }
    linked.sort_by_key(|result| {
        result
            .position
            .or_else(|| {
                result
                    .metadata
                    .get("chunk_index")
                    .and_then(serde_json::Value::as_u64)
                    .and_then(|index| usize::try_from(index).ok())
            })
            .unwrap_or(usize::MAX)
    });
    let remainder = if linked.len() > 2 {
        linked.split_off(2)
    } else {
        Vec::new()
    };
    for (offset, result) in linked.into_iter().enumerate() {
        results.insert(1 + offset, result);
    }
    results.extend(remainder);
}

#[cfg(test)]
mod tests {
    use super::*;
    use lattice_core::application::contracts::search::SearchResultDto;
    use std::collections::HashMap;

    fn result(id: &str, document_id: &str, position: usize) -> SearchResultDto {
        SearchResultDto {
            id: id.into(),
            title: document_id.into(),
            content: String::new(),
            score: position as f32 / 10.0,
            path: None,
            document_id: Some(document_id.into()),
            position: Some(position),
            vector_score: None,
            bm25_score: None,
            vector_rank: None,
            bm25_rank: None,
            metadata: HashMap::new(),
        }
    }

    #[test]
    fn unavailable_reason_is_clipped_to_the_trace_limit() {
        let long = "é".repeat(UNAVAILABLE_REASON_MAX_CHARS * 3);
        let reason = unavailable_reason(&long);
        assert_eq!(reason.chars().count(), UNAVAILABLE_REASON_MAX_CHARS);
        assert_eq!(unavailable_reason("index offline"), "index offline");
    }

    #[test]
    fn an_entity_bridge_groups_its_source_and_target_without_deleting_other_results() {
        let bridge_result = |id: &str, doc_id: &str, pos: usize| {
            let mut r = result(id, doc_id, pos);
            r.metadata.insert("retrievalMethod".into(), serde_json::json!("entity_bridge"));
            r.metadata.insert("bridgeSourceResultId".into(), serde_json::json!("source-0"));
            r
        };
        let mut results = vec![
            result("other", "similar-template", 0),
            bridge_result("target-2", "target", 2),
            result("source-0", "source", 0),
            bridge_result("target-0", "target", 0),
            bridge_result("target-5", "target", 5),
        ];

        promote_entity_bridge(&mut results);

        assert_eq!(
            results
                .iter()
                .map(|result| result.id.as_str())
                .collect::<Vec<_>>(),
            ["source-0", "target-0", "target-2", "other", "target-5"]
        );
        assert_eq!(results[1].score, 0.0);
        assert_eq!(results[2].score, 0.2);
    }
}

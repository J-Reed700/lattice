use std::collections::HashSet;
use std::sync::Arc;
use std::time::Instant;

use tracing::{debug, info, warn};

use crate::features::settings::dto::RetrievalTuningSettingsDto;
use crate::domain::qa::hyde::QueryType;
use crate::interfaces::di::Container;
use crate::shared::text_utils::safe_truncate;

pub(super) async fn run_kb_retrieval(
    container: &Container,
    conv_service: &Arc<dyn crate::features::conversation::ConversationServiceTrait>,
    conversation_id: &str,
    validated_message: &str,
    llm: &Arc<dyn crate::application::ports::LLMPort>,
    search_flags: super::SearchFlags,
    highlight_terms: &[String],
    kb_search_limit: usize,
    enable_reranking: bool,
    semantic_threshold: f32,
    tuning: &RetrievalTuningSettingsDto,
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
            };
        }
    };
    timings.kb_scope_load_ms = super::elapsed_ms(scope_load_start);

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
            kb_unavailable_reason: Some(
                "the active space has no indexed documents yet".to_string(),
            ),
            timings,
        };
    }
    
    let hyde_llm: Arc<dyn crate::application::ports::LLMPort> = match container
        .get_or_load_utility_llm()
        .await
    {
        Ok(Some(util)) => util,
        _ => Arc::clone(llm),
    };

    let hyde_interpretation_start = Instant::now();
    let hyde_service = crate::infrastructure::services::hyde::HyDEService::new(hyde_llm);
    let hyde_context =
        super::build_hyde_context_window_for_conversation(conv_service, conversation_id).await;
    let interpretation = hyde_service
        .interpret_query_with_context(validated_message, hyde_context.as_deref())
        .await
        .unwrap_or_else(|e| {
            warn!(error = %e, "HyDE interpretation failed, falling back to raw query");
            crate::domain::qa::hyde::HyDEInterpretation::raw_only(
                validated_message.to_string(),
                QueryType::Question,
            )
        });
    timings.kb_hyde_interpretation_ms = super::elapsed_ms(hyde_interpretation_start);

    info!(
        query_type = %interpretation.query_type,
        search_strategy = %interpretation.search_strategy,
        has_hyde = interpretation.hyde_text.is_some(),
        "HyDE query interpretation complete"
    );
    debug!(
        query = %validated_message,
        query_len = validated_message.len(),
        hyde_context_len = hyde_context.as_ref().map(|ctx| ctx.len()).unwrap_or(0),
        hyde_len = interpretation.hyde_text.as_ref().map(|s| s.len()).unwrap_or(0),
        hyde_preview = interpretation
            .hyde_text
            .as_ref()
            .map(|s| safe_truncate(s, 220))
            .unwrap_or_default(),
        "HyDE interpretation payload"
    );

    let kb_plan = super::KbSearchPlan::from_interpretation(
        validated_message,
        &interpretation,
        search_flags,
        semantic_threshold,
    );
    info!(
        has_distinct_hyde_query = kb_plan.run_parallel_keyword_branch,
        search_query_len = kb_plan.query.len(),
        mode = ?kb_plan.mode,
        "RAG search planning complete"
    );

    let search_plan_start = Instant::now();
    let (search_response, plan_timings) = super::execute_kb_search_plan(
        container,
        validated_message,
        &interpretation,
        &kb_plan,
        search_flags.force_kb_search,
        kb_search_limit,
        &scope,
        tuning,
    )
    .await;
    timings.kb_search_plan_ms = super::elapsed_ms(search_plan_start);
    timings.kb_shortlist_planning_ms = plan_timings.shortlist_planning_ms;
    timings.kb_query_execution_ms = plan_timings.query_execution_ms;
    timings.kb_merge_shortlist_gate_ms = plan_timings.merge_shortlist_gate_ms;
    let search_response =
        super::apply_hard_space_scope_filter(search_response, &scope.space_id, &scope.document_ids);
    let followup_anchor_terms = if search_flags.force_followup_mode
        || matches!(interpretation.query_type, QueryType::Followup)
    {
        super::load_followup_turn_anchor_terms(conv_service, conversation_id).await
    } else {
        HashSet::new()
    };
    let post_filters_start = Instant::now();
    let mut search_response = super::apply_rag_post_filters(
        search_response,
        validated_message,
        interpretation.hyde_text.as_deref(),
        if followup_anchor_terms.is_empty() {
            None
        } else {
            Some(&followup_anchor_terms)
        },
        tuning,
    );
    timings.kb_post_filters_ms = super::elapsed_ms(post_filters_start);
    if enable_reranking {
        let rerank_start = Instant::now();
        search_response = super::apply_rerank_stage(
            container,
            validated_message,
            &interpretation,
            search_response,
            if followup_anchor_terms.is_empty() {
                None
            } else {
                Some(&followup_anchor_terms)
            },
            tuning,
        )
        .await;
        timings.kb_rerank_ms = super::elapsed_ms(rerank_start);
    }
    let low_confidence = super::is_low_confidence_kb_response(&search_response);

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
        kb_unavailable_reason: None,
        timings,
    }
}

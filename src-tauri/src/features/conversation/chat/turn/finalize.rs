//! Stage 7: save the answer, return it, and start the work that runs after.
use super::generate::GeneratedAnswer;
use super::prepare::PreparedTurn;
use super::*;

#[allow(clippy::too_many_arguments)]
pub(super) async fn finalize_turn(
    container: &dyn ChatRuntime,
    turn: &PreparedTurn,
    flags: SearchFlags,
    generated: GeneratedAnswer,
    router: Option<TurnRouterDto>,
    user_message_id: &str,
    message_tokens: usize,
    emit: &ChatEventSink,
    flow_start: Instant,
    mut metrics: ConversationFlowTimingMetrics,
) -> Result<ChatResponse> {
    let GeneratedAnswer {
        response,
        sources,
        retrieval_trace,
        mut tokens,
        memory_usage,
    } = generated;
    tracing::info!(
        conversation_id = turn.conv_id.as_str(),
        response_length = response.len(),
        "chat_with_conversation: LLM generation successful"
    );

    // Some providers can terminate a stream without yielding content.
    // Persist a friendly fallback message instead of failing the whole turn.
    let assistant_response = if response.trim().is_empty() {
        warn!(
            conversation_id = turn.conv_id.as_str(),
            "LLM returned empty response; using fallback message"
        );
        "I couldn't generate a response for that turn. Please try rephrasing your question."
            .to_string()
    } else {
        response
    };
    let verification_start = Instant::now();
    // A closed-book turn cites nothing, so there is nothing to ground it
    // against — judging it only reports every sentence unsupported.
    let verification_enabled = turn.settings.llm.verification.enabled && !flags.closed_book;
    // Begun here and left running: the check itself happens after the answer
    // is persisted and returned (see `BackgroundVerification`), which finishes
    // this step in the persisted record when it lands.
    if verification_enabled {
        turn.recorder
            .begin(TurnStepKind::Verify, "Checking the answer", None);
    }
    let verification_metadata = Some(if verification_enabled {
        pending_metadata()
    } else {
        serde_json::json!({ "enabled": false })
    });
    metrics.verification_ms = elapsed_ms(verification_start);

    // Built here rather than after persistence so `totalMs` is time to the
    // answer the reader is about to see, and so the step list is exactly what
    // was streamed.
    if tokens.completion.is_none() {
        tokens.completion = Some(turn.llm.count_tokens(&assistant_response) as u64);
    }
    let turn_record = TurnRecordDto {
        model: Some(TurnModelDto {
            id: turn.llm.model_name().to_string(),
            name: turn.llm.model_name().to_string(),
        }),
        steps: turn.recorder.steps(),
        timing: TurnTimingDto {
            total_ms: elapsed_ms(flow_start),
            router_ms: metrics.router_ms,
            retrieval_ms: metrics.retrieval_pipeline_ms,
            generation_ms: metrics.generation_ms,
            verification_ms: metrics.verification_ms,
            tool_ms: generation_subtimings_or_default(&metrics).tool_execution_ms,
        },
        tokens,
        router,
    };

    let background_verification = verification_enabled.then(|| {
        (
            assistant_response.clone(),
            sources.clone(),
            turn_record.clone(),
        )
    });

    let finalize_start = Instant::now();
    // Finalization marks the question failed itself when the commit does not
    // land, and reports success once it has: a failure after the answer is
    // saved is not a failed turn.
    let (mut chat_response, answer_id) = finalize_successful_turn(
        container,
        &turn.conv_service,
        &turn.conv_id,
        user_message_id,
        &turn.message,
        assistant_response,
        turn.history_len,
        sources,
        verification_metadata,
        memory_usage,
        retrieval_trace,
        Some(turn_record),
        message_tokens,
        &turn.llm,
    )
    .await?;
    metrics.finalize_persistence_ms = elapsed_ms(finalize_start);

    if let Some((response, sources, record)) = background_verification {
        start_background_verification(
            container,
            turn,
            answer_id,
            response,
            sources,
            record,
            verification_start,
            emit,
        )
        .await;
    }
    metrics.total_ms = elapsed_ms(flow_start);
    log_flow_timing(&turn.conv_id, &metrics, false);

    chat_response.timing_metrics = Some(metrics);
    Ok(chat_response)
}

/// The grounding check, after the answer is saved and returned: it reports
/// through its own event and finishes the turn's verify step when it lands.
#[allow(clippy::too_many_arguments)]
async fn start_background_verification(
    container: &dyn ChatRuntime,
    turn: &PreparedTurn,
    message_id: String,
    response: String,
    sources: Vec<SourceDto>,
    record: TurnRecordDto,
    started: Instant,
    emit: &ChatEventSink,
) {
    let emit = Arc::clone(emit);
    BackgroundVerification {
        conversation_repository: container.chat_records(),
        conversation_service: Arc::clone(&turn.conv_service),
        load_utility_llm: container.utility_llm_loader(),
        conversation_id: turn.conv_id.clone(),
        request_id: turn.turn_id.clone(),
        message_id,
        response,
        sources,
        tuning: turn.settings.llm.verification.clone(),
        chat_llm: Arc::clone(&turn.llm),
        turn: Some(record),
        started,
        emit: Arc::new(move |payload| {
            if let Err(error) = emit(payload) {
                warn!(%error, "Failed to emit a finished grounding check");
            }
        }),
    }
    .spawn()
    .await;
}

macro_rules! flow_timing_event {
    ($level:expr, $conversation_id:expr, $metrics:expr, $message:literal) => {{
        let metrics = $metrics;
        let retrieval_sub = retrieval_subtimings_or_default(metrics);
        let generation_sub = generation_subtimings_or_default(metrics);
        tracing::event!(
            $level,
            conversation_id = $conversation_id,
            validate_request_ms = metrics.validate_request_ms,
            load_llm_ms = metrics.load_llm_ms,
            conversation_init_ms = metrics.conversation_init_ms,
            settings_load_ms = metrics.settings_load_ms,
            context_build_ms = metrics.context_build_ms,
            router_ms = metrics.router_ms,
            retrieval_pipeline_ms = metrics.retrieval_pipeline_ms,
            retrieval_sub_total_ms = retrieval_sub.total_ms,
            retrieval_sub_kb_total_ms = retrieval_sub.kb_total_ms,
            retrieval_sub_kb_scope_load_ms = retrieval_sub.kb_scope_load_ms,
            retrieval_sub_kb_hyde_interpretation_ms = retrieval_sub.kb_hyde_interpretation_ms,
            retrieval_sub_kb_search_plan_ms = retrieval_sub.kb_search_plan_ms,
            retrieval_sub_kb_shortlist_planning_ms = retrieval_sub.kb_shortlist_planning_ms,
            retrieval_sub_kb_query_execution_ms = retrieval_sub.kb_query_execution_ms,
            retrieval_sub_kb_merge_shortlist_gate_ms = retrieval_sub.kb_merge_shortlist_gate_ms,
            retrieval_sub_kb_post_filters_ms = retrieval_sub.kb_post_filters_ms,
            retrieval_sub_kb_rerank_ms = retrieval_sub.kb_rerank_ms,
            retrieval_sub_kb_build_sources_ms = retrieval_sub.kb_build_sources_ms,
            retrieval_sub_kb_persist_references_ms = retrieval_sub.kb_persist_references_ms,
            retrieval_sub_external_hyde_interpretation_ms =
                retrieval_sub.external_hyde_interpretation_ms,
            retrieval_sub_wiki_search_ms = retrieval_sub.wiki_search_ms,
            retrieval_sub_web_search_ms = retrieval_sub.web_search_ms,
            prompt_build_ms = metrics.prompt_build_ms,
            persist_user_message_ms = metrics.persist_user_message_ms,
            tool_prep_ms = metrics.tool_prep_ms,
            generation_ms = metrics.generation_ms,
            generation_sub_total_ms = generation_sub.total_ms,
            generation_sub_iterations = generation_sub.iterations,
            generation_sub_llm_stream_ms = generation_sub.llm_stream_ms,
            generation_sub_tool_execution_ms = generation_sub.tool_execution_ms,
            generation_sub_tool_call_count = generation_sub.tool_call_count,
            generation_sub_tool_success_count = generation_sub.tool_success_count,
            generation_sub_tool_failure_count = generation_sub.tool_failure_count,
            verification_ms = metrics.verification_ms,
            finalize_persistence_ms = metrics.finalize_persistence_ms,
            total_ms = metrics.total_ms,
            $message
        );
    }};
}

/// Every stage's timing in one line: info for an answered turn, error for a
/// failed one.
pub(super) fn log_flow_timing(
    conversation_id: &str,
    metrics: &ConversationFlowTimingMetrics,
    failed: bool,
) {
    if failed {
        flow_timing_event!(
            tracing::Level::ERROR,
            conversation_id,
            metrics,
            "chat_with_conversation: flow timing metrics (failed turn)"
        );
    } else {
        flow_timing_event!(
            tracing::Level::INFO,
            conversation_id,
            metrics,
            "chat_with_conversation: flow timing metrics"
        );
    }
}

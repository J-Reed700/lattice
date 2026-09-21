mod document_progress;
mod scoped_document_tools;

use crate::features::function_calling::dto::{
    FetchUrlContentOutput, WebSearchOutput, WikiSearchOutput, WikiSummaryOutput,
};
use crate::features::qa::dto::SourceDto;
use crate::features::settings::dto::{LLMPromptSettingsDto, ToolOutputSettingsDto};
use crate::interfaces::di::Container;
use crate::shared::error::{AppError, Result};
use crate::shared::text_utils::safe_truncate;
use futures::StreamExt;
use serde::Serialize;
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;
use std::time::Instant;
use tauri::Emitter;
use tokio::time::timeout;
use tracing::{error, info, warn};

use super::cancellation::is_cancel_requested;
use super::fetch_memory::{self, Delivery, FetchMemory, Recall};
use super::focus::FocusScope;
use super::prompting::render_tool_followup_prompt;
use super::retrieval::{
    build_web_source_citations, deduplicate_sources, fetched_page_text_room, format_tool_result,
    merge_tool_sources, record_tool_document_references,
};
use super::turn_record::{TurnRecorder, TurnStepKind};
use super::web_steps;
use super::ChatStreamEventDto;

#[derive(Debug, Serialize, Clone, Default, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ToolLoopTimingMetrics {
    pub total_ms: u64,
    pub iterations: u32,
    pub llm_stream_ms: u64,
    pub tool_execution_ms: u64,
    pub tool_call_count: u32,
    pub tool_success_count: u32,
    pub tool_failure_count: u32,
    pub empty_response_retries: u32,
    pub followup_prompt_build_ms: u64,
}

#[derive(Debug)]
pub struct ToolLoopOutcome {
    pub response: String,
    pub timings: ToolLoopTimingMetrics,
    /// Prompt and completion tokens as the provider reported them for the round
    /// that produced the answer. `None` when it reported none — absent is not
    /// zero, and a local model that says nothing about its usage has not used
    /// no tokens.
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
}

/// Emits `llm-stream` events tagged with the turn they belong to.
///
/// `llm-stream` is a single global Tauri channel with more than one producer
/// (this tool loop and the QA service), and the payloads previously carried no
/// identity at all. Any consumer therefore had to accept every event on the
/// channel, so a second concurrent generation — a query rewrite running during
/// a chat turn, or a resend whose predecessor's listener was still attached —
/// interleaved its tokens into the wrong bubble.
///
/// Tagging every emit with `conversation_id` and a per-turn `request_id` lets
/// each consumer keep only what is addressed to it.
pub(super) struct StreamEmitter<'a, R: tauri::Runtime> {
    window: &'a tauri::Window<R>,
    conversation_id: String,
    request_id: String,
    /// Whether a terminal `done` has been emitted for this turn.
    done_sent: bool,
}

impl<'a, R: tauri::Runtime> StreamEmitter<'a, R> {
    pub(super) fn new(
        window: &'a tauri::Window<R>,
        conversation_id: &str,
        request_id: &str,
    ) -> Self {
        Self {
            window,
            conversation_id: conversation_id.to_string(),
            request_id: request_id.to_string(),
            done_sent: false,
        }
    }

    /// Emit a content chunk.
    pub(super) fn content(&self, chunk: &str) -> Result<()> {
        self.emit(ChatStreamEventDto {
            content: Some(chunk.to_owned()),
            ..ChatStreamEventDto::new(&self.conversation_id, &self.request_id)
        })
    }

    /// Emit the terminal event. Idempotent, so belt-and-braces calls on
    /// several exit paths cannot produce duplicates.
    pub(super) fn done(&mut self) {
        if self.done_sent {
            return;
        }
        self.done_sent = true;
        if let Err(e) = self.emit(ChatStreamEventDto {
            done: true,
            ..ChatStreamEventDto::new(&self.conversation_id, &self.request_id)
        }) {
            warn!(error = %e, "Failed to emit stream completion");
        }
    }

    fn emit(&self, payload: ChatStreamEventDto) -> Result<()> {
        self.window
            .emit("llm-stream", payload)
            .map_err(|e| AppError::InvalidState(format!("Frontend disconnected: {}", e)))
    }
}

impl<R: tauri::Runtime> Drop for StreamEmitter<'_, R> {
    /// Guarantee a terminal event on **every** exit path, including the error
    /// returns scattered through the tool loop. Without this the UI stays
    /// stuck "generating" unless the outer invoke happens to reject.
    fn drop(&mut self) {
        self.done();
    }
}

// The tool loop is a turn-level orchestration boundary with explicit runtime inputs.
#[allow(clippy::too_many_arguments)]
pub(super) async fn run_agentic_tool_loop<R: tauri::Runtime>(
    container: &Container,
    conv_service: &Arc<dyn crate::features::conversation::ConversationServiceTrait>,
    conv_id: &str,
    request_id: &str,
    llm: &Arc<dyn crate::application::ports::LLMPort>,
    window: &tauri::Window<R>,
    context: &[String],
    enhanced_message: &str,
    validated_message: &str,
    prompt_settings: &LLMPromptSettingsDto,
    highlight_terms: &[String],
    tool_output_settings: &ToolOutputSettingsDto,
    sources: &mut Vec<SourceDto>,
    retrieval_trace: &mut Option<super::RetrievalTraceDto>,
    tools_ref: Option<&[crate::application::ports::ToolDefinition]>,
    time_budget: Duration,
    pages_already_read: FetchMemory,
    focus: &FocusScope,
    recorder: &TurnRecorder,
    // A bounded typed plan from the shared context assembler, when bounded
    // conversation memory is on for this turn. When present it *replaces* the
    // string-context input: the point of the typed path is that memory is never
    // promoted into system instructions by string parsing, and raw user bytes
    // are never trimmed on the way in.
    memory_plan: Option<&crate::application::services::context_assembler::ContextPlan>,
) -> Result<ToolLoopOutcome> {
    use crate::application::ports::llm_port::{CompletionInput, CompletionRequest};
    use crate::application::ports::StreamChunk;

    const MAX_TOOL_ITERATIONS: usize = 5;
    const CANCEL_POLL_INTERVAL_MS: u64 = 200;
    const EMPTY_RESPONSE_RETRY_HINT: &str =
        "Previous generation produced no text. Respond directly to the user query.";
    // One deadline for the whole turn: tool rounds and provider retries share it.
    let deadline = Instant::now() + time_budget;
    let budget_minutes = time_budget.as_secs().div_ceil(60);
    let budget_exhausted = || {
        AppError::ServiceNotAvailable(format!(
            "Generation exceeded this turn's {budget_minutes}-minute time budget."
        ))
    };
    // Each bounded await gets what is left of the turn, never a fresh window.
    let remaining_budget = || {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(budget_exhausted());
        }
        Ok(remaining)
    };

    let tool_loop_start = Instant::now();
    let mut timings = ToolLoopTimingMetrics::default();
    let mut input_tokens: Option<u64> = None;
    let mut output_tokens: Option<u64> = None;

    tracing::info!(
        conversation_id = conv_id,
        "chat_with_conversation: tool loop begin"
    );

    // Owns the terminal `done` event via Drop, so no error path can leave the
    // UI generating forever.
    let mut emitter = StreamEmitter::new(window, conv_id, request_id);
    let mut document_progress = document_progress::DocumentProgress::new(sources);

    let base_prompt = enhanced_message.to_string();
    let mut tool_context = context.to_vec();
    let mut current_prompt = base_prompt.clone();
    let mut native_request = CompletionRequest {
        input: match memory_plan {
            Some(plan) => plan.messages.clone(),
            None => crate::application::services::completion_input::from_context(
                &prompt_settings.system_prompt,
                context,
                &base_prompt,
            ),
        },
        tools: tools_ref.unwrap_or(&[]).to_vec(),
        // Reserving output room and then not enforcing it is only bookkeeping:
        // the model could generate past what the budget set aside and overrun
        // the window the prompt was measured against.
        max_output_tokens: memory_plan
            .map(|plan| u32::try_from(plan.max_output_tokens).unwrap_or(u32::MAX)),
        ..Default::default()
    };
    let cancellation_error = || AppError::InvalidState("Generation cancelled by user.".to_string());

    // Pages are remembered for the whole turn, not just the round that found
    // them — and the turn started before this loop did: retrieval has usually
    // opened the top results already. Starting from its record is what stops
    // round one being spent asking for pages the prompt already carries.
    let mut fetch_memory = pages_already_read;
    // Reading this conversation's own transcript is not a vault read, so it does
    // not go through the space/focus scope path. The conversation id comes from
    // the turn, and `HistoryToolScope` has no setter, so no tool argument can
    // point it at another thread.
    let history_port = crate::features::conversation::repository::ConversationRepository::new(
        container.db_pool().clone(),
    );
    // One memo for the whole turn: a repeated read of a range already exhausted
    // answers with a reference instead of paying for the same text twice.
    let mut history_memo = super::history_tools::HistoryToolMemo::default();
    for iteration in 0..MAX_TOOL_ITERATIONS {
        timings.iterations = (iteration + 1) as u32;
        if is_cancel_requested(request_id) {
            emit_cancelled_stream(window, conv_id, request_id);
            return Err(cancellation_error());
        }
        info!(
            iteration = iteration,
            "Starting LLM generation (tool iteration {})", iteration
        );

        let llm_iteration_start = Instant::now();
        // Begun before either branch so a round that never produces a token is
        // still a visible, timed step rather than a gap. The guard ends it even
        // on the budget and cancellation returns below.
        let generate_step =
            recorder.begin_guarded(TurnStepKind::Generate, thinking_label(iteration), None);
        let remaining = remaining_budget()?;
        native_request.time_budget = Some(remaining);
        let native_progress = llm.supports_typed_completions()
            && (llm.provider_name() == "llamacpp"
                || tools_ref.is_some_and(|tools| !tools.is_empty()));
        let stream_result = if native_progress {
            let response = {
                let first_text_received = std::sync::atomic::AtomicBool::new(false);
                let on_text = |text: String| {
                    if !text.is_empty()
                        && !first_text_received.swap(true, std::sync::atomic::Ordering::Relaxed)
                    {
                        info!(
                            iteration,
                            first_text_ms = llm_iteration_start.elapsed().as_millis() as u64,
                            "LLM first answer text received"
                        );
                    }
                    emitter.content(&text)
                };
                // A provider-level retry is its own step. It used to overwrite
                // the answer bubble's text, so a turn that recovered ended up
                // showing "Model response failed. Retrying..." in place of the
                // answer it went on to write.
                let on_retry = |attempt: usize| {
                    recorder.note(
                        TurnStepKind::Retry,
                        "Retrying the model",
                        None,
                        Some(format!("attempt {attempt}")),
                    );
                    Ok(())
                };
                let completion = timeout(
                    remaining,
                    llm.complete_with_retry_progress(&native_request, &on_text, &on_retry),
                );
                tokio::pin!(completion);
                loop {
                    tokio::select! {
                        result = &mut completion => break result.map_err(|_| budget_exhausted())?,
                        _ = tokio::time::sleep(Duration::from_millis(CANCEL_POLL_INTERVAL_MS)) => {
                            if is_cancel_requested(request_id) { return Err(cancellation_error()); }
                        }
                    }
                }
            }?;
            if matches!(
                response.finish_reason.as_str(),
                "incomplete" | "max_tokens" | "length" | "failed"
            ) {
                return Err(AppError::InvalidState(format!(
                    "Model stopped before completion: {}",
                    response.finish_reason
                )));
            }
            if response.text.trim().is_empty() && response.tool_calls.is_empty() {
                return Err(AppError::InvalidState(
                    "Model returned no answer or tool calls".into(),
                ));
            }
            tracing::info!(input_tokens = response.input_tokens, output_tokens = response.output_tokens, finish_reason = %response.finish_reason, "Native LLM completion");
            // The last round that reports usage is the one that produced the
            // answer, so a later round's numbers replace an earlier round's.
            input_tokens = Some(response.input_tokens);
            output_tokens = Some(response.output_tokens);
            if llm.provider_name() == "openai" {
                if let Some(items) = response.provider_output.as_array() {
                    native_request.input.extend(
                        items
                            .iter()
                            .cloned()
                            .map(|value| CompletionInput::Native { value }),
                    );
                }
            } else if llm.provider_name() == "llamacpp" {
                native_request.input.push(CompletionInput::Native {
                    value: response.provider_output,
                });
            } else {
                native_request.input.push(CompletionInput::Native { value: serde_json::json!({"role":"assistant","content":response.provider_output}) });
            }
            let calls = response
                .tool_calls
                .into_iter()
                .filter_map(|call| match call {
                    CompletionInput::ToolCall {
                        id,
                        name,
                        arguments,
                    } => Some(crate::application::ports::ToolCall {
                        id: Some(id),
                        name,
                        arguments,
                    }),
                    _ => None,
                })
                .collect::<Vec<_>>();
            let chunks = vec![
                Ok(StreamChunk::Content(response.text)),
                Ok(StreamChunk::ToolCalls(calls)),
                Ok(StreamChunk::Done),
            ];
            Ok(Box::new(futures::stream::iter(chunks))
                as Box<
                    dyn futures::Stream<Item = Result<StreamChunk>> + Send + Unpin,
                >)
        } else {
            // A provider that stalls before sending headers must not outlive the
            // budget or ignore the Stop button.
            let creation = timeout(
                remaining_budget()?,
                llm.generate_streaming_with_tools(&current_prompt, &tool_context, None, tools_ref),
            );
            tokio::pin!(creation);
            loop {
                tokio::select! {
                    result = &mut creation => break result.unwrap_or_else(|_| Err(budget_exhausted())),
                    _ = tokio::time::sleep(Duration::from_millis(CANCEL_POLL_INTERVAL_MS)) => {
                        if is_cancel_requested(request_id) { return Err(cancellation_error()); }
                    }
                }
            }
        };

        match stream_result {
            Ok(mut stream) => {
                let mut full_response = String::new();
                let mut pending_tool_calls: Vec<crate::application::ports::ToolCall> = Vec::new();

                let stream_future = async {
                    loop {
                        if is_cancel_requested(request_id) {
                            emit_cancelled_stream(window, conv_id, request_id);
                            return Err(cancellation_error());
                        }

                        let next_chunk = match timeout(
                            Duration::from_millis(CANCEL_POLL_INTERVAL_MS),
                            stream.next(),
                        )
                        .await
                        {
                            Ok(next) => next,
                            Err(_) => continue,
                        };
                        let Some(chunk_result) = next_chunk else {
                            break;
                        };

                        match chunk_result {
                            Ok(StreamChunk::Content(chunk)) => {
                                full_response.push_str(&chunk);
                                if let Err(e) = if native_progress {
                                    Ok(())
                                } else {
                                    emitter.content(&chunk)
                                } {
                                    error!("Failed to emit stream chunk: {}", e);
                                    return Err(e);
                                }
                            }
                            Ok(StreamChunk::ToolCalls(calls)) => {
                                info!(count = calls.len(), "LLM requested tool calls");
                                pending_tool_calls.extend(calls);
                            }
                            Ok(StreamChunk::Done) => break,
                            Err(e) => {
                                error!("Stream error: {}", e);
                                return Err(e);
                            }
                        }
                    }
                    Ok((full_response, pending_tool_calls))
                };

                match timeout(remaining_budget()?, stream_future).await {
                    Ok(Ok((response_text, tool_calls))) => {
                        timings.llm_stream_ms = timings
                            .llm_stream_ms
                            .saturating_add(elapsed_ms(llm_iteration_start));
                        generate_step
                            .done(Some(round_result_line(&response_text, tool_calls.len())));
                        if tool_calls.is_empty() {
                            if response_text.trim().is_empty()
                                && iteration + 1 < MAX_TOOL_ITERATIONS
                            {
                                warn!(
                                    conversation_id = conv_id,
                                    iteration = iteration,
                                    next_attempt = iteration + 2,
                                    "LLM returned empty response chunk; retrying generation"
                                );
                                timings.empty_response_retries =
                                    timings.empty_response_retries.saturating_add(1);
                                // A retry is a step of its own. It used to be
                                // written into the answer bubble, so a turn
                                // that recovered showed the retry notice where
                                // its answer should have been.
                                recorder.note(
                                    TurnStepKind::Retry,
                                    "Asking again — the model returned nothing",
                                    None,
                                    Some(format!("attempt {}", iteration + 2)),
                                );
                                tool_context
                                    .push(format!("System: [{}]", EMPTY_RESPONSE_RETRY_HINT));
                                let retry_prompt = render_tool_followup_prompt(
                                    &prompt_settings.tool_followup_prompt_template,
                                    validated_message,
                                    EMPTY_RESPONSE_RETRY_HINT,
                                );
                                current_prompt = format!("{base_prompt}\n\n{retry_prompt}");
                                continue;
                            }
                            if response_text.trim().is_empty() {
                                return Err(AppError::InvalidState(
                                    "Model returned no answer after repeated attempts".into(),
                                ));
                            }
                            emitter.done();
                            tracing::info!(
                                conversation_id = conv_id,
                                response_len = response_text.len(),
                                iterations = timings.iterations,
                                llm_stream_ms = timings.llm_stream_ms,
                                tool_execution_ms = timings.tool_execution_ms,
                                tool_call_count = timings.tool_call_count,
                                tool_success_count = timings.tool_success_count,
                                tool_failure_count = timings.tool_failure_count,
                                empty_response_retries = timings.empty_response_retries,
                                followup_prompt_build_ms = timings.followup_prompt_build_ms,
                                total_ms = elapsed_ms(tool_loop_start),
                                "chat_with_conversation: tool loop complete"
                            );
                            timings.total_ms = elapsed_ms(tool_loop_start);
                            return Ok(ToolLoopOutcome {
                                response: response_text,
                                timings,
                                input_tokens,
                                output_tokens,
                            });
                        }

                        for (call_index, tc) in tool_calls.iter().enumerate() {
                            let tool_call_start = Instant::now();
                            timings.tool_call_count = timings.tool_call_count.saturating_add(1);
                            // What is left of the window, shared between the
                            // calls this round still has to answer. The
                            // configured cap alone let one round of page
                            // fetches outgrow a small model's entire context.
                            let result_allowance = tool_result_allowance(
                                llm.max_context_tokens(),
                                chars_in_flight(
                                    &native_request.input,
                                    &tool_context,
                                    &current_prompt,
                                ),
                                tool_calls.len().saturating_sub(call_index),
                                tool_output_settings.max_chars as usize,
                            );
                            let round_output_settings = ToolOutputSettingsDto {
                                max_chars: u32::try_from(result_allowance).unwrap_or(u32::MAX),
                                ..tool_output_settings.clone()
                            };
                            if is_cancel_requested(request_id) {
                                emit_cancelled_stream(window, conv_id, request_id);
                                return Err(cancellation_error());
                            }
                            let resolved_tool = canonical_tool_name(tc.name.as_str());
                            if !is_tool_allowed(resolved_tool, tools_ref) {
                                if let Some(id) = &tc.id {
                                    native_request.input.push(CompletionInput::ToolResult {
                                        id: id.clone(),
                                        output: "Requested tool is unavailable".into(),
                                    });
                                }
                                let available_tools = available_tool_names(tools_ref);
                                warn!(
                                    requested_function = tc.name.as_str(),
                                    resolved_function = resolved_tool,
                                    available_tools = available_tools.join(", "),
                                    "Skipping unavailable tool requested by LLM"
                                );
                                tool_context.push(format!(
                                    "System: [Tool '{}' is not available. Available tools: {}. Continue with available tools or answer directly.]",
                                    resolved_tool,
                                    available_tools.join(", ")
                                ));
                                continue;
                            }
                            // A URL this turn already failed on costs a request
                            // to learn nothing. Answer it from memory so the
                            // round can still spend its time somewhere useful.
                            if let Some(url) = fetch_memory::fetch_target(&tc.arguments) {
                                if let Some(reason) = fetch_memory.previous_failure(url) {
                                    let notice = format!(
                                        "Not retried: {url} already failed this turn ({reason}). Use a different source.",
                                    );
                                    warn!(
                                        requested_function = tc.name.as_str(),
                                        resolved_function = resolved_tool,
                                        url,
                                        reason,
                                        "Skipping a URL that already failed this turn"
                                    );
                                    timings.tool_failure_count =
                                        timings.tool_failure_count.saturating_add(1);
                                    if let Some(id) = &tc.id {
                                        native_request.input.push(CompletionInput::ToolResult {
                                            id: id.clone(),
                                            output: notice.clone(),
                                        });
                                    }
                                    tool_context.push(format!("System: [{notice}]"));
                                    timings.tool_execution_ms = timings
                                        .tool_execution_ms
                                        .saturating_add(elapsed_ms(tool_call_start));
                                    continue;
                                }
                                // A page this turn already read is answered
                                // from memory: no request, no politeness delay,
                                // and nothing re-sent that the model has.
                                if let Some(recall) = fetch_memory.recall(url) {
                                    let notice = recalled_page_notice(url, &recall);
                                    info!(
                                        requested_function = tc.name.as_str(),
                                        resolved_function = resolved_tool,
                                        url,
                                        already_whole =
                                            matches!(recall, Recall::AlreadyWhole { .. }),
                                        "Answered a repeat page request from this turn's memory"
                                    );
                                    timings.tool_success_count =
                                        timings.tool_success_count.saturating_add(1);
                                    if let Some(id) = &tc.id {
                                        native_request.input.push(CompletionInput::ToolResult {
                                            id: id.clone(),
                                            output: notice.clone(),
                                        });
                                    }
                                    tool_context.push(format!("System: [{notice}]"));
                                    timings.tool_execution_ms = timings
                                        .tool_execution_ms
                                        .saturating_add(elapsed_ms(tool_call_start));
                                    continue;
                                }
                            }
                            let tool_step = recorder.begin_guarded_with_links(
                                tool_step_kind(resolved_tool),
                                tool_activity_label(resolved_tool, &tc.arguments),
                                web_steps::tool_detail(resolved_tool, &tc.arguments)
                                    .or_else(|| Some(tool_argument_summary(&tc.arguments))),
                                web_steps::links_for_call(resolved_tool, &tc.arguments),
                            );
                            info!(
                                requested_function = tc.name.as_str(),
                                resolved_function = resolved_tool,
                                "Executing tool call from LLM"
                            );

                            let call = crate::features::function_calling::domain::FunctionCall::new(
                                uuid::Uuid::new_v4().to_string(),
                                resolved_tool,
                                tc.arguments.clone(),
                            );
                            let executed = if super::history_tools::is_history_tool(resolved_tool) {
                                let scope = super::history_tools::HistoryToolScope::new(
                                    conv_id,
                                    super::history_tools::HistoryToolBudget {
                                        // A char allowance used as a byte ceiling
                                        // is conservative in the safe direction.
                                        max_response_bytes: result_allowance,
                                        deadline: Some(deadline),
                                    },
                                );
                                super::history_tools::execute(
                                    &history_port,
                                    &scope,
                                    &mut history_memo,
                                    call,
                                )
                                .await
                            } else {
                                scoped_document_tools::execute(container, conv_id, focus, call)
                                    .await
                            };
                            match executed {
                                Ok(result) => {
                                    if result.success {
                                        web_steps::finish_tool_step(
                                            tool_step,
                                            recorder,
                                            resolved_tool,
                                            &tc.arguments,
                                            result.data.as_ref(),
                                        );
                                    } else {
                                        tool_step.failed(result.error_message.clone());
                                    }
                                    timings.tool_success_count =
                                        timings.tool_success_count.saturating_add(1);
                                    let tool_sources = collect_tool_sources(
                                        resolved_tool,
                                        &result,
                                        highlight_terms,
                                        tool_output_settings,
                                    );
                                    let mut tool_chunk_ids = std::collections::HashSet::new();
                                    if !tool_sources.is_empty() {
                                        let offered = tool_sources.len();
                                        let before = sources.len();
                                        tool_chunk_ids = merge_tool_sources(sources, tool_sources);
                                        *sources = deduplicate_sources(std::mem::take(sources));
                                        super::retrieval::assign_citation_ids(sources);
                                        info!(
                                            requested_function = tc.name.as_str(),
                                            resolved_function = resolved_tool,
                                            offered_sources = offered,
                                            added_sources = sources.len().saturating_sub(before),
                                            merged_sources = sources.len(),
                                            "Tool call added verifiable sources"
                                        );
                                    }

                                    if document_progress.record(resolved_tool, &result) {
                                        let trace = retrieval_trace.get_or_insert_with(|| {
                                            super::RetrievalTraceDto {
                                                scope: "vault".to_string(),
                                                ..Default::default()
                                            }
                                        });
                                        document_progress.update_trace(trace);
                                        if let Err(error) = emitter.emit(ChatStreamEventDto {
                                            status: Some("retrieval".to_owned()),
                                            retrieval: Some(trace.clone()),
                                            ..ChatStreamEventDto::new(conv_id, request_id)
                                        }) {
                                            warn!(%error, "Failed to update retrieval trace after tool search");
                                        }
                                    }

                                    let mut result_text = format_tool_result(
                                        resolved_tool,
                                        &result,
                                        highlight_terms,
                                        &round_output_settings,
                                    );
                                    for source in sources
                                        .iter()
                                        .filter(|s| tool_chunk_ids.contains(&s.chunk_id))
                                    {
                                        if let Some(number) = source.citation_id {
                                            result_text.push_str(&format!(
                                                "\n\nCitable passage [{number}] — {}\n{}",
                                                source.file_name, source.content
                                            ));
                                        }
                                    }
                                    if let Some(id) = &tc.id {
                                        native_request.input.push(CompletionInput::ToolResult {
                                            id: id.clone(),
                                            output: result_text.clone(),
                                        });
                                    }
                                    if let Err(e) = record_tool_document_references(
                                        conv_service,
                                        conv_id,
                                        resolved_tool,
                                        &result,
                                    )
                                    .await
                                    {
                                        warn!(
                                            error = %e,
                                            requested_function = tc.name.as_str(),
                                            resolved_function = resolved_tool,
                                            "Failed to persist tool document references"
                                        );
                                    }

                                    tool_context.push(format!(
                                        "Assistant: [Called tool '{}' with args: {}]",
                                        resolved_tool,
                                        serde_json::to_string(&tc.arguments).unwrap_or_default()
                                    ));
                                    tool_context.push(format!(
                                        "System: [Tool '{}' result (excerpted): {}]",
                                        resolved_tool, result_text
                                    ));
                                    if result.success && resolved_tool == "fetch_url_content" {
                                        if let (Some(url), Some(page)) = (
                                            fetch_memory::fetch_target(&tc.arguments),
                                            result.data.clone().and_then(|data| {
                                                serde_json::from_value::<FetchUrlContentOutput>(
                                                    data,
                                                )
                                                .ok()
                                            }),
                                        ) {
                                            let room = fetched_page_text_room(result_allowance);
                                            let text = page.content.trim();
                                            let delivery = if text.chars().count() <= room {
                                                Delivery::Whole
                                            } else {
                                                Delivery::Clipped { shown_chars: room }
                                            };
                                            fetch_memory.record_page(url, text, delivery);
                                        }
                                    }
                                    if !result.success {
                                        if let Some(url) = fetch_memory::fetch_target(&tc.arguments)
                                        {
                                            fetch_memory.record_failure(
                                                url,
                                                result
                                                    .error_message
                                                    .as_deref()
                                                    .unwrap_or("the fetch did not succeed"),
                                            );
                                        }
                                    }
                                    info!(
                                        requested_function = tc.name.as_str(),
                                        resolved_function = resolved_tool,
                                        success = result.success,
                                        "Tool call executed"
                                    );
                                }
                                Err(e) => {
                                    tool_step.failed(Some(e.to_string()));
                                    timings.tool_failure_count =
                                        timings.tool_failure_count.saturating_add(1);
                                    warn!(
                                        requested_function = tc.name.as_str(),
                                        resolved_function = resolved_tool,
                                        error = %e,
                                        "Tool call failed"
                                    );
                                    if let Some(id) = &tc.id {
                                        native_request.input.push(CompletionInput::ToolResult {
                                            id: id.clone(),
                                            output: format!("Tool failed: {e}"),
                                        });
                                    }
                                    if let Some(url) = fetch_memory::fetch_target(&tc.arguments) {
                                        fetch_memory.record_failure(url, &e.to_string());
                                    }
                                    tool_context.push(format!(
                                        "System: [Tool '{}' failed: {}]",
                                        resolved_tool, e
                                    ));
                                }
                            }
                            timings.tool_execution_ms = timings
                                .tool_execution_ms
                                .saturating_add(elapsed_ms(tool_call_start));
                        }

                        // Restate the whole dead list once per round. A single
                        // failure line among several results is easy for the
                        // model to read past; the standing list is not.
                        if let Some(advisory) = fetch_memory.advisory() {
                            tool_context.push(format!("System: [{advisory}]"));
                        }

                        let followup_prompt_start = Instant::now();
                        let previous_response = if response_text.is_empty() {
                            String::new()
                        } else {
                            format!(
                                "Previous draft: {}",
                                safe_truncate(
                                    &response_text,
                                    tool_output_settings.max_chars as usize
                                )
                            )
                        };
                        let followup_prompt = render_tool_followup_prompt(
                            &prompt_settings.tool_followup_prompt_template,
                            validated_message,
                            &previous_response,
                        );
                        timings.followup_prompt_build_ms = timings
                            .followup_prompt_build_ms
                            .saturating_add(elapsed_ms(followup_prompt_start));
                        current_prompt = format!("{base_prompt}\n\n{followup_prompt}");
                    }
                    Ok(Err(e)) => return Err(e),
                    Err(_) => {
                        let llm_stream_ms = timings
                            .llm_stream_ms
                            .saturating_add(elapsed_ms(llm_iteration_start));
                        error!(
                            llm_stream_ms,
                            budget_minutes, "LLM generation exceeded the turn time budget"
                        );
                        return Err(budget_exhausted());
                    }
                }
            }
            Err(e) => {
                let llm_stream_ms = timings
                    .llm_stream_ms
                    .saturating_add(elapsed_ms(llm_iteration_start));
                error!(llm_stream_ms, error = %e, "Failed to start LLM stream");
                return Err(e);
            }
        }
    }

    warn!(
        conversation_id = conv_id,
        iterations = MAX_TOOL_ITERATIONS,
        llm_stream_ms = timings.llm_stream_ms,
        tool_execution_ms = timings.tool_execution_ms,
        tool_call_count = timings.tool_call_count,
        tool_success_count = timings.tool_success_count,
        tool_failure_count = timings.tool_failure_count,
        empty_response_retries = timings.empty_response_retries,
        followup_prompt_build_ms = timings.followup_prompt_build_ms,
        total_ms = elapsed_ms(tool_loop_start),
        "Tool calling loop exhausted iterations"
    );
    emitter.done();
    Err(AppError::Other(
        "Tool calling loop exceeded maximum iterations without producing a final response".into(),
    ))
}

fn elapsed_ms(start: Instant) -> u64 {
    u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX)
}

fn collect_tool_sources(
    tool_name: &str,
    result: &crate::features::function_calling::domain::FunctionResult,
    highlight_terms: &[String],
    tool_output_settings: &ToolOutputSettingsDto,
) -> Vec<SourceDto> {
    if !result.success {
        return Vec::new();
    }
    let data = match result.data.clone() {
        Some(value) => value,
        None => return Vec::new(),
    };

    match tool_name {
        "web_search" => {
            if let Ok(output) = serde_json::from_value::<WebSearchOutput>(data) {
                return build_web_source_citations(
                    &output.results,
                    highlight_terms,
                    tool_output_settings.excerpt_chars as usize,
                );
            }
            Vec::new()
        }
        "fetch_url_content" => {
            if let Ok(output) = serde_json::from_value::<FetchUrlContentOutput>(data) {
                let content =
                    safe_truncate(&output.content, tool_output_settings.excerpt_chars as usize);
                let title = output
                    .title
                    .unwrap_or_else(|| output.url.clone())
                    .trim()
                    .to_string();
                let modified_at = chrono::Utc::now().to_rfc3339();
                let highlights = if highlight_terms.is_empty() {
                    None
                } else {
                    Some(highlight_terms.to_vec())
                };
                return vec![SourceDto {
                    page_number: None,
                    document_id: format!("web:{}", output.url),
                    chunk_id: "web-content-1".to_string(),
                    content: content.clone(),
                    score: 1.0,
                    path: Some(output.url.clone()),
                    position: Some(1),
                    file_name: if title.is_empty() {
                        output.url.clone()
                    } else {
                        title
                    },
                    file_path: output.url.clone(),
                    mime_type: output
                        .content_type
                        .unwrap_or_else(|| "text/html".to_string()),
                    category: "Web Article".to_string(),
                    file_size_bytes: output.word_count as i64,
                    modified_at,
                    excerpt: Some(content),
                    highlights,
                    section: None,
                    chunk_index: Some(1),
                    chunk_excerpts: None,
                    citation_id: None,
                }];
            }
            Vec::new()
        }
        "wiki_search" => {
            if let Ok(output) = serde_json::from_value::<WikiSearchOutput>(data) {
                let as_web_results: Vec<_> = output
                    .results
                    .into_iter()
                    .map(
                        |item| crate::features::function_calling::dto::WebSearchResult {
                            title: item.title,
                            url: item.url,
                            snippet: item.snippet,
                            published_date: None,
                            source: Some("wikipedia".to_string()),
                        },
                    )
                    .collect();
                return build_web_source_citations(
                    &as_web_results,
                    highlight_terms,
                    tool_output_settings.excerpt_chars as usize,
                );
            }
            Vec::new()
        }
        "wiki_summary" => {
            if let Ok(output) = serde_json::from_value::<WikiSummaryOutput>(data) {
                let content = safe_truncate(
                    output.extract.trim(),
                    tool_output_settings.excerpt_chars as usize,
                );
                let highlights = if highlight_terms.is_empty() {
                    None
                } else {
                    Some(highlight_terms.to_vec())
                };
                return vec![SourceDto {
                    page_number: None,
                    document_id: format!("wiki:{}", output.title.replace(' ', "_")),
                    chunk_id: "wiki-summary-1".to_string(),
                    content: content.clone(),
                    score: 1.0,
                    path: Some(output.url.clone()),
                    position: Some(1),
                    file_name: output.title.clone(),
                    file_path: output.url.clone(),
                    mime_type: "text/plain".to_string(),
                    category: "Wikipedia".to_string(),
                    file_size_bytes: output.extract.chars().count() as i64,
                    modified_at: chrono::Utc::now().to_rfc3339(),
                    excerpt: Some(content),
                    highlights,
                    section: None,
                    chunk_index: Some(1),
                    chunk_excerpts: None,
                    citation_id: None,
                }];
            }
            Vec::new()
        }
        _ => Vec::new(),
    }
}

fn canonical_tool_name(tool_name: &str) -> &str {
    match tool_name {
        "browser.search" => "web_search",
        "browser.open" | "browser.fetch" => "fetch_url_content",
        "repo_browser.search" => "semantic_search",
        "repo_browser.read" => "get_document",
        "repo_browser.list" => "list_documents",
        _ => tool_name,
    }
}

fn is_tool_allowed(
    tool_name: &str,
    tools_ref: Option<&[crate::application::ports::ToolDefinition]>,
) -> bool {
    tools_ref.is_some_and(|tools| tools.iter().any(|tool| tool.name.as_str() == tool_name))
}

fn available_tool_names(
    tools_ref: Option<&[crate::application::ports::ToolDefinition]>,
) -> Vec<String> {
    let mut names = tools_ref
        .map(|tools| {
            tools
                .iter()
                .map(|tool| tool.name.clone())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if names.is_empty() {
        return vec!["none".to_string()];
    }
    let mut seen = HashSet::new();
    names.retain(|name| seen.insert(name.clone()));
    names.sort();
    names
}

/// What to show while the model is producing no text.
///
/// The first round is plain thinking; later rounds follow tool results, and
/// saying so is the difference between "stuck" and "on its third source".
fn thinking_label(iteration: usize) -> &'static str {
    if iteration == 0 {
        "Thinking"
    } else {
        "Reading what it found and thinking"
    }
}

/// What one generation round came to.
///
/// A round that only asked for tools wrote no answer, and saying "0 words"
/// would read as a failure rather than as the model deciding to go and look
/// something up.
fn round_result_line(response_text: &str, tool_calls: usize) -> String {
    let words = response_text.split_whitespace().count();
    match (words, tool_calls) {
        (0, 0) => "nothing".to_string(),
        (0, 1) => "1 tool call".to_string(),
        (0, calls) => format!("{calls} tool calls"),
        (words, 0) => format!("{words} words"),
        (words, 1) => format!("{words} words and 1 tool call"),
        (words, calls) => format!("{words} words and {calls} tool calls"),
    }
}

/// Which kind of step a tool call is, so the timeline groups a document search
/// with the retrieval pipeline's rather than filing it under "tool".
fn tool_step_kind(tool: &str) -> TurnStepKind {
    match tool {
        "semantic_search" => TurnStepKind::SearchDocuments,
        "get_document" | "list_documents" => TurnStepKind::OpenDocument,
        "web_search" => TurnStepKind::WebSearch,
        "fetch_url_content" => TurnStepKind::ReadPage,
        "wiki_search" | "wiki_summary" => TurnStepKind::Wiki,
        _ => TurnStepKind::Tool,
    }
}

/// A tool call's arguments in one line, for the step's detail.
///
/// The interesting arguments are short and named the same way across tools; the
/// rest is noise a reader cannot act on, and `TurnRecorder` clips whatever gets
/// through.
fn tool_argument_summary(arguments: &serde_json::Value) -> String {
    const INTERESTING: [&str; 4] = ["query", "url", "document_id", "page"];
    let Some(object) = arguments.as_object() else {
        return arguments.to_string();
    };
    let summary = INTERESTING
        .iter()
        .filter_map(|key| object.get(*key).map(|value| (key, value)))
        .map(|(key, value)| match value.as_str() {
            Some(text) => format!("{key}: {text}"),
            None => format!("{key}: {value}"),
        })
        .collect::<Vec<_>>()
        .join(" · ");
    if summary.is_empty() {
        arguments.to_string()
    } else {
        summary
    }
}

/// Share of the context window the prompt may fill. The rest is the reply's.
const CONTEXT_FILL_LIMIT: f64 = 0.75;

/// Characters per token, rounded down, so the estimate errs towards leaving room.
const CHARS_PER_TOKEN: usize = 3;

/// A result cut shorter than this says too little to have been worth the round
/// that asked for it, so a nearly full window still gets this much.
const MIN_TOOL_RESULT_CHARS: usize = 1_500;

/// Roughly how many characters the next generation already has to read. The
/// native request and the rendered transcript carry the same turn in two forms
/// and only one is sent, so the larger of the two is the honest figure.
fn chars_in_flight(
    native_input: &[crate::application::ports::llm_port::CompletionInput],
    transcript: &[String],
    prompt: &str,
) -> usize {
    use crate::application::ports::llm_port::CompletionInput;
    let native: usize = native_input
        .iter()
        .map(|item| match item {
            CompletionInput::Message { content, .. } => content.chars().count(),
            CompletionInput::ToolResult { output, .. } => output.chars().count(),
            CompletionInput::ToolCall {
                name, arguments, ..
            } => name.len() + arguments.to_string().len(),
            CompletionInput::Native { value } => value.to_string().len(),
        })
        .sum();
    let rendered: usize = transcript
        .iter()
        .map(|line| line.chars().count())
        .sum::<usize>()
        + prompt.chars().count();
    native.max(rendered)
}

/// How many characters one tool result may take, given the model's window,
/// what is already in it, and how many results this round has still to fit.
fn tool_result_allowance(
    context_tokens: usize,
    chars_in_flight: usize,
    calls_left: usize,
    configured_max: usize,
) -> usize {
    let window_chars =
        ((context_tokens as f64 * CONTEXT_FILL_LIMIT) as usize).saturating_mul(CHARS_PER_TOKEN);
    let share = window_chars.saturating_sub(chars_in_flight) / calls_left.max(1);
    share.clamp(MIN_TOOL_RESULT_CHARS.min(configured_max), configured_max)
}

/// What the model is told when it asks for a page this turn already read.
///
/// Says where the text already is, because "already fetched" alone invites the
/// model to conclude the content was lost and ask a third way.
fn recalled_page_notice(url: &str, recall: &Recall) -> String {
    match recall {
        Recall::AlreadyWhole { word_count } => format!(
            "Not fetched again: {url} was already read in full this turn ({word_count} words) and its complete text is in your context above. There is nothing more on that page. Answer from it, or choose a different source.",
        ),
        Recall::Remainder { text, shown_chars } => format!(
            "Continuation of {url}. The first {shown_chars} characters are in your context above; this is the rest of the page, and with it you have the whole page:\n\n{text}",
        ),
    }
}

/// What to show while one tool call runs. Names the host for a fetch, because
/// "Reading thereviewgeek.com" is the difference between visible progress and a
/// spinner, and a blocked site is then obvious rather than mysterious.
fn tool_activity_label(tool: &str, arguments: &serde_json::Value) -> String {
    match tool {
        "fetch_url_content" => match fetch_memory::fetch_target(arguments)
            .and_then(|url| url::Url::parse(url).ok())
            .and_then(|url| url.host_str().map(|host| host.to_string()))
        {
            Some(host) => format!("Reading {host}"),
            None => "Reading a web page".to_string(),
        },
        "web_search" => "Searching the web".to_string(),
        "wiki_search" | "wiki_summary" => "Checking Wikipedia".to_string(),
        "semantic_search" => "Searching your documents".to_string(),
        "list_documents" => "Looking through your documents".to_string(),
        "get_document" => "Opening a document".to_string(),
        "search_conversation_history" | "read_conversation_history" => {
            "Looking back through this conversation".to_string()
        }
        other => format!("Running {other}"),
    }
}

fn emit_cancelled_stream<R: tauri::Runtime>(
    window: &tauri::Window<R>,
    conversation_id: &str,
    request_id: &str,
) {
    if let Err(e) = window.emit(
        "llm-stream",
        ChatStreamEventDto {
            done: true,
            status: Some("cancelled".to_owned()),
            ..ChatStreamEventDto::new(conversation_id, request_id)
        },
    ) {
        warn!("Failed to emit cancellation event: {}", e);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::ports::llm_port::CompletionInput;

    /// Four page fetches in one round on a 32k-token model. At the configured
    /// 50,000 characters each they are ~200,000 characters — several times the
    /// model's whole window — and the next generation simply failed.
    #[test]
    fn a_round_of_results_is_made_to_fit_a_small_window() {
        let context_tokens = 32_768;
        let window_chars = (context_tokens as f64 * CONTEXT_FILL_LIMIT) as usize * CHARS_PER_TOKEN;
        let already = 20_000;

        let mut spent = already;
        for calls_left in (1..=4).rev() {
            spent += tool_result_allowance(context_tokens, spent, calls_left, 50_000);
        }
        assert!(
            spent <= window_chars,
            "{spent} characters in a {window_chars}-character window"
        );
    }

    /// The model in the log has a 131k window. It should get whole pages.
    #[test]
    fn a_large_window_gives_each_result_the_configured_maximum() {
        assert_eq!(tool_result_allowance(131_072, 20_000, 4, 50_000), 50_000);
    }

    /// A result of nothing would make the round that asked for it worthless.
    #[test]
    fn a_full_window_still_leaves_a_result_worth_reading() {
        assert_eq!(
            tool_result_allowance(8_192, 1_000_000, 3, 50_000),
            MIN_TOOL_RESULT_CHARS
        );
    }

    #[test]
    fn a_configured_maximum_below_the_floor_is_respected() {
        assert_eq!(tool_result_allowance(131_072, 0, 1, 800), 800);
    }

    #[test]
    fn what_is_in_flight_is_the_larger_of_the_two_forms_of_the_turn() {
        let native = vec![
            CompletionInput::Message {
                role: "user".to_string(),
                content: "x".repeat(100),
            },
            CompletionInput::ToolResult {
                id: "1".to_string(),
                output: "y".repeat(400),
            },
        ];
        let transcript = vec!["z".repeat(50)];
        assert_eq!(chars_in_flight(&native, &transcript, "prompt"), 500);
        assert_eq!(chars_in_flight(&[], &transcript, "prompt"), 56);
    }

    /// "Already fetched" alone invites the model to think the text was lost.
    #[test]
    fn a_page_already_read_in_full_is_pointed_back_to_the_context() {
        let notice = recalled_page_notice(
            "https://example.test/recap",
            &Recall::AlreadyWhole { word_count: 3521 },
        );
        assert!(notice.contains("https://example.test/recap"), "{notice}");
        assert!(notice.contains("3521 words"), "{notice}");
        assert!(notice.contains("in your context above"), "{notice}");
    }

    #[test]
    fn the_rest_of_a_clipped_page_says_where_the_first_part_is() {
        let notice = recalled_page_notice(
            "https://example.test/recap",
            &Recall::Remainder {
                text: "and then the flames came down.".to_string(),
                shown_chars: 12_000,
            },
        );
        assert!(notice.contains("first 12000 characters"), "{notice}");
        assert!(
            notice.ends_with("and then the flames came down."),
            "{notice}"
        );
    }
}

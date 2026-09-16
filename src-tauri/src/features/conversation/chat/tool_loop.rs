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
use super::prompting::render_tool_followup_prompt;
use super::retrieval::{
    build_web_source_citations, deduplicate_sources, format_tool_result,
    record_tool_document_references,
};
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

    /// Emit a non-fatal status update (e.g. a retry notice).
    pub(super) fn status(&self, status: &str, attempt: usize) -> Result<()> {
        self.emit(ChatStreamEventDto {
            status: Some(status.to_owned()),
            attempt: Some(attempt),
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
) -> Result<ToolLoopOutcome> {
    use crate::application::ports::llm_port::{CompletionInput, CompletionRequest};
    use crate::application::ports::StreamChunk;

    const MAX_TOOL_ITERATIONS: usize = 5;
    const CANCEL_POLL_INTERVAL_MS: u64 = 200;
    const EMPTY_RESPONSE_RETRY_HINT: &str =
        "Previous generation produced no text. Respond directly to the user query.";
    let timeout_duration = Duration::from_secs(300);

    let tool_loop_start = Instant::now();
    let mut timings = ToolLoopTimingMetrics::default();

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
        input: crate::application::services::completion_input::from_context(
            &prompt_settings.system_prompt,
            context,
            &base_prompt,
        ),
        tools: tools_ref.unwrap_or(&[]).to_vec(),
        ..Default::default()
    };
    let cancellation_error = || AppError::InvalidState("Generation cancelled by user.".to_string());

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
                let on_retry = |attempt| emitter.status("retrying", attempt);
                let completion = timeout(
                    timeout_duration,
                    llm.complete_with_retry_progress(&native_request, &on_text, &on_retry),
                );
                tokio::pin!(completion);
                loop {
                    tokio::select! {
                        result = &mut completion => break result.map_err(|_| AppError::ServiceNotAvailable(
                            "LLM generation timed out. The model may be overloaded.".into(),
                        ))?,
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
            llm.generate_streaming_with_tools(&current_prompt, &tool_context, None, tools_ref)
                .await
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

                match timeout(timeout_duration, stream_future).await {
                    Ok(Ok((response_text, tool_calls))) => {
                        timings.llm_stream_ms = timings
                            .llm_stream_ms
                            .saturating_add(elapsed_ms(llm_iteration_start));
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
                                emitter.status("retrying", iteration + 2)?;
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
                            });
                        }

                        for tc in &tool_calls {
                            let tool_call_start = Instant::now();
                            timings.tool_call_count = timings.tool_call_count.saturating_add(1);
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
                            match scoped_document_tools::execute(container, conv_id, call).await {
                                Ok(result) => {
                                    timings.tool_success_count =
                                        timings.tool_success_count.saturating_add(1);
                                    let tool_sources = collect_tool_sources(
                                        resolved_tool,
                                        &result,
                                        highlight_terms,
                                        tool_output_settings,
                                    );
                                    let tool_chunk_ids: std::collections::HashSet<_> =
                                        tool_sources.iter().map(|s| s.chunk_id.clone()).collect();
                                    if !tool_sources.is_empty() {
                                        let added = tool_sources.len();
                                        sources.extend(tool_sources);
                                        *sources = deduplicate_sources(std::mem::take(sources));
                                        super::retrieval::assign_citation_ids(sources);
                                        info!(
                                            requested_function = tc.name.as_str(),
                                            resolved_function = resolved_tool,
                                            added_sources = added,
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
                                        tool_output_settings,
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
                                    info!(
                                        requested_function = tc.name.as_str(),
                                        resolved_function = resolved_tool,
                                        success = result.success,
                                        "Tool call executed"
                                    );
                                }
                                Err(e) => {
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
                        error!(llm_stream_ms, "LLM generation timed out after 5 minutes");
                        return Err(AppError::ServiceNotAvailable(
                            "LLM generation timed out. The model may be overloaded.".into(),
                        ));
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

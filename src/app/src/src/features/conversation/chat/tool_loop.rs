use crate::features::settings::dto::{LLMPromptSettingsDto, ToolOutputSettingsDto};
use crate::features::qa::dto::SourceDto;
use crate::features::function_calling::dto::{
    FetchUrlContentOutput, WebSearchOutput, WikiSearchOutput, WikiSummaryOutput,
};
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

#[derive(Debug, Serialize, Clone, Default)]
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

pub(super) async fn run_agentic_tool_loop<R: tauri::Runtime>(
    container: &Container,
    conv_service: &Arc<dyn crate::features::conversation::ConversationServiceTrait>,
    conv_id: &str,
    llm: &Arc<dyn crate::application::ports::LLMPort>,
    window: &tauri::Window<R>,
    context: &[String],
    enhanced_message: &str,
    validated_message: &str,
    prompt_settings: &LLMPromptSettingsDto,
    highlight_terms: &[String],
    tool_output_settings: &ToolOutputSettingsDto,
    sources: &mut Vec<SourceDto>,
    tools_ref: Option<&[crate::application::ports::ToolDefinition]>,
) -> Result<ToolLoopOutcome> {
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

    let base_prompt = enhanced_message.to_string();
    let mut tool_context = context.to_vec();
    let mut current_prompt = base_prompt.clone();
    let cancellation_error = || AppError::InvalidState("Generation cancelled by user.".to_string());

    for iteration in 0..MAX_TOOL_ITERATIONS {
        timings.iterations = (iteration + 1) as u32;
        if is_cancel_requested(conv_id) {
            emit_cancelled_stream(window);
            return Err(cancellation_error());
        }
        info!(
            iteration = iteration,
            "Starting LLM generation (tool iteration {})", iteration
        );

        let llm_iteration_start = Instant::now();
        let stream_result = llm
            .generate_streaming_with_tools(&current_prompt, &tool_context, None, tools_ref)
            .await;

        match stream_result {
            Ok(mut stream) => {
                let mut full_response = String::new();
                let mut pending_tool_calls: Vec<crate::application::ports::ToolCall> = Vec::new();

                let stream_future = async {
                    loop {
                        if is_cancel_requested(conv_id) {
                            emit_cancelled_stream(window);
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
                                if let Err(e) = window.emit(
                                    "llm-stream",
                                    serde_json::json!({
                                        "content": chunk,
                                        "done": false
                                    }),
                                ) {
                                    error!("Failed to emit stream chunk: {}", e);
                                    return Err(AppError::InvalidState(format!(
                                        "Frontend disconnected: {}",
                                        e
                                    )));
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
                                if let Err(e) = window.emit(
                                    "llm-stream",
                                    serde_json::json!({
                                        "content": "",
                                        "done": false,
                                        "status": "retrying",
                                        "attempt": iteration + 2
                                    }),
                                ) {
                                    warn!("Failed to emit retrying status: {}", e);
                                }
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
                            if let Err(e) =
                                window.emit("llm-stream", serde_json::json!({ "done": true }))
                            {
                                warn!("Failed to emit stream completion: {}", e);
                            }
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

                        let executor = container.function_executor();
                        for tc in &tool_calls {
                            let tool_call_start = Instant::now();
                            timings.tool_call_count = timings.tool_call_count.saturating_add(1);
                            if is_cancel_requested(conv_id) {
                                emit_cancelled_stream(window);
                                return Err(cancellation_error());
                            }
                            let resolved_tool = canonical_tool_name(tc.name.as_str());
                            if !is_tool_allowed(resolved_tool, tools_ref) {
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
                            match executor.execute(call).await {
                                Ok(result) => {
                                    timings.tool_success_count =
                                        timings.tool_success_count.saturating_add(1);
                                    let tool_sources = collect_tool_sources(
                                        resolved_tool,
                                        &result,
                                        highlight_terms,
                                        tool_output_settings,
                                    );
                                    if !tool_sources.is_empty() {
                                        let added = tool_sources.len();
                                        sources.extend(tool_sources);
                                        *sources = deduplicate_sources(std::mem::take(sources));
                                        info!(
                                            requested_function = tc.name.as_str(),
                                            resolved_function = resolved_tool,
                                            added_sources = added,
                                            merged_sources = sources.len(),
                                            "Tool call added verifiable sources"
                                        );
                                    }

                                    let result_text = format_tool_result(
                                        resolved_tool,
                                        &result,
                                        highlight_terms,
                                        tool_output_settings,
                                    );
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
                        timings.llm_stream_ms = timings
                            .llm_stream_ms
                            .saturating_add(elapsed_ms(llm_iteration_start));
                        error!("LLM generation timed out after 5 minutes");
                        return Err(AppError::ServiceNotAvailable(
                            "LLM generation timed out. The model may be overloaded.".into(),
                        ));
                    }
                }
            }
            Err(e) => {
                timings.llm_stream_ms = timings
                    .llm_stream_ms
                    .saturating_add(elapsed_ms(llm_iteration_start));
                error!("Failed to start LLM stream: {}", e);
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
    if let Err(e) = window.emit("llm-stream", serde_json::json!({ "done": true })) {
        warn!("Failed to emit stream completion: {}", e);
    }
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
    tools_ref.map_or(false, |tools| {
        tools.iter().any(|tool| tool.name.as_str() == tool_name)
    })
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

fn emit_cancelled_stream<R: tauri::Runtime>(window: &tauri::Window<R>) {
    if let Err(e) = window.emit(
        "llm-stream",
        serde_json::json!({
            "done": true,
            "status": "cancelled"
        }),
    ) {
        warn!("Failed to emit cancellation event: {}", e);
    }
}

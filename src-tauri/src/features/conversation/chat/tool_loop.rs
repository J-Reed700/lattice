mod document_evidence;
mod document_progress;
mod scoped_document_tools;

use crate::application::ports::llm_port::CompletionInput;
use crate::application::ports::ToolDefinition;
use crate::features::conversation::chat::ports::ChatRuntime;
use crate::features::function_calling::dto::{
    FetchUrlContentOutput, WebSearchOutput, WikiSearchOutput, WikiSummaryOutput,
};
use crate::features::qa::dto::SourceDto;
use crate::features::settings::dto::ToolOutputSettingsDto;
use crate::shared::error::{AppError, Result};
use crate::shared::text::{build_excerpt, safe_truncate};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;
use std::time::Instant;
use tokio::time::timeout;
use tracing::{info, warn};

use super::fetch_memory::{self, Delivery, FetchMemory, Recall};
use super::focus::FocusScope;
use super::retrieval::{
    build_web_source_citations, deduplicate_sources, fetched_page_text_room, format_tool_result,
    merge_tool_sources, record_tool_document_references,
};
use super::turn::{ResearchRound, Rounds};
use super::turn_record::{TurnRecorder, TurnStepKind};
use super::web_steps;
use super::{ChatEventSink, ChatStreamEventDto};

#[derive(Debug, Serialize, Deserialize, Clone, Default, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ToolLoopTimingMetrics {
    pub total_ms: u64,
    pub iterations: u32,
    pub llm_stream_ms: u64,
    pub tool_execution_ms: u64,
    pub tool_call_count: u32,
    pub tool_success_count: u32,
    pub tool_failure_count: u32,
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

/// The request the turn assembled, as the loop sends its first round.
pub(super) struct LoopRequest<'a> {
    /// Policy, memory or history, and the turn's prompt, already planned
    /// against the window by the context assembler.
    pub(super) input: Vec<CompletionInput>,
    /// The folder tools are only here in an Explorer turn.
    pub(super) tools: Option<&'a [ToolDefinition]>,
    /// The generation reservation the plan made. Sent with every round so the
    /// provider holds exactly that back.
    pub(super) max_output_tokens: usize,
    /// What the request may occupy, in tokens, as tool results grow it.
    pub(super) input_budget: usize,
    /// One deadline for the whole turn: tool rounds and provider retries share it.
    pub(super) time_budget: Duration,
    /// Model rounds this turn may spend; see [`max_tool_rounds`].
    pub(super) max_tool_rounds: usize,
    /// A research turn saves every round it finishes here, and a resumed one
    /// starts after the last it saved.
    pub(super) research: Option<Rounds<'a>>,
}

/// One call the model asked for this round.
struct RequestedCall {
    id: String,
    name: String,
    arguments: serde_json::Value,
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
pub(super) struct StreamEmitter<'a> {
    sink: &'a ChatEventSink,
    conversation_id: String,
    request_id: String,
    /// Whether a terminal `done` has been emitted for this turn.
    done_sent: bool,
}

impl<'a> StreamEmitter<'a> {
    pub(super) fn new(sink: &'a ChatEventSink, conversation_id: &str, request_id: &str) -> Self {
        Self {
            sink,
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
        (self.sink)(payload)
    }
}

impl Drop for StreamEmitter<'_> {
    /// Guarantee a terminal event on **every** exit path, including the error
    /// returns scattered through the tool loop. Without this the UI stays
    /// stuck "generating" unless the outer invoke happens to reject.
    fn drop(&mut self) {
        self.done();
    }
}

// The tool loop is a turn-level orchestration boundary with explicit runtime inputs.
#[allow(clippy::too_many_arguments)]
pub(super) async fn run_agentic_tool_loop(
    container: &dyn ChatRuntime,
    conv_service: &Arc<dyn crate::features::conversation::ConversationServiceTrait>,
    conv_id: &str,
    request_id: &str,
    llm: &Arc<dyn crate::application::ports::LLMPort>,
    sink: &ChatEventSink,
    request: LoopRequest<'_>,
    highlight_terms: &[String],
    tool_output_settings: &ToolOutputSettingsDto,
    sources: &mut Vec<SourceDto>,
    retrieval_trace: &mut Option<super::RetrievalTraceDto>,
    pages_already_read: FetchMemory,
    focus: &FocusScope,
    // The folder of an Explorer conversation; `None` for every other turn. The
    // folder tools are only in `request.tools` when this is set.
    explorer: Option<&crate::features::explorer::prompt::ExplorerTurn>,
    recorder: &TurnRecorder,
) -> Result<ToolLoopOutcome> {
    use crate::application::ports::llm_port::{CompletionRequest, InferencePriority};

    let LoopRequest {
        mut input,
        tools: tools_ref,
        max_output_tokens,
        input_budget,
        time_budget,
        max_tool_rounds,
        research,
    } = request;
    let max_tool_rounds = max_tool_rounds.max(1);
    let (journal, start) = match research {
        Some(Rounds { journal, start }) => (Some(journal), start),
        None => (None, None),
    };
    // What the rounds add after the planned request is what a research turn
    // saves; a resumed one sends its saved rounds after the request again.
    let planned_len = input.len();
    let mut timings = ToolLoopTimingMetrics::default();
    let mut queries = Vec::new();
    let mut fetched = Vec::new();
    // The last round always answers, so a resumed turn has at least that one.
    let mut first_round = 0;
    if let Some(start) = start {
        input.extend(start.transcript);
        queries = start.queries;
        fetched = start.fetched;
        timings = start.timings;
        first_round = start.rounds.min(max_tool_rounds - 1);
    }
    let spent = Duration::from_millis(
        timings
            .llm_stream_ms
            .saturating_add(timings.tool_execution_ms),
    );
    // One deadline for the whole turn: tool rounds and provider retries share
    // it, and a resumed turn has only what its saved rounds left.
    let deadline = Instant::now() + time_budget.saturating_sub(spent);
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

    tracing::info!(
        conversation_id = conv_id,
        "chat_with_conversation: tool loop begin"
    );

    // Owns the terminal `done` event via Drop, so no error path can leave the
    // UI generating forever.
    let mut emitter = StreamEmitter::new(sink, conv_id, request_id);
    let mut document_progress = document_progress::DocumentProgress::new(sources);

    // The stop button as a token. Every round's request carries it, so a stop
    // press takes a request out of the backend's queue, or aborts it while it
    // generates, at once rather than at the next poll.
    let turn_cancel = super::cancellation::turn_token(request_id);
    let mut native_request = CompletionRequest {
        // Someone is watching this turn; it goes ahead of background work and
        // may take the slot kept free for interactive requests.
        priority: InferencePriority::Interactive,
        cancel: Some(turn_cancel.clone()),
        // Every round of a conversation shares its prefix with the last one.
        cache_key: Some(conv_id.to_string()),
        input,
        tools: tools_ref.unwrap_or(&[]).to_vec(),
        // The turn record can reveal provider-supplied reasoning on demand.
        // Providers that expose none simply leave the disclosure absent.
        include_reasoning: true,
        // Reserving output room and then not enforcing it is only bookkeeping:
        // the model could generate past what the budget set aside and overrun
        // the window the prompt was measured against.
        max_output_tokens: Some(u32::try_from(max_output_tokens).unwrap_or(u32::MAX))
            .filter(|budget| *budget > 0),
        ..Default::default()
    };
    let cancellation_error = super::cancellation::cancelled_error;

    // Pages are remembered for the whole turn, not just the round that found
    // them — and the turn started before this loop did: retrieval has usually
    // opened the top results already. Starting from its record is what stops
    // round one being spent asking for pages the prompt already carries.
    let mut fetch_memory = pages_already_read;
    // Reading this conversation's own transcript is not a vault read, so it does
    // not go through the space/focus scope path. The conversation id comes from
    // the turn, and `HistoryToolScope` has no setter, so no tool argument can
    // point it at another thread.
    let history_port = container
        .chat_records()
        .with_recall_embedding(container.get_or_load_embedding().await.ok());
    // One memo for the whole turn: a repeated read of a range already exhausted
    // answers with a reference instead of paying for the same text twice.
    let mut history_memo = super::history_tools::HistoryToolMemo::default();
    let mut document_evidence = document_evidence::DocumentEvidence::default();
    // The schemas ride along with every round and take room like any message.
    let tool_schema_chars = tools_ref.map_or(0, |tools| tool_schema_text(tools).chars().count());
    // Stop is honoured at this interval while a tool runs, as it is while the
    // model generates.
    let cancelled = || turn_cancel.cancelled();
    for iteration in first_round..max_tool_rounds {
        timings.iterations = (iteration + 1) as u32;
        // The last round is for answering. A model still calling tools here
        // would otherwise end the turn with an error and lose every page and
        // passage it had gathered, so it is offered no tools and told to
        // answer from what it has.
        // A window with no room left for another result is the last round too:
        // calls made now would come back cut to nothing.
        let window_full = iteration > 0
            && !window_has_room_for_a_result(
                window_chars(input_budget, llm.chars_per_token()),
                chars_in_flight(&native_request.input) + tool_schema_chars,
            );
        let final_round = iteration + 1 == max_tool_rounds || window_full;
        let tools_withdrawn = final_round && tools_ref.is_some_and(|tools| !tools.is_empty());
        if tools_withdrawn {
            info!(
                iteration,
                window_full, "Last tool round: asking for the answer without tools"
            );
            withdraw_tools_for_answer(&mut native_request);
        }
        // Offered no tools, a model can still write a call into its text. It is
        // never run, so it is held back from the bubble as it streams.
        let leaked_calls = std::sync::Mutex::new(LeakedCallFilter::new(tools_withdrawn));
        if turn_cancel.is_cancelled() {
            emit_cancelled_stream(sink, conv_id, request_id);
            return Err(cancellation_error());
        }
        info!(
            iteration = iteration,
            "Starting LLM generation (tool iteration {})", iteration
        );

        let llm_iteration_start = Instant::now();
        // Begun before the call so a round that never produces a token is
        // still a visible, timed step rather than a gap. The guard ends it even
        // on the budget and cancellation returns below.
        let generate_step =
            recorder.begin_guarded(TurnStepKind::Generate, thinking_label(iteration), None);
        let reasoning_progress = std::sync::Mutex::new(ReasoningProgress::new());
        let remaining = remaining_budget()?;
        native_request.time_budget = Some(remaining);
        let mut response = {
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
                let shown = leaked_calls
                    .lock()
                    .map_or_else(|_| text.clone(), |mut filter| filter.push(&text));
                if shown.is_empty() {
                    return Ok(());
                }
                emitter.content(&shown)
            };
            let on_reasoning = |delta: String| {
                let update = reasoning_progress
                    .lock()
                    .ok()
                    .and_then(|mut progress| progress.push(&delta));
                if let Some(reasoning) = update.as_deref() {
                    generate_step.set_reasoning(Some(reasoning));
                }
                Ok(())
            };
            // A provider-level retry is its own step. It used to overwrite
            // the answer bubble's text, so a turn that recovered ended up
            // showing "Model response failed. Retrying..." in place of the
            // answer it went on to write.
            let on_retry = |attempt: usize| {
                if let Ok(mut progress) = reasoning_progress.lock() {
                    progress.reset();
                }
                generate_step.set_reasoning(None);
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
                llm.complete_with_reasoning_progress(
                    &native_request,
                    &on_text,
                    &on_reasoning,
                    &on_retry,
                ),
            );
            tokio::select! {
                biased;
                _ = turn_cancel.cancelled() => return Err(cancellation_error()),
                result = completion => result.map_err(|_| budget_exhausted())?,
            }
        }?;
        if let Some(reasoning) = response
            .reasoning
            .as_deref()
            .map(str::trim)
            .filter(|text| !text.is_empty())
        {
            // Flush the last sub-threshold delta and make the persisted
            // step agree exactly with the provider's completed value.
            generate_step.set_reasoning(Some(reasoning));
        } else if let Ok(progress) = reasoning_progress.lock() {
            generate_step.set_reasoning(progress.text());
        }
        if answer_was_cut_short(
            &response.finish_reason,
            &response.text,
            !response.tool_calls.is_empty(),
        )? {
            emitter.content(CUT_SHORT_NOTE)?;
            response.text.push_str(CUT_SHORT_NOTE);
        }
        if response.text.trim().is_empty() && response.tool_calls.is_empty() {
            return Err(AppError::InvalidState(
                "Model returned no answer or tool calls".into(),
            ));
        }
        tracing::info!(input_tokens = response.input_tokens, output_tokens = response.output_tokens, finish_reason = %response.finish_reason, "Native LLM completion");
        // The round that produces the answer reports the turn's usage.
        let input_tokens = Some(response.input_tokens);
        let output_tokens = Some(response.output_tokens);
        // The model's turn goes back in the provider's own shape — its
        // tool calls, and any signed or opaque reasoning, included — so
        // the results below answer calls the provider recognises.
        native_request.input.append(&mut response.replay);
        let tool_calls = response
            .tool_calls
            .into_iter()
            .filter_map(|call| match call {
                CompletionInput::ToolCall {
                    id,
                    name,
                    arguments,
                } => Some(RequestedCall {
                    id,
                    name,
                    arguments,
                }),
                _ => None,
            })
            .collect::<Vec<_>>();
        let response_text = response.text;
        let tool_calls = if final_round && !tool_calls.is_empty() {
            // Offered no tools, a model can still write a call.
            // It is not run: this round's text is the answer.
            warn!(
                ignored_calls = tool_calls.len(),
                "Model called tools in the last round; keeping its text as the answer"
            );
            Vec::new()
        } else {
            tool_calls
        };
        // A call written as text is cut out of the answer, and
        // the answer says why it stops there rather than ending
        // mid-thought for no visible reason.
        let response_text = match tools_withdrawn
            .then(|| leaked_tool_call_start(&response_text))
            .flatten()
        {
            Some(start) => {
                warn!(
                    conversation_id = conv_id,
                    "Model wrote tool calls as text in the last round; cutting them from the answer"
                );
                let kept = response_text[..start].trim_end();
                emitter.content(OUT_OF_ROUNDS_NOTE)?;
                if kept.is_empty() {
                    OUT_OF_ROUNDS_NOTE.trim_start().to_string()
                } else {
                    format!("{kept}{OUT_OF_ROUNDS_NOTE}")
                }
            }
            None => response_text,
        };
        timings.llm_stream_ms = timings
            .llm_stream_ms
            .saturating_add(elapsed_ms(llm_iteration_start));
        generate_step.done(Some(round_result_line(&response_text, tool_calls.len())));
        if tool_calls.is_empty() {
            if response_text.trim().is_empty() {
                return Err(AppError::InvalidState("Model returned no answer".into()));
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
                window_chars(input_budget, llm.chars_per_token()),
                chars_in_flight(&native_request.input) + tool_schema_chars,
                tool_calls.len().saturating_sub(call_index),
                tool_output_settings.max_chars as usize,
            );
            let round_output_settings = ToolOutputSettingsDto {
                max_chars: u32::try_from(result_allowance).unwrap_or(u32::MAX),
                ..tool_output_settings.clone()
            };
            if turn_cancel.is_cancelled() {
                emit_cancelled_stream(sink, conv_id, request_id);
                return Err(cancellation_error());
            }
            // A call the model wrote but that cannot be run is
            // answered with why, and the model tries again.
            if let Some(problem) =
                crate::features::llm::llama_cpp::invalid_tool_call_problem(&tc.arguments)
            {
                warn!(
                    requested_function = tc.name.as_str(),
                    %problem,
                    "Model wrote a tool call that cannot be run"
                );
                timings.tool_failure_count = timings.tool_failure_count.saturating_add(1);
                recorder.note(
                    TurnStepKind::Retry,
                    "The model's tool call was malformed",
                    None,
                    Some(tc.name.clone()),
                );
                native_request.input.push(CompletionInput::ToolResult {
                    id: tc.id.clone(),
                    output: problem.clone(),
                });
                continue;
            }
            let resolved_tool = canonical_tool_name(tc.name.as_str());
            if !is_tool_allowed(resolved_tool, tools_ref) {
                native_request.input.push(CompletionInput::ToolResult {
                    id: tc.id.clone(),
                    output: "Requested tool is unavailable".into(),
                });
                let available_tools = available_tool_names(tools_ref);
                warn!(
                    requested_function = tc.name.as_str(),
                    resolved_function = resolved_tool,
                    available_tools = available_tools.join(", "),
                    "Skipping unavailable tool requested by LLM"
                );
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
                    timings.tool_failure_count = timings.tool_failure_count.saturating_add(1);
                    native_request.input.push(CompletionInput::ToolResult {
                        id: tc.id.clone(),
                        output: notice.clone(),
                    });
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
                        already_whole = matches!(recall, Recall::AlreadyWhole { .. }),
                        "Answered a repeat page request from this turn's memory"
                    );
                    timings.tool_success_count = timings.tool_success_count.saturating_add(1);
                    native_request.input.push(CompletionInput::ToolResult {
                        id: tc.id.clone(),
                        output: notice.clone(),
                    });
                    timings.tool_execution_ms = timings
                        .tool_execution_ms
                        .saturating_add(elapsed_ms(tool_call_start));
                    continue;
                }
            }
            if let Some(query) = tc
                .arguments
                .get("query")
                .and_then(serde_json::Value::as_str)
                .map(str::trim)
                .filter(|query| !query.is_empty())
            {
                queries.push(query.to_string());
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
            let explorer_turn = explorer
                .filter(|_| crate::features::explorer::tools::is_explorer_tool(resolved_tool));
            let execution = async {
                if let Some(turn) = explorer_turn {
                    // Cut to the same allowance as every other
                    // result, at a line boundary, with a note
                    // saying where to read on.
                    turn.execute_tool(call, result_allowance).await
                } else if super::history_tools::is_history_tool(resolved_tool) {
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
                        history_port.as_ref(),
                        &scope,
                        &mut history_memo,
                        call,
                    )
                    .await
                } else if resolved_tool == "fetch_url_content" {
                    // A page this conversation already read is served
                    // from its permanent archive — the citation's
                    // snapshot, not another network request.
                    match fetch_memory::fetch_target(&tc.arguments) {
                        Some(url) => match super::source_snapshots::archived_page(
                            container, conv_id, url,
                        )
                        .await
                        {
                            Some(page) => Ok(
                                crate::features::function_calling::domain::FunctionResult::success(
                                    serde_json::to_value(page).unwrap_or_default(),
                                ),
                            ),
                            None => {
                                scoped_document_tools::execute(container, conv_id, focus, call)
                                    .await
                            }
                        },
                        None => {
                            scoped_document_tools::execute(container, conv_id, focus, call).await
                        }
                    }
                } else {
                    scoped_document_tools::execute(container, conv_id, focus, call).await
                }
            };
            // A fetch can sit in a site's queue, then its HTTP
            // timeout, then the browser fallback. Stop and the
            // turn's deadline must not wait for all three.
            let executed = match run_tool_bounded(execution, remaining_budget()?, cancelled()).await
            {
                ToolRun::Finished(result) => result,
                ToolRun::Cancelled => {
                    tool_step.failed(Some("Stopped".into()));
                    emit_cancelled_stream(sink, conv_id, request_id);
                    return Err(cancellation_error());
                }
                ToolRun::TimedOut => {
                    tool_step.failed(Some("Out of time".into()));
                    return Err(budget_exhausted());
                }
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
                    if result.success {
                        timings.tool_success_count = timings.tool_success_count.saturating_add(1);
                    } else {
                        timings.tool_failure_count = timings.tool_failure_count.saturating_add(1);
                    }
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
                        let trace =
                            retrieval_trace.get_or_insert_with(|| super::RetrievalTraceDto {
                                scope: "vault".to_string(),
                                ..Default::default()
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
                    result_text = document_evidence.render(
                                    resolved_tool,
                                    &result,
                                    result_text,
                                    |excerpt| {
                                        native_request.input.iter().any(|item| matches!(item,
                                            CompletionInput::ToolResult { output, .. } if output == excerpt))
                                    },
                                );
                    native_request.input.push(CompletionInput::ToolResult {
                        id: tc.id.clone(),
                        output: result_text.clone(),
                    });
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

                    if result.success && resolved_tool == "fetch_url_content" {
                        if let (Some(url), Some(page)) = (
                            fetch_memory::fetch_target(&tc.arguments),
                            result.data.clone().and_then(|data| {
                                serde_json::from_value::<FetchUrlContentOutput>(data).ok()
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
                            fetched.push(url.to_string());
                            // A live read becomes the
                            // conversation's permanent archive;
                            // an archived one already is.
                            super::source_snapshots::archive_page_for_url(
                                container, conv_id, url, &page,
                            )
                            .await;
                        }
                    }
                    if !result.success {
                        if let Some(url) = fetch_memory::fetch_target(&tc.arguments) {
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
                    timings.tool_failure_count = timings.tool_failure_count.saturating_add(1);
                    warn!(
                        requested_function = tc.name.as_str(),
                        resolved_function = resolved_tool,
                        error = %e,
                        "Tool call failed"
                    );
                    native_request.input.push(CompletionInput::ToolResult {
                        id: tc.id.clone(),
                        output: format!("Tool failed: {e}"),
                    });
                    if let Some(url) = fetch_memory::fetch_target(&tc.arguments) {
                        fetch_memory.record_failure(url, &e.to_string());
                    }
                }
            }
            timings.tool_execution_ms = timings
                .tool_execution_ms
                .saturating_add(elapsed_ms(tool_call_start));
        }
        if let Some(journal) = journal {
            journal
                .round(&ResearchRound {
                    completed: iteration + 1,
                    transcript: native_request
                        .input
                        .get(planned_len..)
                        .map(<[CompletionInput]>::to_vec)
                        .unwrap_or_default(),
                    queries: queries.clone(),
                    fetched: fetched.clone(),
                    sources: sources.clone(),
                    retrieval_trace: retrieval_trace.clone(),
                    pages_read: fetch_memory.clone(),
                    steps: recorder.steps(),
                    timings: timings.clone(),
                    elapsed_ms: recorder.elapsed_ms(),
                })
                .await?;
        }
    }

    warn!(
        conversation_id = conv_id,
        iterations = max_tool_rounds,
        llm_stream_ms = timings.llm_stream_ms,
        tool_execution_ms = timings.tool_execution_ms,
        tool_call_count = timings.tool_call_count,
        tool_success_count = timings.tool_success_count,
        tool_failure_count = timings.tool_failure_count,
        total_ms = elapsed_ms(tool_loop_start),
        "Tool calling loop exhausted iterations"
    );
    emitter.done();
    Err(AppError::Other(
        "Tool calling loop exceeded maximum iterations without producing a final response".into(),
    ))
}

/// How a tool call raced against Stop and the turn's deadline ended.
enum ToolRun<T> {
    Finished(T),
    Cancelled,
    TimedOut,
}

/// Run one tool call, giving up when the turn is stopped or out of time.
async fn run_tool_bounded<T>(
    tool: impl std::future::Future<Output = T>,
    remaining: Duration,
    cancelled: impl std::future::Future<Output = ()>,
) -> ToolRun<T> {
    tokio::select! {
        finished = timeout(remaining, tool) => match finished {
            Ok(result) => ToolRun::Finished(result),
            Err(_) => ToolRun::TimedOut,
        },
        () = cancelled => ToolRun::Cancelled,
    }
}

/// Said to the model on the last round, when it is offered no more tools.
const FINAL_ROUND_INSTRUCTION: &str = "You have used every tool round this turn allows. No tools are available now, so do not write any tool calls. Answer my question now from what you have already gathered. If you could not read enough to answer fully, say what you covered and what is left, so I can ask you to continue.";

/// Turn the next round into the answering round: no tools on offer, and an
/// instruction to answer from what the turn already holds.
///
/// The instruction is the user's last message, not a system one: llama.cpp
/// chat templates take one system message, so a late one is merged into the
/// top of the prompt, tens of thousands of tokens before the model reads its
/// last tool result. From there it was not seen, and the model went on
/// writing calls as text.
fn withdraw_tools_for_answer(request: &mut crate::application::ports::llm_port::CompletionRequest) {
    request.tools.clear();
    request.input.push(
        crate::application::ports::llm_port::CompletionInput::Message {
            role: "user".into(),
            content: FINAL_ROUND_INSTRUCTION.into(),
        },
    );
}

/// Model rounds a turn may spend on tool calls, the last of which is kept for
/// the answer. Deep research has a two-hour budget and branches across many
/// sources; five rounds would end it long before that budget does. An
/// Explorer turn reads a folder file by file, two or three a round, so five
/// rounds ended a walk through a codebase before it reached the answer.
pub(super) fn max_tool_rounds(deep_research: bool, explorer: bool) -> usize {
    if deep_research {
        12
    } else if explorer {
        10
    } else {
        5
    }
}

/// Appended where the last round's answer stopped to write a tool call it
/// could not make. One sentence, so the grounding check can know it whole.
pub(in crate::features::conversation::chat) const OUT_OF_ROUNDS_NOTE: &str =
    "\n\n_(Stopped here: the model used every tool round this turn allows before it finished — say \"continue\" to let it keep going.)_";

/// How tool calls look when a model writes them as text: Qwen and Hermes
/// `<tool_call>`, the XML `<function=` form, Mistral's `[TOOL_CALLS]`, and
/// the `<|tool_call` special-token spelling.
const LEAKED_CALL_MARKERS: [&str; 4] = ["<tool_call>", "<function=", "[TOOL_CALLS]", "<|tool_call"];

/// Where the first tool call written as text begins, if there is one.
fn leaked_tool_call_start(text: &str) -> Option<usize> {
    LEAKED_CALL_MARKERS
        .iter()
        .filter_map(|marker| text.find(marker))
        .min()
}

/// Streams the last round's text up to the first tool call written into it,
/// and nothing after. A chunk can end partway through a marker, so a tail
/// that could still become one is held until the next chunk settles it.
struct LeakedCallFilter {
    active: bool,
    held: String,
    stopped: bool,
}

impl LeakedCallFilter {
    fn new(active: bool) -> Self {
        Self {
            active,
            held: String::new(),
            stopped: false,
        }
    }

    /// The part of `chunk` that may be shown now.
    fn push(&mut self, chunk: &str) -> String {
        if !self.active {
            return chunk.to_string();
        }
        if self.stopped {
            return String::new();
        }
        self.held.push_str(chunk);
        if let Some(start) = leaked_tool_call_start(&self.held) {
            self.stopped = true;
            let shown = self.held[..start].to_string();
            self.held.clear();
            return shown;
        }
        let keep_from = self.held.len() - partial_marker_suffix_len(&self.held);
        let shown = self.held[..keep_from].to_string();
        self.held.drain(..keep_from);
        shown
    }
}

/// Length of the longest tail of `text` that is the start of a marker.
fn partial_marker_suffix_len(text: &str) -> usize {
    LEAKED_CALL_MARKERS
        .iter()
        .flat_map(|marker| (1..marker.len()).rev().map(move |len| &marker[..len]))
        .filter(|prefix| text.ends_with(prefix))
        .map(str::len)
        .max()
        .unwrap_or(0)
}

/// Whether another tool result could still fit in the window, at the size
/// below which a result says too little to be worth a round. A provider that
/// reports no window is never treated as full.
fn window_has_room_for_a_result(window_chars: usize, chars_in_flight: usize) -> bool {
    if window_chars == 0 {
        return true;
    }
    window_chars.saturating_sub(chars_in_flight) >= MIN_TOOL_RESULT_CHARS
}

/// The characters the prompt may fill: the plan's input budget, converted at
/// the backend's calibrated characters per token and erring short, so dense
/// text under-fills rather than overruns.
fn window_chars(input_budget: usize, chars_per_token: f64) -> usize {
    crate::application::ports::llm_port::chars_within_tokens(input_budget, chars_per_token)
}

/// The tool definitions as the provider receives them, for budgeting: the
/// JSON `parameters` of each tool are sent and read like any other prompt text.
pub(super) fn tool_schema_text(tools: &[crate::application::ports::ToolDefinition]) -> String {
    crate::features::llm::llama_cpp::tool_specs(tools).to_string()
}

fn elapsed_ms(start: Instant) -> u64 {
    u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX)
}

/// Preview length for a page the model fetched itself, matching the preview a
/// vault passage gets.
const FETCHED_PAGE_EXCERPT_CHARS: usize = 480;

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
                    // The card's short preview, not the page: the UI refuses an
                    // excerpt over 2000 characters, and a whole-page excerpt here
                    // dropped the source — its `[n]` chip went dead and it was
                    // missing from the list under the answer.
                    excerpt: Some(build_excerpt(
                        &output.content,
                        highlight_terms,
                        FETCHED_PAGE_EXCERPT_CHARS,
                    )),
                    highlights,
                    section: None,
                    chunk_index: Some(1),
                    chunk_excerpts: None,
                    citation_id: None,

                    web_snapshot: None,
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

                    web_snapshot: None,
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

/// Appended to an answer the model stopped writing at its output limit. The
/// grounding check knows it by this text: it is the app's line, not a claim.
pub(in crate::features::conversation::chat) const CUT_SHORT_NOTE: &str =
    "\n\n_(The answer was cut short: the model reached its output limit.)_";

/// Whether a round's answer stopped at the output limit, or an error when
/// the round has nothing worth keeping.
///
/// A cut-short answer is still an answer: failing the turn threw away
/// everything the user had watched stream in. A cut-short tool round has
/// nothing to keep — its calls are truncated — so it still fails.
fn answer_was_cut_short(finish_reason: &str, text: &str, has_tool_calls: bool) -> Result<bool> {
    let cut_short = matches!(finish_reason, "incomplete" | "max_tokens" | "length");
    if finish_reason == "failed" || (cut_short && (text.trim().is_empty() || has_tool_calls)) {
        return Err(AppError::InvalidState(format!(
            "Model stopped before completion: {finish_reason}"
        )));
    }
    Ok(cut_short)
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

/// Live reasoning is a progress surface, not an unbounded transcript. Batch
/// token-sized deltas before emitting a replacement step so React does not
/// rerender once per token, then let the completed provider value replace this
/// capture before the step is persisted.
struct ReasoningProgress {
    text: String,
    chars: usize,
    last_sent_chars: usize,
    last_sent_at: Instant,
}

impl ReasoningProgress {
    const MAX_CHARS: usize = 24_000;
    const MIN_EMIT_CHARS: usize = 160;
    const MAX_EMIT_DELAY: Duration = Duration::from_millis(120);

    fn new() -> Self {
        Self {
            text: String::new(),
            chars: 0,
            last_sent_chars: 0,
            last_sent_at: Instant::now(),
        }
    }

    fn push(&mut self, delta: &str) -> Option<String> {
        let remaining = Self::MAX_CHARS.saturating_sub(self.chars);
        if remaining > 0 {
            let kept = crate::shared::text::safe_truncate(delta, remaining);
            self.chars = self.chars.saturating_add(kept.chars().count());
            self.text.push_str(&kept);
        }
        let should_emit = self.last_sent_chars == 0
            || self.chars.saturating_sub(self.last_sent_chars) >= Self::MIN_EMIT_CHARS
            || self.last_sent_at.elapsed() >= Self::MAX_EMIT_DELAY;
        if !should_emit || self.text.trim().is_empty() {
            return None;
        }
        self.last_sent_chars = self.chars;
        self.last_sent_at = Instant::now();
        Some(self.text.clone())
    }

    fn reset(&mut self) {
        self.text.clear();
        self.chars = 0;
        self.last_sent_chars = 0;
        self.last_sent_at = Instant::now();
    }

    fn text(&self) -> Option<&str> {
        (!self.text.trim().is_empty()).then_some(self.text.as_str())
    }
}

/// Which kind of step a tool call is, so the timeline groups a document search
/// with the retrieval pipeline's rather than filing it under "tool".
fn tool_step_kind(tool: &str) -> TurnStepKind {
    match tool {
        "semantic_search" => TurnStepKind::SearchDocuments,
        "get_document" | "list_documents" | "list_attachments" => TurnStepKind::OpenDocument,
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
    const INTERESTING: [&str; 6] = ["query", "url", "document_id", "page", "path", "pattern"];
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

/// A result cut shorter than this says too little to have been worth the round
/// that asked for it, so a nearly full window still gets this much.
const MIN_TOOL_RESULT_CHARS: usize = 1_500;

/// Roughly how many characters the next generation already has to read.
fn chars_in_flight(input: &[crate::application::ports::llm_port::CompletionInput]) -> usize {
    use crate::application::ports::llm_port::CompletionInput;
    input
        .iter()
        .map(|item| match item {
            CompletionInput::Message { content, .. } => content.chars().count(),
            CompletionInput::ToolResult { output, .. } => output.chars().count(),
            CompletionInput::ToolCall {
                name, arguments, ..
            } => name.len() + arguments.to_string().len(),
            CompletionInput::Native { value } => value.to_string().len(),
        })
        .sum()
}

/// How many characters one tool result may take, given the model's window,
/// what is already in it, and how many results this round has still to fit.
fn tool_result_allowance(
    window_chars: usize,
    chars_in_flight: usize,
    calls_left: usize,
    configured_max: usize,
) -> usize {
    let room = window_chars.saturating_sub(chars_in_flight);
    let share = room / calls_left.max(1);
    // The floor is only kept while the window can still take it. Past that, a
    // result is given its share of what is left and no more: overrunning the
    // window fails the next round outright, which is worse than a short result.
    let floor = MIN_TOOL_RESULT_CHARS.min(configured_max);
    if room < floor {
        return share.min(configured_max);
    }
    share.clamp(floor, configured_max)
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
        "list_attachments" => "Checking what you attached".to_string(),
        "get_document" => "Opening a document".to_string(),
        "search_conversation_history" | "read_conversation_history" => {
            "Looking back through this conversation".to_string()
        }
        other if crate::features::explorer::tools::is_explorer_tool(other) => {
            crate::features::explorer::tools::activity_label(other, arguments)
        }
        other => format!("Running {other}"),
    }
}

fn emit_cancelled_stream(sink: &ChatEventSink, conversation_id: &str, request_id: &str) {
    if let Err(e) = sink(ChatStreamEventDto {
        done: true,
        status: Some("cancelled".to_owned()),
        ..ChatStreamEventDto::new(conversation_id, request_id)
    }) {
        warn!("Failed to emit cancellation event: {}", e);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::ports::llm_port::DEFAULT_CHARS_PER_TOKEN;

    /// The input budget the context assembler plans for a model of this size.
    fn input_budget(context_tokens: usize) -> usize {
        use crate::application::services::context_assembler::{
            BudgetAllocation, BudgetRequest, ModelCapacity,
        };
        BudgetAllocation::plan(
            &ModelCapacity::new("test-model", context_tokens),
            &BudgetRequest::default(),
        )
        .unwrap()
        .input_budget
    }

    #[test]
    fn stream_sink_keeps_turn_identity_and_emits_completion_once() {
        let events = Arc::new(std::sync::Mutex::new(Vec::new()));
        let captured = Arc::clone(&events);
        let sink: ChatEventSink = Arc::new(move |event| {
            captured.lock().unwrap().push(event);
            Ok(())
        });
        {
            let mut stream = StreamEmitter::new(&sink, "conversation", "request");
            stream.content("hello").unwrap();
            stream.done();
            stream.done();
        }
        let events = events.lock().unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].content.as_deref(), Some("hello"));
        assert!(events[1].done);
        assert!(events
            .iter()
            .all(|event| event.conversation_id == "conversation" && event.request_id == "request"));
    }

    #[test]
    fn stream_drop_reports_completion_after_early_return() {
        let events = Arc::new(std::sync::Mutex::new(Vec::new()));
        let captured = Arc::clone(&events);
        let sink: ChatEventSink = Arc::new(move |event| {
            captured.lock().unwrap().push(event);
            Ok(())
        });
        drop(StreamEmitter::new(&sink, "conversation", "request"));
        let events = events.lock().unwrap();
        assert_eq!(events.len(), 1);
        assert!(events[0].done);
    }

    #[test]
    fn an_answer_stopped_at_the_output_limit_is_kept_not_failed() {
        for reason in ["length", "max_tokens", "incomplete"] {
            assert!(answer_was_cut_short(reason, "half an answer", false).unwrap());
            // Nothing written, or a tool call cut mid-arguments: nothing to keep.
            assert!(answer_was_cut_short(reason, "  ", false).is_err());
            assert!(answer_was_cut_short(reason, "", true).is_err());
        }
        assert!(answer_was_cut_short("failed", "", false).is_err());
        assert!(!answer_was_cut_short("stop", "done", false).unwrap());
        assert!(!answer_was_cut_short("tool_calls", "", true).unwrap());
    }

    /// Four page fetches in one round on a 32k-token model. At the configured
    /// 50,000 characters each they are ~200,000 characters — several times the
    /// model's whole window — and the next generation simply failed.
    #[test]
    fn a_round_of_results_is_made_to_fit_a_small_window() {
        let context_tokens = 32_768;
        let window = window_chars(input_budget(context_tokens), DEFAULT_CHARS_PER_TOKEN);
        let already = 20_000;

        let mut spent = already;
        for calls_left in (1..=4).rev() {
            spent += tool_result_allowance(window, spent, calls_left, 50_000);
        }
        assert!(
            spent <= window,
            "{spent} characters in a {window}-character window"
        );
    }

    /// The model in the log has a 131k window. It should get whole pages.
    #[test]
    fn a_large_window_gives_each_result_the_configured_maximum() {
        assert_eq!(
            tool_result_allowance(
                window_chars(input_budget(131_072), DEFAULT_CHARS_PER_TOKEN),
                20_000,
                4,
                50_000
            ),
            50_000
        );
    }

    /// Past a full window, the floor would overrun it and fail the next round.
    #[test]
    fn a_full_window_gives_a_result_only_what_is_left() {
        assert_eq!(
            tool_result_allowance(
                window_chars(input_budget(8_192), DEFAULT_CHARS_PER_TOKEN),
                1_000_000,
                3,
                50_000
            ),
            0
        );
        let window = window_chars(input_budget(8_192), DEFAULT_CHARS_PER_TOKEN);
        let nearly_full = window - 900;
        assert_eq!(
            tool_result_allowance(
                window_chars(input_budget(8_192), DEFAULT_CHARS_PER_TOKEN),
                nearly_full,
                1,
                50_000
            ),
            900
        );
    }

    /// With room, a result still gets enough to be worth the round.
    #[test]
    fn a_window_with_room_keeps_the_floor() {
        let window = window_chars(input_budget(8_192), DEFAULT_CHARS_PER_TOKEN);
        assert_eq!(
            tool_result_allowance(
                window_chars(input_budget(8_192), DEFAULT_CHARS_PER_TOKEN),
                window - 4_000,
                3,
                50_000
            ),
            MIN_TOOL_RESULT_CHARS
        );
    }

    /// On a 4k-token local window the schemas alone are a large share, and a
    /// round that ignored them handed out room the window did not have.
    #[test]
    fn tool_schemas_are_charged_against_a_small_window() {
        let tools = vec![crate::application::ports::ToolDefinition {
            name: "web_search".into(),
            description: "Search the web.".into(),
            parameters: serde_json::json!({"type":"object","properties":{
                "query":{"type":"string","description":"x".repeat(3_000)}}}),
        }];
        let schema_chars = tool_schema_text(&tools).chars().count();
        assert!(
            schema_chars > 3_000,
            "parameters must be counted: {schema_chars}"
        );

        let window = window_chars(input_budget(4_096), DEFAULT_CHARS_PER_TOKEN);
        let prompt = vec![CompletionInput::Message {
            role: "user".into(),
            content: "p".repeat(window - schema_chars - 500),
        }];
        let in_flight = chars_in_flight(&prompt) + schema_chars;
        let allowance = tool_result_allowance(
            window_chars(input_budget(4_096), DEFAULT_CHARS_PER_TOKEN),
            in_flight,
            1,
            50_000,
        );
        assert_eq!(allowance, 500);
        assert!(in_flight + allowance <= window);
    }

    #[test]
    fn the_last_round_offers_no_tools_and_asks_for_the_answer() {
        use crate::application::ports::llm_port::CompletionRequest;
        let mut request = CompletionRequest {
            input: vec![CompletionInput::Message {
                role: "user".into(),
                content: "q".into(),
            }],
            tools: vec![crate::application::ports::ToolDefinition {
                name: "web_search".into(),
                description: "Search".into(),
                parameters: serde_json::json!({}),
            }],
            ..Default::default()
        };
        withdraw_tools_for_answer(&mut request);
        assert!(request.tools.is_empty());
        assert!(matches!(request.input.last(),
            Some(CompletionInput::Message { content, .. }) if content == FINAL_ROUND_INSTRUCTION));
    }

    #[test]
    fn deep_research_gets_more_tool_rounds() {
        assert!(max_tool_rounds(true, false) > max_tool_rounds(false, false));
        assert!(
            max_tool_rounds(false, false) >= 2,
            "one round to search, one to answer"
        );
    }

    #[test]
    fn an_explorer_turn_gets_more_rounds_than_a_plain_chat() {
        assert!(max_tool_rounds(false, true) > max_tool_rounds(false, false));
        assert!(max_tool_rounds(true, true) >= max_tool_rounds(false, true));
    }

    /// The instruction has to be the last thing the model reads. As a system
    /// message, llama.cpp merged it into the top of a 27,000-token prompt.
    #[test]
    fn the_answer_instruction_is_the_last_user_message() {
        use crate::application::ports::llm_port::CompletionRequest;
        let mut request = CompletionRequest {
            input: vec![
                CompletionInput::Message {
                    role: "system".into(),
                    content: "You are helpful.".into(),
                },
                CompletionInput::ToolResult {
                    id: "c1".into(),
                    output: "file text".into(),
                },
            ],
            ..Default::default()
        };
        withdraw_tools_for_answer(&mut request);
        assert!(matches!(request.input.last(),
            Some(CompletionInput::Message { role, .. }) if role == "user"));
    }

    #[test]
    fn a_tool_call_written_as_text_is_found() {
        let text = "Now the engine.\n\n<tool_call> <function=list_directory> <parameter=path> src </parameter> </function> </tool_call>";
        assert_eq!(leaked_tool_call_start(text), text.find("<tool_call>"));
        assert_eq!(
            leaked_tool_call_start("<function=read_file>"),
            Some(0),
            "the XML form without the wrapper"
        );
        assert_eq!(leaked_tool_call_start("Plain answer with a < b."), None);
    }

    #[test]
    fn the_stream_stops_at_a_call_split_across_chunks() {
        let mut filter = LeakedCallFilter::new(true);
        let mut shown = String::new();
        for chunk in [
            "Now the engine. <to",
            "ol_c",
            "all> <function=list_directory>",
            " more",
        ] {
            shown.push_str(&filter.push(chunk));
        }
        assert_eq!(shown, "Now the engine. ");
    }

    #[test]
    fn a_lone_angle_bracket_is_released_once_it_is_not_a_call() {
        let mut filter = LeakedCallFilter::new(true);
        let first = filter.push("if a <");
        let second = filter.push(" b then");
        assert_eq!(format!("{first}{second}"), "if a < b then");
    }

    #[test]
    fn rounds_with_tools_stream_untouched() {
        let mut filter = LeakedCallFilter::new(false);
        assert_eq!(filter.push("<tool_call>"), "<tool_call>");
    }

    #[test]
    fn a_full_window_has_no_room_for_another_result() {
        let window = window_chars(input_budget(8_192), DEFAULT_CHARS_PER_TOKEN);
        assert!(window_has_room_for_a_result(
            window_chars(input_budget(8_192), DEFAULT_CHARS_PER_TOKEN),
            0
        ));
        assert!(!window_has_room_for_a_result(
            window,
            window - MIN_TOOL_RESULT_CHARS + 1
        ));
        assert!(
            window_has_room_for_a_result(window_chars(0, DEFAULT_CHARS_PER_TOKEN), usize::MAX),
            "no window reported"
        );
    }

    /// Stop is honoured while a tool is still running, not after it returns.
    #[tokio::test]
    async fn a_stopped_turn_does_not_wait_for_a_pending_tool() {
        let started = Instant::now();
        let outcome = run_tool_bounded(
            std::future::pending::<()>(),
            Duration::from_secs(600),
            tokio::time::sleep(Duration::from_millis(20)),
        )
        .await;
        assert!(matches!(outcome, ToolRun::Cancelled));
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    #[tokio::test]
    async fn a_tool_that_outlives_the_turn_is_cut_off_at_the_deadline() {
        let outcome = run_tool_bounded(
            std::future::pending::<()>(),
            Duration::from_millis(20),
            std::future::pending::<()>(),
        )
        .await;
        assert!(matches!(outcome, ToolRun::TimedOut));
        let outcome = run_tool_bounded(
            async { 7 },
            Duration::from_secs(1),
            std::future::pending::<()>(),
        )
        .await;
        assert!(matches!(outcome, ToolRun::Finished(7)));
    }

    /// A malformed call reaches the loop as a call whose reply is the error,
    /// quoting what the model wrote, so the next round can correct it.
    #[test]
    fn a_malformed_call_is_answered_with_the_error() {
        let response = crate::features::llm::llama_cpp::parse_completion(serde_json::json!({
            "choices":[{"message":{"content":"Searching.","tool_calls":[{"id":"c1","type":"function",
                "function":{"name":"web_search","arguments":"{query: rust"}}]},
                "finish_reason":"tool_calls"}]}))
        .unwrap();
        assert_eq!(response.text, "Searching.");
        let Some(CompletionInput::ToolCall { arguments, .. }) = response.tool_calls.first() else {
            panic!("the malformed call must be kept");
        };
        let reply = crate::features::llm::llama_cpp::invalid_tool_call_problem(arguments)
            .expect("marked invalid");
        assert!(reply.contains("not valid JSON"), "{reply}");
        assert!(reply.contains("{query: rust"), "{reply}");
    }

    #[test]
    fn a_configured_maximum_below_the_floor_is_respected() {
        assert_eq!(
            tool_result_allowance(
                window_chars(input_budget(131_072), DEFAULT_CHARS_PER_TOKEN),
                0,
                1,
                800
            ),
            800
        );
    }

    #[test]
    fn what_is_in_flight_counts_every_item_the_next_round_sends() {
        let input = vec![
            CompletionInput::Message {
                role: "user".to_string(),
                content: "x".repeat(100),
            },
            CompletionInput::ToolResult {
                id: "1".to_string(),
                output: "y".repeat(400),
            },
        ];
        assert_eq!(chars_in_flight(&input), 500);
        assert_eq!(chars_in_flight(&[]), 0);
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

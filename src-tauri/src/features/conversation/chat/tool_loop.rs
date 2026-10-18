mod call_labels;
mod calls;
mod clock;
mod document_evidence;
mod document_progress;
mod final_round;
mod generation;
mod images;
#[cfg(test)]
mod loop_tests;
mod research_notes;
mod scoped_document_tools;
mod tool_sources;

use crate::features::conversation::chat::ports::ChatRuntime;
use lattice_core::application::contracts::settings::ToolOutputSettingsDto;
use lattice_core::application::contracts::sources::SourceDto;
use lattice_core::application::ports::llm_port::{CompletionInput, ThinkingMode};
use lattice_core::application::ports::ToolDefinition;
use lattice_core::shared::error::{AppError, Result};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use std::time::Instant;
use tracing::{info, warn};

use super::fetch_memory::FetchMemory;
use super::turn::{ResearchRound, Rounds};
use super::turn_record::TurnStepKind;
use super::{ChatEventSink, ChatStreamEventDto, ChatTurn};

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

pub(super) async fn run_agentic_tool_loop(
    container: &dyn ChatRuntime,
    turn: ChatTurn<'_>,
    sink: &ChatEventSink,
    request: LoopRequest<'_>,
    tool_output_settings: &ToolOutputSettingsDto,
    sources: &mut Vec<SourceDto>,
    retrieval_trace: &mut Option<super::RetrievalTraceDto>,
    pages_already_read: FetchMemory,
    // The folder of an Explorer conversation; `None` for every other turn. The
    // folder tools are only in `request.tools` when this is set.
    explorer: Option<&super::ExplorerTurn>,
) -> Result<ToolLoopOutcome> {
    use lattice_core::application::ports::llm_port::{CompletionRequest, InferencePriority};

    let ChatTurn {
        conv_service,
        conversation_id: conv_id,
        request_id,
        llm,
        highlight_terms,
        focus,
        recorder,
        ..
    } = turn;
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
    // One deadline for the whole turn: tool rounds, condensing and provider
    // retries share it, and a resumed turn has only what its saved rounds left.
    let clock = clock::TurnClock::new(time_budget, spent);

    let tool_loop_start = Instant::now();

    tracing::info!(
        conversation_id = conv_id,
        "chat_with_conversation: tool loop begin"
    );

    // Owns the terminal `done` event via Drop, so no error path can leave the
    // UI generating forever.
    let mut emitter = StreamEmitter::new(sink, conv_id, request_id);

    // The stop button as a token. Every round's request carries it, so a stop
    // press takes a request out of the backend's queue, or aborts it while it
    // generates, at once rather than at the next poll.
    let turn_cancel = container.turn_cancellation().turn_token(request_id);
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
    // Whether the user lets the answer think. A round that can still call a
    // tool never does (see `ThinkingMode`).
    let answer_thinking = container
        .settings()
        .await
        .map(|settings| settings.llm.answer_thinking)
        .unwrap_or(false);
    let window = window_chars(input_budget, llm.chars_per_token());
    // The schemas ride along with every round and take room like any message.
    let tool_schema_chars = tools_ref.map_or(0, |tools| tool_schema_text(tools).chars().count());
    let mut calls = calls::ToolCalls::new(
        calls::CallScope {
            container,
            conv_service,
            conv_id,
            request_id,
            sink,
            tools: tools_ref,
            highlight_terms,
            tool_output_settings,
            focus,
            explorer,
            recorder,
            history: container
                .chat_records()
                .with_recall_embedding(container.get_or_load_embedding().await.ok()),
            cancel: &turn_cancel,
            clock: &clock,
            window_chars: window,
            tool_schema_chars,
        },
        // Pages are remembered for the whole turn — and the turn started
        // before this loop did: retrieval has usually opened the top results
        // already.
        pages_already_read,
        queries,
        fetched,
        sources,
    );
    let mut condensed = 0;
    for iteration in first_round..max_tool_rounds {
        timings.iterations = (iteration + 1) as u32;
        let out_of_rounds = iteration + 1 == max_tool_rounds;
        // Before the window can be judged full: results that have outgrown
        // their share are condensed into notes and the research goes on. The
        // full-window ending below is the last resort.
        if iteration > 0
            && !out_of_rounds
            && research_notes::should_condense(
                window,
                &native_request.input,
                planned_len,
                condensed,
            )
        {
            condensed += 1;
            let condense_start = Instant::now();
            if let Some(notes) = research_notes::condense(
                llm.as_ref(),
                &mut native_request,
                planned_len,
                recorder,
                &turn_cancel,
                &clock,
            )
            .await?
            {
                info!(
                    iteration,
                    chars_before = notes.chars_before,
                    chars_after = notes.chars_after,
                    "Condensed earlier tool results to keep researching"
                );
                calls.results_condensed(&notes.result_ids);
            }
            timings.llm_stream_ms = timings
                .llm_stream_ms
                .saturating_add(elapsed_ms(condense_start));
        }
        // A window with no room left for another result is the last round too:
        // calls made now would come back cut to nothing.
        let window_full = iteration > 0
            && !window_has_room_for_a_result(
                window,
                chars_in_flight(&native_request.input) + tool_schema_chars,
            );
        let last_round = out_of_rounds || window_full;
        // The last round is for answering. A model still calling tools here
        // would otherwise end the turn with an error and lose every page and
        // passage it had gathered, so it may call none and is told to answer
        // from what it has.
        let answer_only = last_round && tools_ref.is_some_and(|tools| !tools.is_empty());
        if answer_only {
            info!(
                iteration,
                window_full, "Last tool round: asking for the answer with calls forbidden"
            );
            if answer_thinking {
                // Thinking with the tools still in the prompt wrote the call
                // as text and answered nothing, so this answer has none.
                final_round::ask_for_answer_without_tools(&mut native_request);
            } else {
                final_round::ask_for_answer(&mut native_request);
            }
        }
        native_request.thinking = ThinkingMode::for_answer(answer_thinking, &native_request);
        if turn_cancel.is_cancelled() {
            emit_cancelled_stream(sink, conv_id, request_id);
            return Err(cancellation_error());
        }
        info!(
            iteration = iteration,
            "Starting LLM generation (tool iteration {})", iteration
        );

        let llm_iteration_start = Instant::now();
        let mut round = generation::generate(
            llm.as_ref(),
            &mut native_request,
            &emitter,
            recorder,
            &turn_cancel,
            &clock,
            generation::thinking_label(iteration),
            answer_only,
        )
        .await?;
        if answer_only && final_round::wrote_only_calls(&round.response) {
            // Forbidding calls does not stop a model writing them, and with
            // thinking on Qwen 3.5 writes them into its text. Once, with no
            // tools in the prompt at all: this request re-reads the prompt,
            // but the turn ends with an answer rather than nothing.
            warn!(
                iteration,
                "Model answered the last round with tool calls; asking again without tools"
            );
            round
                .step
                .done(Some("tool calls instead of an answer".into()));
            recorder.note(
                TurnStepKind::Retry,
                "Asking again for the answer",
                None,
                Some("the model called tools in the last round".into()),
            );
            final_round::insist_on_answer(&mut native_request);
            round = generation::generate(
                llm.as_ref(),
                &mut native_request,
                &emitter,
                recorder,
                &turn_cancel,
                &clock,
                "Writing the answer",
                true,
            )
            .await?;
        }
        let generation::Generation {
            mut response,
            step: generate_step,
        } = round;
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
        let tool_calls = if last_round && !tool_calls.is_empty() {
            // Forbidden to call, a model can still call. It is not run: this
            // round's text is the answer.
            warn!(
                ignored_calls = tool_calls.len(),
                "Model called tools in the last round; keeping its text as the answer"
            );
            Vec::new()
        } else {
            tool_calls
        };
        // A call written as text is cut out of the answer, and the answer
        // says why it stops there rather than ending mid-thought for no
        // visible reason.
        let response_text = match answer_only
            .then(|| final_round::written_call_start(&response_text))
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
                condensed,
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

        calls
            .run_round(
                &tool_calls,
                &mut native_request.input,
                &mut timings,
                sources,
                retrieval_trace,
            )
            .await?;
        if let Some(journal) = journal {
            journal
                .round(&ResearchRound {
                    completed: iteration + 1,
                    transcript: native_request
                        .input
                        .get(planned_len..)
                        .map(<[CompletionInput]>::to_vec)
                        .unwrap_or_default(),
                    queries: calls.queries.clone(),
                    fetched: calls.fetched.clone(),
                    sources: sources.clone(),
                    retrieval_trace: retrieval_trace.clone(),
                    pages_read: calls.fetch_memory.clone(),
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
    lattice_core::application::ports::llm_port::chars_within_tokens(input_budget, chars_per_token)
}

/// The tool definitions as the provider receives them, for budgeting: the
/// JSON `parameters` of each tool are sent and read like any other prompt text.
pub(super) fn tool_schema_text(
    tools: &[lattice_core::application::ports::ToolDefinition],
) -> String {
    lattice_core::application::ports::llm_port::tool_specs(tools).to_string()
}

fn elapsed_ms(start: Instant) -> u64 {
    u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX)
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

/// A result cut shorter than this says too little to have been worth the round
/// that asked for it, so a nearly full window still gets this much.
const MIN_TOOL_RESULT_CHARS: usize = 1_500;

/// Roughly how many characters the next generation already has to read. A
/// picture counts as the characters its declared tokens stand for, never as
/// its bytes.
fn chars_in_flight(input: &[lattice_core::application::ports::llm_port::CompletionInput]) -> usize {
    use lattice_core::application::ports::llm_port::{CompletionInput, DEFAULT_CHARS_PER_TOKEN};
    input
        .iter()
        .map(|item| match item {
            CompletionInput::Message { content, .. } => content.chars().count(),
            CompletionInput::Multimodal { .. } => {
                item.text().chars().count()
                    + (item.image_tokens() as f64 * DEFAULT_CHARS_PER_TOKEN) as usize
            }
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
///
/// The window alone decides: there is no configured ceiling on top of it, so a
/// large window carries a long result whole. A result longer than its share is
/// shown in part and says how to read on (see `format_tool_result`).
///
/// One share of what is left is always kept back for the round after this
/// one. A lone page read that took the whole window left the next round
/// nothing to read with: it had to condense or stop, so one long page ended
/// the research. The rest of a long page is a further call by offset.
fn tool_result_allowance(window_chars: usize, chars_in_flight: usize, calls_left: usize) -> usize {
    let room = window_chars.saturating_sub(chars_in_flight);
    let share = room / calls_left.saturating_add(1).max(2);
    // The floor is only kept while the window can still take it. Past that, a
    // result is given its share of what is left and no more: overrunning the
    // window fails the next round outright, which is worse than a short result.
    if room < MIN_TOOL_RESULT_CHARS {
        return share;
    }
    share.max(MIN_TOOL_RESULT_CHARS)
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
mod tests;

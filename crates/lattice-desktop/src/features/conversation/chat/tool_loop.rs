mod call_labels;
mod calls;
mod clock;
mod document_evidence;
mod document_progress;
mod emitter;
mod final_round;
mod generation;
mod image_pick;
mod images;
mod limits;
#[cfg(test)]
mod loop_tests;
mod reading;
mod research_notes;
mod room;
mod scoped_document_tools;
mod tool_sources;
mod written_calls;

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
use emitter::StreamEmitter;
use final_round::LastRound;
pub(super) use reading::READING_INSTRUCTION;
use room::{chars_in_flight, tool_result_allowance, window_has_room_for_a_result};
use written_calls::WrittenCalls;

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
        message: question,
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
        // A turn whose reading came to nothing answered without the
        // instruction to read, and carries on without it.
        if !reading::noted_in(&start.transcript) {
            reading::withdraw(&mut input);
        }
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
    let mut window = room::Window::new(input_budget, llm.chars_per_token());
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
            window_chars: window.chars(),
            tool_schema_chars,
            llm,
            question,
        },
        // Pages are remembered for the whole turn — and the turn started
        // before this loop did: retrieval has usually opened the top results
        // already.
        pages_already_read,
        queries,
        fetched,
        sources,
    );
    // Why the next round has to answer, when a limit cut the round before it
    // short: the deadline passed, or the prompt no longer fit.
    let mut forced: Option<LastRound> = None;
    // A message that shows numbered passages asks for a reading of them
    // first; a resumed turn has its notes in the saved transcript already.
    let mut notes = None;
    if native_request.input.len() == planned_len {
        if let Some(passages) = reading::asked(&native_request.input, sources.as_slice()) {
            let reading_start = Instant::now();
            notes = match reading::read_passages(
                llm.as_ref(),
                &native_request,
                &passages,
                recorder,
                &turn_cancel,
                &clock,
            )
            .await
            {
                Ok(notes) => notes,
                Err(error) if limits::ran_out(&clock, &turn_cancel) => {
                    warn!(%error, "The turn ran out of time reading its passages; answering without notes");
                    forced = Some(LastRound::OutOfTime);
                    None
                }
                Err(error) => return Err(error),
            };
            let given = match notes.as_ref() {
                Some(read) => reading::append_notes(llm.as_ref(), &mut native_request, read).await,
                None => false,
            };
            notes = notes.filter(|_| given);
            timings.llm_stream_ms = timings
                .llm_stream_ms
                .saturating_add(elapsed_ms(reading_start));
        }
        // No notes, none that fit beside the answer, or nothing to read: the
        // answer is asked for exactly as before reading existed.
        if notes.is_none() {
            reading::withdraw(&mut native_request.input);
        }
    }
    let mut condensed = 0;
    let mut iteration = first_round;
    // A forced round runs even after the last: the round it stands in for was
    // cut short, not finished.
    while iteration < max_tool_rounds || forced.is_some() {
        timings.iterations = (iteration + 1) as u32;
        let out_of_rounds = iteration + 1 >= max_tool_rounds;
        // The prompt as the backend counts it, where it counts exactly: the
        // window follows what this turn has read, and a prompt already past
        // it sheds its newest results and answers.
        if iteration > 0 && forced.is_none() && llm.counts_tokens_exactly() {
            let tokens = reading::prompt_tokens(llm.as_ref(), &native_request).await;
            window.measured(
                chars_in_flight(&native_request.input) + tool_schema_chars,
                tokens,
            );
            calls.set_window_chars(window.chars());
            if tokens > window.tokens()
                && limits::shed(
                    &mut native_request.input,
                    planned_len,
                    window.chars_in(tokens - window.tokens()),
                    &mut calls,
                )
            {
                forced = Some(LastRound::WindowFull);
            }
        }
        // Before the window can be judged full: results that have outgrown
        // their share are condensed into notes and the research goes on. The
        // full-window ending below is the last resort.
        if iteration > 0
            && !out_of_rounds
            && forced.is_none()
            && research_notes::should_condense(
                window.chars(),
                &native_request.input,
                planned_len,
                condensed,
            )
        {
            condensed += 1;
            let condense_start = Instant::now();
            match research_notes::condense(
                llm.as_ref(),
                &mut native_request,
                planned_len,
                recorder,
                &turn_cancel,
                &clock,
            )
            .await
            {
                Ok(Some(notes)) => {
                    info!(
                        iteration,
                        chars_before = notes.chars_before,
                        chars_after = notes.chars_after,
                        "Condensed earlier tool results to keep researching"
                    );
                    calls.results_condensed(&notes.result_ids);
                }
                Ok(None) => {}
                Err(error) if limits::ran_out(&clock, &turn_cancel) => {
                    warn!(%error, "The turn ran out of time condensing its results; answering from them");
                    forced = Some(LastRound::OutOfTime);
                }
                Err(error) => return Err(error),
            }
            timings.llm_stream_ms = timings
                .llm_stream_ms
                .saturating_add(elapsed_ms(condense_start));
        }
        // A window with no room left for another result is the last round too:
        // calls made now would come back cut to nothing.
        let window_full = iteration > 0
            && !window_has_room_for_a_result(
                window.chars(),
                chars_in_flight(&native_request.input) + tool_schema_chars,
            );
        // A forced round says what forced it. Out of rounds is named before a
        // full window: the last round skips condensing, so its window can
        // look fuller than it would have been.
        let last_round = forced
            .or(out_of_rounds.then_some(LastRound::OutOfRounds))
            .or(window_full.then_some(LastRound::WindowFull));
        // The last round is for answering. A model still calling tools here
        // would otherwise end the turn with an error and lose every page and
        // passage it had gathered, so it may call none and is told to answer
        // from what it has.
        let answer_only = last_round.filter(|_| tools_ref.is_some_and(|tools| !tools.is_empty()));
        if let Some(last_round) = answer_only {
            info!(
                iteration,
                ?last_round,
                "Last tool round: asking for the answer with calls forbidden"
            );
            // A round asked again after the firmer retry has no tools left
            // to forbid, and a provider refuses a choice that names none.
            if answer_thinking || native_request.tools.is_empty() {
                // Thinking with the tools still in the prompt wrote the call
                // as text and answered nothing, so this answer has none.
                final_round::ask_for_answer_without_tools(&mut native_request, last_round);
            } else {
                final_round::ask_for_answer(&mut native_request, last_round);
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

        // A call written into the text instead of made is never shown: the
        // last round cuts it, and a round that may call runs it.
        let written_calls = match tools_ref.filter(|tools| !tools.is_empty()) {
            None => WrittenCalls::Shown,
            Some(_) if answer_only.is_some() => WrittenCalls::Cut,
            Some(tools) => WrittenCalls::Run(tools),
        };
        let llm_iteration_start = Instant::now();
        // Past the deadline the answering round alone runs on, bounded by
        // Stop and its output limit.
        let deadline = (last_round != Some(LastRound::OutOfTime)).then_some(&clock);
        let sent = chars_in_flight(&native_request.input)
            + if native_request.tools.is_empty() {
                0
            } else {
                tool_schema_chars
            };
        let mut round = match generation::generate(
            llm.as_ref(),
            &mut native_request,
            &emitter,
            recorder,
            &turn_cancel,
            deadline,
            match notes {
                Some(_) if iteration == 0 => "Writing the answer",
                _ => generation::thinking_label(iteration),
            },
            written_calls,
        )
        .await
        {
            Ok(round) => round,
            Err(error) => {
                forced = Some(limits::recover(
                    error,
                    last_round,
                    &mut native_request,
                    planned_len,
                    sent,
                    &mut window,
                    &mut calls,
                    &clock,
                    &turn_cancel,
                )?);
                continue;
            }
        };
        // The provider's count of what was sent: the next round's window is
        // judged at this turn's own rate.
        if let Ok(tokens @ 1..) = usize::try_from(round.response.input_tokens) {
            window.measured(sent, tokens);
            calls.set_window_chars(window.chars());
        }
        if let Some(last_round) =
            answer_only.filter(|_| final_round::wrote_only_calls(&round.response))
        {
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
            final_round::insist_on_answer(&mut native_request, last_round);
            let sent = chars_in_flight(&native_request.input);
            round = match generation::generate(
                llm.as_ref(),
                &mut native_request,
                &emitter,
                recorder,
                &turn_cancel,
                deadline,
                "Writing the answer",
                WrittenCalls::Cut,
            )
            .await
            {
                Ok(round) => round,
                Err(error) => {
                    forced = Some(limits::recover(
                        error,
                        Some(last_round),
                        &mut native_request,
                        planned_len,
                        sent,
                        &mut window,
                        &mut calls,
                        &clock,
                        &turn_cancel,
                    )?);
                    continue;
                }
            };
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
        let dropped_calls = last_round.is_some() && !tool_calls.is_empty();
        let tool_calls = if dropped_calls {
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
        let mut response_text = match answer_only.and_then(|last_round| {
            final_round::written_call_start(&response_text).map(|start| (start, last_round))
        }) {
            Some((start, last_round)) => {
                warn!(
                    conversation_id = conv_id,
                    "Model wrote tool calls as text in the last round; cutting them from the answer"
                );
                let kept = response_text[..start].trim_end();
                let note = last_round.note();
                emitter.content(note)?;
                if kept.is_empty() {
                    note.trim_start().to_string()
                } else {
                    format!("{kept}{note}")
                }
            }
            None => response_text,
        };
        // A last round that only called, even asked again, ends with the note
        // in place of an answer rather than an error: the turn keeps the
        // sources and results it gathered, and can be continued.
        if let Some(last_round) =
            last_round.filter(|_| dropped_calls && response_text.trim().is_empty())
        {
            let note = last_round.note().trim_start();
            emitter.content(note)?;
            response_text = note.to_string();
        }
        timings.llm_stream_ms = timings
            .llm_stream_ms
            .saturating_add(elapsed_ms(llm_iteration_start));
        generate_step.done(Some(round_result_line(&response_text, tool_calls.len())));
        if tool_calls.is_empty() {
            if response_text.trim().is_empty() {
                return Err(AppError::InvalidState("Model returned no answer".into()));
            }
            // A last round answers at a limit, and adds nothing after it.
            if let Some(notes) = notes.as_ref().filter(|_| last_round.is_none()) {
                let coverage_start = Instant::now();
                reading::add_what_was_left_out(
                    llm.as_ref(),
                    &mut native_request,
                    notes,
                    &mut response_text,
                    &emitter,
                    recorder,
                    &turn_cancel,
                    &clock,
                )
                .await?;
                timings.llm_stream_ms = timings
                    .llm_stream_ms
                    .saturating_add(elapsed_ms(coverage_start));
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

        if let Err(error) = calls
            .run_round(
                &tool_calls,
                &mut native_request.input,
                &mut timings,
                sources,
                retrieval_trace,
            )
            .await
        {
            if !limits::ran_out(&clock, &turn_cancel) {
                return Err(error);
            }
            warn!(%error, "The turn ran out of time running its tools; answering from what they found");
            final_round::answer_open_calls(
                &mut native_request.input,
                tool_calls.iter().map(|call| call.id.as_str()),
                final_round::OUT_OF_TIME_RESULT,
            );
            forced = Some(LastRound::OutOfTime);
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
        iteration += 1;
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
/// could not make, when the rounds ran out. One sentence, so the grounding
/// check can know it whole.
pub(in crate::features::conversation::chat) const OUT_OF_ROUNDS_NOTE: &str =
    "\n\n_(Stopped here: the model used every tool round this turn allows before it finished — say \"continue\" to let it keep going.)_";

/// The same, when the window had no room for another result. It points to a
/// larger window, not "continue": the next turn plans the same window, and
/// what filled this one may fill it again.
pub(in crate::features::conversation::chat) const WINDOW_FULL_NOTE: &str =
    "\n\n_(Stopped here: the context window filled before the model finished — a larger context window in Settings gives it room to read more.)_";

/// The same, when the turn's deadline passed. "Continue" starts a new turn
/// with a deadline of its own.
pub(in crate::features::conversation::chat) const OUT_OF_TIME_NOTE: &str =
    "\n\n_(Stopped here: the turn ran out of time before the model finished — say \"continue\" to let it keep going.)_";

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

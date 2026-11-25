//! Stage 6: the model writes the answer, calling tools as it goes.
use super::assemble::{PlannedRequest, RenderedPrompt};
use super::prepare::PreparedTurn;
use super::research::Rounds;
use super::*;
use crate::features::conversation::chat::tool_loop::{
    run_agentic_tool_loop_with_partial, LoopRequest,
};

/// The answer and what it cites, before it is saved.
pub(super) struct GeneratedAnswer {
    pub(super) response: String,
    /// The prompt's sources plus whatever the tool rounds added.
    pub(super) sources: Vec<SourceDto>,
    pub(super) retrieval_trace: Option<RetrievalTraceDto>,
    pub(super) tokens: TurnTokensDto,
    pub(super) memory_usage: Option<serde_json::Value>,
    /// Passed on to consolidation after the answer is saved.
    pub(super) summary_refold: Option<lattice_core::domain::conversation::memory::SummaryRefold>,
}

pub(super) enum GenerateOutcome {
    Completed(GeneratedAnswer),
    Cancelled {
        error: AppError,
        partial: Option<GeneratedAnswer>,
    },
}

/// `research` saves each round of a research turn, and resumes one after the
/// rounds it saved.
#[cfg(test)]
pub(super) async fn generate_answer(
    container: &dyn ChatRuntime,
    turn: &PreparedTurn,
    flags: SearchFlags,
    prompt: RenderedPrompt,
    request: PlannedRequest,
    research: Option<Rounds<'_>>,
    emit: &ChatEventSink,
    metrics: &mut ConversationFlowTimingMetrics,
) -> Result<GeneratedAnswer> {
    match generate_answer_outcome(
        container, turn, flags, prompt, request, research, emit, metrics,
    )
    .await?
    {
        GenerateOutcome::Completed(answer) => Ok(answer),
        GenerateOutcome::Cancelled { error, .. } => Err(error),
    }
}

/// Runs generation while capturing any partial streamed reply on user cancellation.
#[allow(clippy::too_many_arguments)]
pub(super) async fn generate_answer_outcome(
    container: &dyn ChatRuntime,
    turn: &PreparedTurn,
    flags: SearchFlags,
    prompt: RenderedPrompt,
    request: PlannedRequest,
    research: Option<Rounds<'_>>,
    emit: &ChatEventSink,
    metrics: &mut ConversationFlowTimingMetrics,
) -> Result<GenerateOutcome> {
    let RenderedPrompt {
        mut sources,
        mut retrieval_trace,
        short_circuit_response,
        pages_read,
        ..
    } = prompt;
    let PlannedRequest {
        input,
        tools,
        max_output_tokens,
        input_budget,
        memory_usage,
        summary_refold,
    } = request;
    turn.names.save().await?;
    let generation_start = Instant::now();
    let mut tokens = TurnTokensDto::default();
    let mut partial_text: Option<String> = None;
    let response = match short_circuit_response {
        Some(response) => {
            metrics.generation_subtimings = Some(ToolLoopTimingMetrics::default());
            Ok(response)
        }
        None => run_agentic_tool_loop_with_partial(
            container,
            turn.chat_turn(),
            emit,
            LoopRequest {
                input,
                tools: (!tools.is_empty()).then_some(tools.as_slice()),
                max_output_tokens,
                input_budget,
                time_budget: generation_time_budget(flags),
                max_tool_rounds: tool_loop::max_tool_rounds(
                    flags.deep_research_mode,
                    turn.source
                        .as_ref()
                        .map(|source| source.policy().tool_rounds),
                ),
                research,
            },
            &turn.settings.llm.tool_output,
            &mut sources,
            &mut retrieval_trace,
            pages_read,
            turn.source.as_ref(),
            &mut partial_text,
        )
        .await
        .map(|outcome| {
            metrics.generation_subtimings = Some(outcome.timings);
            tokens = TurnTokensDto {
                completion: outcome.output_tokens,
                context_used: outcome.input_tokens,
            };
            outcome.response
        }),
    };
    metrics.generation_ms = elapsed_ms(generation_start);
    match response {
        Ok(response) => Ok(GenerateOutcome::Completed(GeneratedAnswer {
            response,
            sources,
            retrieval_trace,
            tokens,
            memory_usage,
            summary_refold,
        })),
        Err(error)
            if crate::features::conversation::chat::cancellation::is_cancellation(&error) =>
        {
            let partial = partial_text.map(|text| GeneratedAnswer {
                response: text,
                sources,
                retrieval_trace,
                tokens,
                memory_usage,
                summary_refold,
            });
            Ok(GenerateOutcome::Cancelled { error, partial })
        }
        Err(error) => Err(error),
    }
}

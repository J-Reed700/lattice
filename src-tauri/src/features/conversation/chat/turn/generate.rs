//! Stage 6: the model writes the answer, calling tools as it goes.
use super::assemble::{PlannedRequest, RenderedPrompt};
use super::prepare::PreparedTurn;
use super::*;
use crate::features::conversation::chat::tool_loop::LoopRequest;

/// The answer and what it cites, before it is saved.
pub(super) struct GeneratedAnswer {
    pub(super) response: String,
    /// The prompt's sources plus whatever the tool rounds added.
    pub(super) sources: Vec<SourceDto>,
    pub(super) retrieval_trace: Option<RetrievalTraceDto>,
    pub(super) tokens: TurnTokensDto,
    pub(super) memory_usage: Option<serde_json::Value>,
}

pub(super) async fn generate_answer(
    container: &dyn ChatRuntime,
    turn: &PreparedTurn,
    flags: SearchFlags,
    prompt: RenderedPrompt,
    request: PlannedRequest,
    emit: &ChatEventSink,
    metrics: &mut ConversationFlowTimingMetrics,
) -> Result<GeneratedAnswer> {
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
    } = request;
    let generation_start = Instant::now();
    let mut tokens = TurnTokensDto::default();
    let response = match short_circuit_response {
        Some(response) => {
            metrics.generation_subtimings = Some(ToolLoopTimingMetrics::default());
            Ok(response)
        }
        None => run_agentic_tool_loop(
            container,
            &turn.conv_service,
            &turn.conv_id,
            &turn.turn_id,
            &turn.llm,
            emit,
            LoopRequest {
                input,
                tools: (!tools.is_empty()).then_some(tools.as_slice()),
                max_output_tokens,
                input_budget,
                time_budget: generation_time_budget(flags),
                max_tool_rounds: tool_loop::max_tool_rounds(
                    flags.deep_research_mode,
                    turn.explorer.is_some(),
                ),
            },
            &turn.highlight_terms,
            &turn.settings.llm.tool_output,
            &mut sources,
            &mut retrieval_trace,
            pages_read,
            &turn.focus,
            turn.explorer.as_ref(),
            &turn.recorder,
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
    Ok(GeneratedAnswer {
        response: response?,
        sources,
        retrieval_trace,
        tokens,
        memory_usage,
    })
}

//! Stage 2: what the turn needs, and where it looks for it.
use super::prepare::PreparedTurn;
use super::*;

/// Where the router sent the turn, and its record for the turn's trace.
pub(super) struct Route {
    pub(super) decision: RouterDecisionOutcome,
    pub(super) record: Option<TurnRouterDto>,
}

/// Classifies the turn's intent and routes against any recent document in a
/// single pass using the pre-resolved [`ScopeSnapshot`].
pub(super) async fn triage_turn(
    container: &dyn ChatRuntime,
    turn: &PreparedTurn,
    tool_preferences: Option<&ToolPreferences>,
    metrics: &mut ConversationFlowTimingMetrics,
) -> Result<(InferredIntent, Route)> {
    let outcome = triage_turn_with_scope(
        container,
        TurnTriageInput {
            chat_llm: &turn.llm,
            tool_preferences,
            requested_flags: turn.requested_flags,
            validated_message: &turn.message,
            context: &turn.context,
            document_context: &turn.document_context,
            scope: &turn.scope,
            source: turn.source.as_ref().map(|source| source.policy()),
            router_settings: &turn.settings.llm.router,
            recorder: &turn.recorder,
            cancel: turn.cancel_token(),
        },
        metrics,
    )
    .await?;
    Ok((
        outcome.inferred,
        Route {
            decision: outcome.decision,
            record: outcome.record,
        },
    ))
}

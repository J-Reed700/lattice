//! Stage 2: what the turn needs, and where it looks for it.
use super::prepare::PreparedTurn;
use super::*;

/// The flags the turn runs with. The utility model may add a vault search and
/// follow-up mode to an unflagged turn, and allow the web only as a fallback
/// after the vault. Not in an Explorer turn, which reads the library and the
/// web only when the user turns them on.
pub(super) async fn classify_turn(
    container: &dyn ChatRuntime,
    turn: &PreparedTurn,
    tool_preferences: Option<&ToolPreferences>,
) -> InferredIntent {
    if turn
        .source
        .as_ref()
        .is_some_and(|source| !source.policy().infers_intent)
    {
        return InferredIntent::as_requested(turn.requested_flags);
    }
    infer_turn_intent(
        container,
        &turn.llm,
        tool_preferences,
        turn.requested_flags,
        &turn.message,
        &turn.context,
        turn.cancel_token(),
    )
    .await
}

/// Where the router sent the turn, and its record for the turn's trace.
pub(super) struct Route {
    pub(super) decision: RouterDecisionOutcome,
    pub(super) record: Option<TurnRouterDto>,
}

pub(super) async fn route_turn(
    container: &dyn ChatRuntime,
    turn: &PreparedTurn,
    flags: SearchFlags,
    metrics: &mut ConversationFlowTimingMetrics,
) -> Result<Route> {
    let router_start = Instant::now();
    let (decision, record) = resolve_router_decision(
        container,
        &turn.document_context,
        &turn.message,
        flags,
        turn.source.as_ref().map(|source| source.policy()),
        &turn.settings.llm.router,
        &turn.recorder,
        turn.cancel_token(),
    )
    .await?;
    metrics.router_ms = elapsed_ms(router_start);
    Ok(Route { decision, record })
}

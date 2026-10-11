//! Stage 2: what the turn needs, and where it looks for it.
use super::prepare::PreparedTurn;
use super::*;

/// The flags the turn runs with.
///
/// The utility model infers what an unflagged turn needs (vault search, web,
/// follow-up) so an obvious case retrieves even with every toggle on its
/// default. Enable-only: it can add retrieval, never take any away.
///
/// Not in an Explorer turn: its subject is the folder on screen, and the
/// library and the web are read there only when the user turns them on.
pub(super) async fn classify_turn(
    container: &dyn ChatRuntime,
    turn: &PreparedTurn,
    tool_preferences: Option<&ToolPreferences>,
) -> SearchFlags {
    if turn.explorer.is_some() {
        return turn.requested_flags;
    }
    infer_turn_intent_flags(
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
        turn.explorer.is_some(),
        &turn.settings.llm.router,
        &turn.recorder,
        turn.cancel_token(),
    )
    .await?;
    metrics.router_ms = elapsed_ms(router_start);
    Ok(Route { decision, record })
}

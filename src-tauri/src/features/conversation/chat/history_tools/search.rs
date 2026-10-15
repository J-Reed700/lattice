//! The `search_conversation_history` tool.
use super::recall::exact_identifiers;
use super::{
    clip_to_bytes, FunctionResult, HistoryToolMemo, HistoryToolScope, SearchArgs, MAX_CANDIDATES,
    MIN_RESPONSE_BYTES, TOOL_READ_CONVERSATION_HISTORY,
};
use lattice_core::application::ports::conversation_memory::ConversationMemoryReadPort;
use lattice_core::shared::error::Result;

pub(super) async fn execute_search(
    port: &dyn ConversationMemoryReadPort,
    scope: &HistoryToolScope,
    memo: &mut HistoryToolMemo,
    arguments: &serde_json::Value,
) -> Result<FunctionResult> {
    let args: SearchArgs = serde_json::from_value(arguments.clone()).unwrap_or(SearchArgs {
        query: String::new(),
        limit: None,
    });
    let query = args.query.trim();
    if query.is_empty() {
        // Distinct from a miss: nothing was searched for, so nothing about the
        // transcript has been established either way.
        return Ok(FunctionResult::error(
            "EMPTY_QUERY",
            "Give a non-empty query. An empty search establishes nothing about what is in \
             this conversation.",
        ));
    }
    let limit = args
        .limit
        .map(|limit| (limit as usize).clamp(1, MAX_CANDIDATES))
        .unwrap_or(8);

    let key = format!("{}|{limit}", query.to_lowercase());
    if let Some(previous) = memo.searches.get(&key) {
        // The same search a second time cannot have a different answer, and the
        // excerpts are already in the transcript above.
        return Ok(FunctionResult::success(serde_json::json!({
            "conversation_scope": "current",
            "repeat_of_earlier_search": true,
            "message_ids": previous,
            "note": "You already ran this exact search this turn; its excerpts are above. \
                     Read one of these ids with read_conversation_history, or search for \
                     something different.",
        })));
    }

    let turn_remaining = memo.turn_remaining();
    if turn_remaining < MIN_RESPONSE_BYTES {
        // Checked before the search runs, so an empty result can never be
        // reported as "searched and found nothing" when the truth is that there
        // was no room left to send an answer.
        return Ok(FunctionResult::error(
            "TURN_BUDGET_EXHAUSTED",
            "This turn has spent its history-reading budget, so this search was not run. \
             Answer from what you have and name what you could not check.",
        ));
    }

    let exact_terms = exact_identifiers(query, &[]);
    let found = port
        .search_source_messages(scope.conversation_id(), query, &exact_terms, limit)
        .await?;

    let mut budget = scope.budget.resolve_bytes(None).min(turn_remaining);
    let mut matches = Vec::new();
    let mut ids = Vec::new();
    let mut truncated_by_budget = false;
    for candidate in found.candidates.iter().take(limit) {
        let (excerpt, clipped) = clip_to_bytes(&candidate.excerpt, budget);
        if excerpt.is_empty() {
            truncated_by_budget = true;
            break;
        }
        budget = budget.saturating_sub(excerpt.len());
        memo.bytes_delivered = memo.bytes_delivered.saturating_add(excerpt.len());
        truncated_by_budget |= clipped;
        ids.push(candidate.message_id.clone());
        matches.push(serde_json::json!({
            "message_id": candidate.message_id,
            "sequence": candidate.sequence,
            "role": candidate.role.as_str(),
            "excerpt": excerpt,
            "excerpt_is_partial": clipped,
            "matched_by": if candidate.exact_identifier { "exact_identifier" } else { "ranked" },
        }));
    }

    let returned = matches.len();
    if found.index_error.is_none() {
        // A search that could not run may succeed on a retry, so it is not
        // remembered as answered — otherwise a transient index failure would
        // turn into a permanent "you already asked that" for the rest of the turn.
        memo.searches.insert(key, ids.clone());
    }
    Ok(FunctionResult::success(serde_json::json!({
        "conversation_scope": "current",
        "matches": matches,
        "returned": returned,
        "more_may_exist": truncated_by_budget || found.candidates.len() > returned,
        "continuation": (returned > 0).then(|| serde_json::json!({
            "read_with": TOOL_READ_CONVERSATION_HISTORY,
            "message_ids": ids,
        })),
        "retrieval": retrieval_status(found.lexical_ran, found.index_error.as_deref(), returned),
    })))
}

/// The three outcomes §9.1 keeps apart, stated where the model will read them.
fn retrieval_status(
    lexical_ran: bool,
    index_error: Option<&str>,
    returned: usize,
) -> serde_json::Value {
    let (availability, note) = match (index_error, returned) {
        (Some(_), _) => (
            "unavailable",
            "The history index could not be searched, so this result says nothing about \
             whether the text exists. Ask the user rather than assuming.",
        ),
        (None, 0) => (
            "searched_no_match",
            "This search found no match. That is not evidence it was never said; try other \
             wording or exact identifiers.",
        ),
        (None, _) => ("searched_with_matches", "These are exact source excerpts."),
    };
    serde_json::json!({
        "availability": availability,
        "lexical_ran": lexical_ran,
        "index_error": index_error,
        "note": note,
    })
}

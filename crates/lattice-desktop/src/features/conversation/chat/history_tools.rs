//! Recovering what compaction left out, from this conversation's own
//! transcript (design: `docs/design/2026-09-19-conversation-memory.md` §9):
//! two read-only tools a model can call ([`history_tool_definitions`],
//! [`execute`]), and the same retrieval run before generation for paths
//! without tools ([`recall_for_turn`]).
//!
//! The conversation is never an argument: [`HistoryToolScope`] comes from the
//! turn, and a call naming a conversation is refused. Nothing is written
//! ([`ConversationMemoryReadPort`] has no mutating method), and an unavailable
//! index is reported apart from a miss. These tools stay out of the
//! process-wide function registry, which has no conversation in hand; the
//! turn's list comes from [`tools_for_turn`].

use std::collections::BTreeMap;
use std::time::Instant;

use serde::Deserialize;

use lattice_core::application::contracts::tools::{FunctionCall, FunctionResult};
use lattice_core::application::ports::conversation_memory::ConversationMemoryReadPort;
use lattice_core::application::ports::ToolDefinition;
use lattice_core::shared::error::Result;

mod read;
mod recall;
mod search;

use read::execute_read;
pub use recall::{
    recall_for_turn, PassageGroup, RecallAvailability, RecallRequest, RecalledContext,
};
use search::execute_search;

/// Tool the model calls to find where something was said.
pub const TOOL_SEARCH_CONVERSATION_HISTORY: &str = "search_conversation_history";
/// Tool the model calls to read the text back exactly.
pub const TOOL_READ_CONVERSATION_HISTORY: &str = "read_conversation_history";
pub const TOOL_SEARCH_SAVED_KNOWLEDGE: &str = "search_saved_knowledge";

/// §9.1 step 5: candidates one search returns. The history search tool's
/// largest page; a result names the offset its next page starts at, so every
/// match is reachable. Automatic recall searches again for more while its
/// budget has room.
const MAX_CANDIDATES: usize = 24;
/// §9.1 step 1: how much recent-turn context may be folded into the query, in
/// whitespace-separated tokens. Enough to resolve "that option"; not enough for
/// the whole tail of the conversation to become the query.
const MAX_CONTEXT_TOKENS: usize = 512;
/// Exact identifiers probed alongside the lexical query. The repository applies
/// the same ceiling; matching it here keeps the request honest about what will
/// actually be tried.
const MAX_EXACT_TERMS: usize = 8;
/// One neighbour each side is what explains "yes, option B": the question it
/// answered and the acknowledgement after it.
const ADJACENT_TURNS: usize = 1;
/// Default response ceiling for one read when the caller does not supply one.
const DEFAULT_RESPONSE_BYTES: usize = 8 * 1024;
/// Floor on a response ceiling. Below this a read returns a cursor and no text,
/// which costs a generation round to learn nothing.
const MIN_RESPONSE_BYTES: usize = 512;
/// What these tools may deliver across one whole turn when the caller names no
/// window. The tool loop names its model's, so separately affordable reads
/// cannot add up past it (§9.3).
const DEFAULT_TURN_RESPONSE_BYTES: usize = 16 * DEFAULT_RESPONSE_BYTES;
/// Message ceiling for one read. Paired with a byte ceiling because either one
/// alone is unbounded in the other dimension.
const MAX_READ_MESSAGES: usize = 64;
/// Arguments that would name a conversation. Present so a model that invents
/// one is told the tool is already scoped, instead of receiving this
/// conversation's text under the impression it read another.
const SCOPE_ARGUMENT_KEYS: [&str; 5] = [
    "conversation_id",
    "conversationId",
    "thread_id",
    "space_id",
    "spaceId",
];

// ---------------------------------------------------------------------------
// Tool definitions and per-turn availability
// ---------------------------------------------------------------------------

/// True for the two names this module answers. Everything else belongs to the
/// ordinary executor.
pub fn is_history_tool(name: &str) -> bool {
    name == TOOL_SEARCH_CONVERSATION_HISTORY
        || name == TOOL_READ_CONVERSATION_HISTORY
        || name == TOOL_SEARCH_SAVED_KNOWLEDGE
}

/// The two definitions, in the provider-facing shape.
///
/// The descriptions say what the results are and are not. A ranked excerpt list
/// is not an inventory of everything said, and neither tool can confirm that
/// something was never mentioned.
pub fn history_tool_definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: TOOL_SEARCH_SAVED_KNOWLEDGE.into(),
            description: "Search saved facts, decisions, preferences and open work in this conversation plus explicitly shared space and personal memory. Every result retains original evidence and source conversation. Use include_history for corrected, superseded or expired facts; never treat those as current. A miss is not proof a fact was never recorded. You cannot choose a foreign conversation or sharing scope.".into(),
            parameters: serde_json::json!({"type":"object","properties":{"query":{"type":"string"},"include_history":{"type":"boolean","default":false}},"required":["query"],"additionalProperties":false}),
        },
        ToolDefinition {
            name: TOOL_SEARCH_CONVERSATION_HISTORY.to_string(),
            description: "Search the original messages of THIS conversation, including older \
                 turns that were compacted out of your context. Returns ranked excerpts with \
                 message ids, sequence numbers and roles, so you can then read the full text \
                 with read_conversation_history. Results are a ranked subset, not a complete \
                 inventory: finding nothing means this search did not match it, not that it \
                 was never said. Always scoped to the current conversation."
                .to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "What to look for. Include exact identifiers, paths, \
                                        version numbers or quoted phrases when you have them."
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Excerpts in this page of results",
                        "default": 8,
                        "minimum": 1,
                        "maximum": MAX_CANDIDATES
                    },
                    "offset": {
                        "type": "integer",
                        "description": "Matches to skip: the next_offset a previous page named",
                        "default": 0,
                        "minimum": 0
                    }
                },
                "required": ["query"],
                "additionalProperties": false
            }),
        },
        ToolDefinition {
            name: TOOL_READ_CONVERSATION_HISTORY.to_string(),
            description: "Read the exact original text of messages in THIS conversation, by \
                 message id or by sequence interval. Give either message_ids or from_sequence \
                 (with an optional to_sequence), not both. Responses are cut at a byte budget \
                 and carry a continuation cursor; pass start_byte to resume. Always scoped to \
                 the current conversation."
                .to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "message_ids": {
                        "type": "array",
                        "items": {"type": "string"},
                        "description": "Message ids from search_conversation_history",
                        "maxItems": MAX_READ_MESSAGES
                    },
                    "from_sequence": {
                        "type": "integer",
                        "description": "First sequence number to read, inclusive",
                        "minimum": 0
                    },
                    "to_sequence": {
                        "type": "integer",
                        "description": "Last sequence number to read, inclusive. Omit to read \
                                        forward until the budget is spent.",
                        "minimum": 0
                    },
                    "start_byte": {
                        "type": "integer",
                        "description": "Resume offset from a previous continuation cursor",
                        "minimum": 0
                    },
                    "max_bytes": {
                        "type": "integer",
                        "description": "Requested response ceiling in bytes; the turn's own \
                                        budget may lower it",
                        "minimum": MIN_RESPONSE_BYTES
                    }
                },
                "additionalProperties": false
            }),
        },
    ]
}

/// The tool list for one turn: history access added on top of whatever
/// `external_tools` the turn's branch selected. A closed-book turn keeps it,
/// since the current conversation is task context, not an external source.
pub fn tools_for_turn(
    external_tools: &[ToolDefinition],
    closed_book: bool,
    history_available: bool,
) -> Vec<ToolDefinition> {
    let mut selected = if closed_book {
        Vec::new()
    } else {
        external_tools.to_vec()
    };
    if history_available {
        for definition in history_tool_definitions() {
            if !selected.iter().any(|tool| tool.name == definition.name) {
                selected.push(definition);
            }
        }
    }
    selected
}

// ---------------------------------------------------------------------------
// Trusted scope and shared budget
// ---------------------------------------------------------------------------

/// What one call may spend. The byte ceiling and `deadline` are the turn's, so
/// history reads draw from the same allowance as every other tool round (§9.3).
#[derive(Debug, Clone, Copy)]
pub struct HistoryToolBudget {
    pub max_response_bytes: usize,
    /// Everything these tools may deliver across the turn, from the model's
    /// window.
    pub turn_response_bytes: usize,
    pub deadline: Option<Instant>,
}

impl Default for HistoryToolBudget {
    fn default() -> Self {
        Self {
            max_response_bytes: DEFAULT_RESPONSE_BYTES,
            turn_response_bytes: DEFAULT_TURN_RESPONSE_BYTES,
            deadline: None,
        }
    }
}

impl HistoryToolBudget {
    /// A ceiling the model asked for, lowered to what the turn can afford.
    fn resolve_bytes(&self, requested: Option<u64>) -> usize {
        let ceiling = self.max_response_bytes.max(MIN_RESPONSE_BYTES);
        match requested {
            Some(requested) => (requested as usize).clamp(MIN_RESPONSE_BYTES, ceiling),
            None => ceiling,
        }
    }

    fn expired(&self) -> bool {
        self.deadline
            .is_some_and(|deadline| Instant::now() >= deadline)
    }
}

/// The conversation these tools may read, and nothing else. The id is private
/// and set only from the turn being executed, so no tool argument can change
/// which conversation is read.
pub struct HistoryToolScope {
    conversation_id: String,
    budget: HistoryToolBudget,
}

impl HistoryToolScope {
    /// Scope a turn's history tools to the conversation it is running in.
    pub fn new(conversation_id: impl Into<String>, budget: HistoryToolBudget) -> Self {
        Self {
            conversation_id: conversation_id.into(),
            budget,
        }
    }

    /// The trusted id, for logging and for the port call.
    pub fn conversation_id(&self) -> &str {
        &self.conversation_id
    }
}

// ---------------------------------------------------------------------------
// Per-turn memo
// ---------------------------------------------------------------------------

/// How much of one requested range the model already has.
#[derive(Debug, Default)]
struct ReadProgress {
    /// Where the next byte of unsent text begins, as a cursor into the range.
    next: Option<ReadCursor>,
    /// Where the first delivery started, so a repeat can be recognised as the
    /// same request rather than a deliberately narrower one.
    first_start: u64,
    /// True once the whole range has been handed over.
    exhausted: bool,
    message_ids: Vec<String>,
    sequences: Vec<i64>,
}

/// Where a truncated read stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ReadCursor {
    /// Sequence of the first message that is not fully delivered.
    from_sequence: i64,
    /// Byte offset within that message.
    start_byte: u64,
}

/// What this turn has already sent back from its own transcript, so a repeated
/// range gets a reference instead of the same text (as [`super::fetch_memory`]
/// does for URLs).
#[derive(Debug, Default)]
pub struct HistoryToolMemo {
    reads: BTreeMap<String, ReadProgress>,
    searches: BTreeMap<String, Vec<String>>,
    bytes_delivered: usize,
}

impl HistoryToolMemo {
    /// Bytes still available to these tools for the rest of the turn.
    fn turn_remaining(&self, budget: &HistoryToolBudget) -> usize {
        budget
            .turn_response_bytes
            .saturating_sub(self.bytes_delivered)
    }
}

// ---------------------------------------------------------------------------
// Arguments
// ---------------------------------------------------------------------------

#[derive(Debug, Default, Deserialize)]
struct SearchArgs {
    #[serde(default)]
    query: String,
    #[serde(default)]
    limit: Option<u64>,
    #[serde(default)]
    offset: Option<u64>,
}

#[derive(Debug, Default, Deserialize)]
struct ReadArgs {
    #[serde(default)]
    message_ids: Vec<String>,
    #[serde(default)]
    from_sequence: Option<i64>,
    #[serde(default)]
    to_sequence: Option<i64>,
    #[serde(default)]
    start_byte: Option<u64>,
    #[serde(default)]
    max_bytes: Option<u64>,
}

/// Refuses a call that names a conversation, space or thread: ignored, the
/// model would attribute this conversation's text to another. Other unknown
/// arguments are ignored so a stray hint field does not cost a round.
fn reject_scope_arguments(arguments: &serde_json::Value) -> Option<FunctionResult> {
    let object = arguments.as_object()?;
    let named = SCOPE_ARGUMENT_KEYS
        .iter()
        .find(|key| object.contains_key(**key))?;
    Some(FunctionResult::error(
        "SCOPE_IS_NOT_AN_ARGUMENT",
        format!(
            "'{named}' is not accepted: conversation history tools always read the \
             conversation you are in, and cannot reach another one. Remove it and retry."
        ),
    ))
}

// ---------------------------------------------------------------------------
// Execution
// ---------------------------------------------------------------------------

/// Runs one history tool call.
///
/// # Errors
///
/// Only when the port fails in a way the turn cannot absorb. Model mistakes
/// come back as an unsuccessful [`FunctionResult`] it can act on next round.
pub async fn execute(
    port: &dyn ConversationMemoryReadPort,
    scope: &HistoryToolScope,
    memo: &mut HistoryToolMemo,
    call: FunctionCall,
) -> Result<FunctionResult> {
    if let Some(refusal) = reject_scope_arguments(&call.arguments) {
        return Ok(refusal);
    }
    if scope.budget.expired() {
        return Ok(FunctionResult::error(
            "TURN_BUDGET_EXHAUSTED",
            "This turn is out of time. Answer with what you already have, and say which part \
             you could not check.",
        ));
    }
    match call.name.as_str() {
        TOOL_SEARCH_SAVED_KNOWLEDGE => {
            let query = call
                .arguments
                .get("query")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if query.trim().is_empty() {
                return Ok(FunctionResult::error(
                    "EMPTY_QUERY",
                    "Give a non-empty query.",
                ));
            }
            let room = scope
                .budget
                .max_response_bytes
                .min(memo.turn_remaining(&scope.budget));
            if room < MIN_RESPONSE_BYTES {
                return Ok(FunctionResult::error(
                    "TURN_BUDGET_EXHAUSTED",
                    "Answer using the evidence already available.",
                ));
            }
            let result = port
                .search_shared_knowledge(
                    scope.conversation_id(),
                    query,
                    call.arguments
                        .get("include_history")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false),
                    room,
                )
                .await?;
            memo.bytes_delivered = memo
                .bytes_delivered
                .saturating_add(result.to_string().len());
            Ok(FunctionResult::success(result))
        }
        TOOL_SEARCH_CONVERSATION_HISTORY => {
            execute_search(port, scope, memo, &call.arguments).await
        }
        TOOL_READ_CONVERSATION_HISTORY => execute_read(port, scope, memo, &call.arguments).await,
        other => Ok(FunctionResult::error(
            "NO_HANDLER",
            format!("'{other}' is not a conversation history tool"),
        )),
    }
}

// ---------------------------------------------------------------------------
// Byte-safe clipping
// ---------------------------------------------------------------------------

/// The longest prefix of `text` that fits in `max_bytes` without splitting a
/// character, and whether anything was left behind.
///
/// Byte-precise because evidence spans are byte offsets: rounding down to a
/// character boundary keeps `start_byte`/`end_byte` in the response usable as
/// offsets into the real message.
fn clip_to_bytes(text: &str, max_bytes: usize) -> (&str, bool) {
    if text.len() <= max_bytes {
        return (text, false);
    }
    let cut = floor_boundary(text, max_bytes);
    (&text[..cut], cut < text.len())
}

/// `index`, moved back to the nearest character boundary.
fn floor_boundary(text: &str, index: usize) -> usize {
    let mut index = index.min(text.len());
    while index > 0 && !text.is_char_boundary(index) {
        index -= 1;
    }
    index
}

#[cfg(test)]
mod tests;

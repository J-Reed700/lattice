//! Recovering what compaction left out, from this conversation's own transcript.
//!
//! Design: `docs/design/2026-09-19-conversation-memory.md` §9.
//!
//! Two shapes of the same job:
//!
//! * [`history_tool_definitions`] + [`execute`] — the two read-only tools a
//!   continuation model can call when the summary is not enough. These are the
//!   recovery path: the model noticed a gap and went looking.
//! * [`recall_for_turn`] — the same retrieval run automatically, before
//!   generation, for providers and QA paths that have no tools at all. Nothing
//!   here instructs a model to call a tool it was not given.
//!
//! Three properties hold for every entry point, and the tests are named after
//! them:
//!
//! * **The conversation is not an argument.** [`HistoryToolScope`] is built from
//!   trusted turn-execution context and its id is private. No tool schema here
//!   has a `conversation_id`, and a call that carries one is refused rather than
//!   quietly ignored, because a model that believes it read another thread will
//!   attribute the answer to the wrong person.
//! * **Nothing is written.** These paths reach the database only through
//!   [`ConversationMemoryReadPort`], which has no mutating method. Bookmarks,
//!   read-state, memory items and source text are untouched.
//! * **A miss is a miss, not an absence.** "This retrieval did not find it" and
//!   "the index was unavailable" are separate outcomes all the way out to the
//!   tool response, and neither one means the user never said the thing.
//!
//! These tools are deliberately *not* in the process-wide function registry.
//! That registry is shared with a Tauri command and with `FunctionExecutor`,
//! neither of which has a conversation in hand — a global registration would
//! advertise a tool that can only answer `NO_HANDLER` outside a chat turn, and
//! would slip past the vault-scope tripwire in
//! `tool_loop::scoped_document_tools` without being classified. The turn's tool
//! list is assembled instead by [`tools_for_turn`].

use std::collections::BTreeMap;
use std::time::Instant;

use serde::Deserialize;

use crate::features::function_calling::domain::{FunctionCall, FunctionResult};
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
/// page, which reports when more may exist; automatic recall searches again
/// for more while its budget has room.
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
/// Everything these tools may deliver across one whole turn. Per-call ceilings
/// come from the tool loop's remaining-context arithmetic; this is the other
/// half of §9.3's "share the turn's budget", and it is what stops twenty
/// separately-affordable reads from adding up to the window.
const MAX_TURN_RESPONSE_BYTES: usize = 128 * 1024;
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
                        "description": "Maximum excerpts to return",
                        "default": 8,
                        "minimum": 1,
                        "maximum": MAX_CANDIDATES
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

/// The tool list for one turn.
///
/// `external_tools` is whatever the turn's own availability branch selected —
/// the grounded subset, the full set, or nothing. History access is added on
/// top of every one of those branches, and a closed-book turn keeps it while
/// losing all of them: the current conversation is internal task context, not
/// an external source, and a turn that may not search the web still has to be
/// able to recover a requirement the user stated twenty turns ago.
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

/// What one call may spend.
///
/// `max_response_bytes` comes from the tool loop's remaining-context
/// arithmetic, and `deadline` is the turn's single wall-clock deadline. Both are
/// the turn's, not this tool's: §9.3 requires history reads to draw from the
/// same allowance as every other tool round.
#[derive(Debug, Clone, Copy)]
pub struct HistoryToolBudget {
    pub max_response_bytes: usize,
    pub deadline: Option<Instant>,
}

impl Default for HistoryToolBudget {
    fn default() -> Self {
        Self {
            max_response_bytes: DEFAULT_RESPONSE_BYTES,
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

/// The conversation these tools may read, and nothing else.
///
/// The id is private and has no setter. The only way to build one of these is
/// from the id the turn is already executing against, so there is no path by
/// which tool arguments — valid, malformed or adversarial — change which
/// conversation is read.
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

/// What this turn has already sent back from its own transcript.
///
/// A tool round costs a full model generation, so a round spent re-receiving
/// text that is already three messages up the transcript is the most expensive
/// kind of nothing — and with a long read it can also push the window over on
/// its own. Same reasoning as [`super::fetch_memory`], applied to ranges
/// instead of URLs.
#[derive(Debug, Default)]
pub struct HistoryToolMemo {
    reads: BTreeMap<String, ReadProgress>,
    searches: BTreeMap<String, Vec<String>>,
    bytes_delivered: usize,
}

impl HistoryToolMemo {
    /// Bytes still available to these tools for the rest of the turn.
    fn turn_remaining(&self) -> usize {
        MAX_TURN_RESPONSE_BYTES.saturating_sub(self.bytes_delivered)
    }
}

// ---------------------------------------------------------------------------
// Arguments
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct SearchArgs {
    #[serde(default)]
    query: String,
    #[serde(default)]
    limit: Option<u64>,
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

/// Refuse a call that names a conversation, space or thread.
///
/// Ignoring the argument would be quieter and worse: the model would read this
/// conversation's text believing it had reached another one, and would then
/// attribute what it found to the wrong conversation. Unknown arguments that
/// are *not* about identity are ignored, because a model adding a stray hint
/// field should not lose a whole round over it.
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

/// Run one history tool call.
///
/// # Errors
///
/// Returns `Err` only when the port itself fails in a way the turn cannot
/// absorb. Everything the model did wrong — a missing argument, an id from
/// another conversation, an exhausted budget — comes back as an unsuccessful
/// [`FunctionResult`], because those are things the model can act on in the
/// next round and a hard error would end the turn instead.
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
                .min(MAX_TURN_RESPONSE_BYTES.saturating_sub(memo.bytes_delivered));
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

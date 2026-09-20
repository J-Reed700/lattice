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

use crate::application::ports::conversation_memory::{
    ConversationMemoryReadPort, RecallCandidate, SourceReadLimits,
};
use crate::application::ports::ToolDefinition;
use crate::application::services::context_assembler::{RecallDiagnostics, SelectedPassage};
use crate::domain::conversation_memory::SourceMessage;
use crate::features::function_calling::domain::{FunctionCall, FunctionResult};
use crate::shared::error::Result;

/// Tool the model calls to find where something was said.
pub const TOOL_SEARCH_CONVERSATION_HISTORY: &str = "search_conversation_history";
/// Tool the model calls to read the text back exactly.
pub const TOOL_READ_CONVERSATION_HISTORY: &str = "read_conversation_history";

/// §9.1 step 5: the candidate ceiling for one retrieval.
const MAX_CANDIDATES: usize = 24;
/// §9.1 step 7: passage groups packed before the budget is consulted at all.
const MAX_PASSAGE_GROUPS: usize = 6;
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
    name == TOOL_SEARCH_CONVERSATION_HISTORY || name == TOOL_READ_CONVERSATION_HISTORY
}

/// The two definitions, in the provider-facing shape.
///
/// The descriptions say what the results are and are not. A ranked excerpt list
/// is not an inventory of everything said, and neither tool can confirm that
/// something was never mentioned.
pub fn history_tool_definitions() -> Vec<ToolDefinition> {
    vec![
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

async fn execute_search(
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

async fn execute_read(
    port: &dyn ConversationMemoryReadPort,
    scope: &HistoryToolScope,
    memo: &mut HistoryToolMemo,
    arguments: &serde_json::Value,
) -> Result<FunctionResult> {
    let args: ReadArgs = serde_json::from_value(arguments.clone()).unwrap_or_default();
    let wanted_ids: Vec<String> = args
        .message_ids
        .iter()
        .map(|id| id.trim().to_string())
        .filter(|id| !id.is_empty())
        .take(MAX_READ_MESSAGES)
        .collect();
    let by_id = !wanted_ids.is_empty();
    let by_range = args.from_sequence.is_some();
    if by_id && by_range {
        return Ok(FunctionResult::error(
            "AMBIGUOUS_RANGE",
            "Give either message_ids or from_sequence, not both: the two name different text \
             and the result could not be labelled honestly.",
        ));
    }
    if !by_id && !by_range {
        return Ok(FunctionResult::error(
            "NOTHING_REQUESTED",
            "Give message_ids from a search result, or from_sequence to read an interval.",
        ));
    }

    // What this turn already sent for this exact range. Read out before any
    // mutation so the lookup and the later update cannot disagree.
    let key = read_key(by_id, &wanted_ids, &args);
    let previous = memo.reads.get(&key);
    let exhausted = previous.is_some_and(|progress| progress.exhausted);
    let first_start = previous.map(|progress| progress.first_start).unwrap_or(0);
    let memo_cursor = previous.and_then(|progress| progress.next);
    let requested_start = args.start_byte.unwrap_or(0);
    if exhausted && (args.start_byte.is_none() || requested_start == first_start) {
        // §9.3: an identical, already-exhausted range gets a reference, not the
        // same text a second time.
        return Ok(FunctionResult::success(serde_json::json!({
            "conversation_scope": "current",
            "already_delivered": true,
            "message_ids": previous.map(|p| p.message_ids.clone()).unwrap_or_default(),
            "sequences": previous.map(|p| p.sequences.clone()).unwrap_or_default(),
            "note": "You already have the full text of this range earlier in this turn. \
                     Re-read it there, or request a different range.",
        })));
    }
    // An explicit offset is the model being deliberate and wins; otherwise a
    // repeat of a truncated range continues where it stopped rather than
    // re-sending what the model already has.
    let resume = if args.start_byte.is_some() {
        None
    } else {
        memo_cursor
    };
    let mut offset = args
        .start_byte
        .or(resume.map(|cursor| cursor.start_byte))
        .unwrap_or(0) as usize;

    let turn_remaining = memo.turn_remaining();
    if turn_remaining < MIN_RESPONSE_BYTES {
        return Ok(FunctionResult::error(
            "TURN_BUDGET_EXHAUSTED",
            "This turn has spent its history-reading budget. Answer from what you have and \
             name what you could not check.",
        ));
    }
    let budget = scope
        .budget
        .resolve_bytes(args.max_bytes)
        .min(turn_remaining);
    // The port's ceilings stay at the §14 defaults: the *response* budget is
    // applied below, so a single message larger than the budget still arrives
    // and can be clipped into a resumable cursor instead of vanishing.
    let limits = SourceReadLimits::new(MAX_READ_MESSAGES, SourceReadLimits::DEFAULT.max_bytes);

    let mut messages = if by_id {
        port.read_messages(scope.conversation_id(), &wanted_ids, limits)
            .await?
    } else {
        let from = resume
            .map(|cursor| cursor.from_sequence)
            .or(args.from_sequence)
            .unwrap_or_default();
        let to = args.to_sequence.unwrap_or(i64::MAX);
        port.read_sequence_range(scope.conversation_id(), from, to, limits)
            .await?
            .messages
    };
    // The port cap bound the fetch, so more of this range probably exists. An
    // observation, not a guess: the alternative is comparing against a
    // requested end that may be past the end of the conversation.
    let port_capped = messages.len() >= MAX_READ_MESSAGES;
    if let Some(cursor) = resume.filter(|_| by_id) {
        messages.retain(|message| message.sequence >= cursor.from_sequence);
    }

    // A failed assistant generation was never an answer this conversation gave.
    // Handing it back as history would attribute an abandoned draft to the
    // assistant, so it is dropped and counted rather than shown (§6.2).
    let before = messages.len();
    messages.retain(SourceMessage::is_eligible_source);
    let omitted_failed_outputs = before - messages.len();

    let delivered_ids: Vec<String> = messages.iter().map(|m| m.id.clone()).collect();
    let delivered_sequences: Vec<i64> = messages.iter().map(|m| m.sequence).collect();
    // An id from another conversation never comes back from the port, so its
    // absence here is the whole answer the model gets about it. A suffix the
    // budget could not carry is absent for a different reason, and the label
    // covers both rather than claiming to know which.
    let not_readable: Vec<String> = wanted_ids
        .iter()
        .filter(|id| !delivered_ids.iter().any(|found| found == *id))
        .cloned()
        .collect();

    let mut remaining = budget;
    let mut rendered = Vec::new();
    let mut cursor: Option<ReadCursor> = None;
    let mut rendered_all = true;
    for message in &messages {
        if remaining == 0 {
            cursor = Some(ReadCursor {
                from_sequence: message.sequence,
                start_byte: 0,
            });
            rendered_all = false;
            break;
        }
        let from = floor_boundary(&message.content, offset);
        offset = 0;
        let (text, clipped) = clip_to_bytes(&message.content[from..], remaining);
        let end = from + text.len();
        remaining = remaining.saturating_sub(text.len());
        memo.bytes_delivered = memo.bytes_delivered.saturating_add(text.len());
        rendered.push(serde_json::json!({
            "message_id": message.id,
            "sequence": message.sequence,
            "role": message.role.as_str(),
            "text": text,
            "start_byte": from,
            "end_byte": end,
            "is_whole_message": from == 0 && !clipped,
        }));
        if clipped {
            cursor = Some(ReadCursor {
                from_sequence: message.sequence,
                start_byte: end as u64,
            });
            rendered_all = false;
            break;
        }
    }
    if cursor.is_none() && port_capped && rendered_all {
        cursor = messages.last().map(|last| ReadCursor {
            from_sequence: last.sequence.saturating_add(1),
            start_byte: 0,
        });
    }

    let progress = memo.reads.entry(key).or_default();
    if progress.message_ids.is_empty() {
        progress.first_start = requested_start;
        progress.message_ids = delivered_ids;
        progress.sequences = delivered_sequences;
    }
    progress.next = cursor;
    progress.exhausted = cursor.is_none();

    Ok(FunctionResult::success(serde_json::json!({
        "conversation_scope": "current",
        "messages": rendered,
        "truncated": cursor.is_some(),
        "continuation": cursor.map(|cursor| serde_json::json!({
            "from_sequence": cursor.from_sequence,
            "start_byte": cursor.start_byte,
        })),
        "budget_bytes": budget,
        "not_readable_message_ids": not_readable,
        "omitted_failed_outputs": omitted_failed_outputs,
        "note": "Exact source text from this conversation. Ids listed as not readable are \
                 either outside this conversation or did not fit this response.",
    })))
}

/// The memo key for one read request. A function so the lookup before the read
/// and the update after it cannot drift apart.
fn read_key(by_id: bool, wanted_ids: &[String], args: &ReadArgs) -> String {
    if by_id {
        let mut sorted = wanted_ids.to_vec();
        sorted.sort();
        format!("ids:{}", sorted.join(","))
    } else {
        format!(
            "range:{}-{}",
            args.from_sequence.unwrap_or_default(),
            args.to_sequence
                .map(|to| to.to_string())
                .unwrap_or_else(|| "end".to_string())
        )
    }
}

// ---------------------------------------------------------------------------
// Automatic per-turn retrieval
// ---------------------------------------------------------------------------

/// What automatic retrieval is given about the turn.
#[derive(Debug, Default)]
pub struct RecallRequest<'a> {
    /// The user's message for this turn, exactly as submitted.
    pub user_input: &'a str,
    /// Recent turn text already in the prompt, oldest first. Used both to
    /// resolve pronouns in the query and to avoid re-sending what is already
    /// there.
    pub recent_turns: &'a [String],
    /// Active memory text already in the prompt, for the same exclusion.
    pub active_memory: &'a [String],
    /// Byte ceiling on the packed passages.
    pub max_bytes: usize,
}

/// One candidate and the turns around it, which is the unit that explains a
/// short reply: "yes, option B" is only an answer next to its question.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PassageGroup {
    /// Chronological within the group (§9.1 step 7). Always exact source text.
    pub passages: Vec<SelectedPassage>,
    /// Ids this group explains but does not repeat, because the prompt already
    /// carries them verbatim — from a recent turn, from active memory, or from
    /// an earlier group of this same recall (§9.1 step 8).
    pub already_in_context: Vec<String>,
    /// Sequence of the candidate this group was built around.
    pub anchor_sequence: i64,
    /// Order this group was selected in, best first.
    ///
    /// Kept because the two consumers want opposite orders: the model reads the
    /// thread in the order it happened, while `ContextAssembler` evicts recalled
    /// passages from the tail of the list it is handed. Rendering chronologically
    /// from a rank-ordered list would otherwise mean the newest passage is the
    /// one dropped when the recall pool is short, rather than the weakest.
    pub selection_rank: usize,
    /// True when the anchor came from exact-identifier matching rather than
    /// ranking, which is the case where lexical rank is least trustworthy.
    pub from_exact_identifier: bool,
}

/// Whether this retrieval ran, and what that means.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecallAvailability {
    /// Ran and selected passages.
    Selected,
    /// Ran and matched nothing. Not evidence that it was never said.
    NotFoundByRetrieval,
    /// Could not run. `diagnostics.index_error` carries the code.
    Unavailable,
}

impl RecallAvailability {
    /// The flag §9.3 requires beside automatically retrieved context, in the
    /// words the model sees. It never tells the model to call a tool.
    pub fn note(self) -> &'static str {
        match self {
            Self::Selected => {
                "Older passages below are exact text from this conversation, with ids and roles."
            }
            Self::NotFoundByRetrieval => {
                "A search of this conversation's older messages found nothing for this turn. \
                 That is not evidence the user never said it; ask rather than assert."
            }
            Self::Unavailable => {
                "This conversation's older messages could not be searched for this turn, so \
                 nothing here rules out something having been said earlier."
            }
        }
    }
}

/// Packed passage groups and what it took to get them.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RecalledContext {
    pub groups: Vec<PassageGroup>,
    pub diagnostics: RecallDiagnostics,
}

impl RecalledContext {
    /// Flattened for `ContextRequest::recalled`: groups in selection order,
    /// best first, and chronological inside each group. Worst-last is what makes
    /// the assembler's tail eviction drop the weakest group rather than the
    /// newest one.
    pub fn passages(&self) -> Vec<SelectedPassage> {
        let mut ordered: Vec<&PassageGroup> = self.groups.iter().collect();
        ordered.sort_by_key(|group| group.selection_rank);
        ordered
            .into_iter()
            .flat_map(|group| group.passages.iter().cloned())
            .collect()
    }

    pub fn availability(&self) -> RecallAvailability {
        if self.diagnostics.index_error.is_some() && self.groups.is_empty() {
            RecallAvailability::Unavailable
        } else if self.groups.is_empty() {
            RecallAvailability::NotFoundByRetrieval
        } else {
            RecallAvailability::Selected
        }
    }
}

/// Retrieval for one turn, for a path that has no tools to offer the model.
///
/// Implements §9.1 steps 1–8. The conversation comes from `scope`, never from
/// anything the model produced, so this shares the tools' scope guarantee.
///
/// # Errors
///
/// Returns `Err` only if the port fails outside its own degradation path. A
/// missing or broken index is reported through
/// [`RecallDiagnostics::index_error`] and leaves the turn otherwise intact.
pub async fn recall_for_turn(
    port: &dyn ConversationMemoryReadPort,
    scope: &HistoryToolScope,
    request: &RecallRequest<'_>,
) -> Result<RecalledContext> {
    let mut diagnostics = RecallDiagnostics::default();
    let query = build_recall_query(request.user_input, request.recent_turns);
    let exact_terms = exact_identifiers(request.user_input, request.recent_turns);
    if query.trim().is_empty() && exact_terms.is_empty() {
        return Ok(RecalledContext {
            groups: Vec::new(),
            diagnostics,
        });
    }
    if scope.budget.expired() {
        // Out of time is not the same as out of matches.
        diagnostics.index_error = Some("turn_budget_exhausted".to_string());
        return Ok(RecalledContext {
            groups: Vec::new(),
            diagnostics,
        });
    }

    let found = port
        .search_source_messages(
            scope.conversation_id(),
            &query,
            &exact_terms,
            MAX_CANDIDATES,
        )
        .await?;
    diagnostics.lexical_ran = found.lexical_ran;
    diagnostics.semantic_ran = found.semantic_ran;
    diagnostics.candidates_considered = found.candidates.len();
    diagnostics.index_error = found.index_error.clone();

    let mut already: Vec<&str> = Vec::new();
    already.extend(request.recent_turns.iter().map(String::as_str));
    already.extend(request.active_memory.iter().map(String::as_str));

    let max_bytes = if request.max_bytes == 0 {
        DEFAULT_RESPONSE_BYTES
    } else {
        request.max_bytes
    };
    let mut remaining = max_bytes;
    let mut groups: Vec<PassageGroup> = Vec::new();
    let mut claimed: Vec<String> = Vec::new();

    for candidate in ranked(&found.candidates) {
        if groups.len() >= MAX_PASSAGE_GROUPS || remaining == 0 {
            break;
        }
        if claimed.contains(&candidate.message_id) {
            // Already inside a neighbouring group; a second copy of it would
            // spend budget to say the same thing twice.
            continue;
        }
        let window = port
            .read_adjacent_turns(
                scope.conversation_id(),
                candidate.sequence,
                ADJACENT_TURNS,
                ADJACENT_TURNS,
                SourceReadLimits::new(1 + 2 * ADJACENT_TURNS, remaining.max(MIN_RESPONSE_BYTES)),
            )
            .await?;
        let window: Vec<&SourceMessage> = window
            .iter()
            .filter(|message| message.is_eligible_source())
            .collect();
        if window.is_empty() {
            continue;
        }

        let mut group = PassageGroup {
            passages: Vec::new(),
            already_in_context: Vec::new(),
            anchor_sequence: candidate.sequence,
            selection_rank: groups.len(),
            from_exact_identifier: candidate.exact_identifier,
        };
        for message in &window {
            let in_prompt = already
                .iter()
                .any(|present| present.contains(message.content.as_str()));
            // Adjacent windows overlap, so a neighbour can belong to a group
            // that was already packed. Paying for it twice buys nothing and
            // takes the budget from a group that has something new to say.
            if in_prompt || claimed.contains(&message.id) {
                // §9.1 step 8: named so the group still explains the link,
                // without paying for text the prompt already carries.
                group.already_in_context.push(message.id.clone());
                claimed.push(message.id.clone());
                continue;
            }
            let (text, clipped) = clip_to_bytes(&message.content, remaining);
            if text.is_empty() {
                break;
            }
            remaining = remaining.saturating_sub(text.len());
            claimed.push(message.id.clone());
            group.passages.push(SelectedPassage {
                message_id: message.id.clone(),
                sequence: message.sequence,
                role: message.role,
                text: text.to_string(),
            });
            if clipped {
                break;
            }
        }
        if group.passages.is_empty() {
            // Everything in the window is already in front of the model.
            continue;
        }
        groups.push(group);
    }

    // Selection is by rank so the budget drops the weakest group, but the model
    // reads the thread in the order it happened, so emission is chronological.
    groups.sort_by_key(|group| group.anchor_sequence);
    diagnostics.passages_selected = groups.iter().map(|group| group.passages.len()).sum();
    diagnostics.selected_message_ids = groups
        .iter()
        .flat_map(|group| group.passages.iter().map(|p| p.message_id.clone()))
        .collect();

    Ok(RecalledContext {
        groups,
        diagnostics,
    })
}

/// Candidates in selection order: exact-identifier hits first, then BM25 rank
/// (more negative is better), with sequence breaking ties so the order is
/// stable across runs.
fn ranked(candidates: &[RecallCandidate]) -> Vec<&RecallCandidate> {
    let mut ordered: Vec<&RecallCandidate> = candidates.iter().collect();
    ordered.sort_by(|a, b| {
        b.exact_identifier
            .cmp(&a.exact_identifier)
            .then(
                a.score
                    .partial_cmp(&b.score)
                    .unwrap_or(std::cmp::Ordering::Equal),
            )
            .then(a.sequence.cmp(&b.sequence))
    });
    ordered
}

// ---------------------------------------------------------------------------
// Query formation
// ---------------------------------------------------------------------------

/// The user's input plus a bounded tail of recent-turn context (§9.1 step 1).
///
/// The tail exists for pronouns: "does that still hold?" has nothing in it to
/// match on. It is capped in tokens rather than messages because one pasted
/// message would otherwise become the entire query.
fn build_recall_query(user_input: &str, recent_turns: &[String]) -> String {
    let mut query = String::from(user_input.trim());
    let mut budget = MAX_CONTEXT_TOKENS;
    // Newest first: the nearest turn is the one a pronoun refers to.
    for turn in recent_turns.iter().rev() {
        if budget == 0 {
            break;
        }
        let taken: Vec<&str> = turn.split_whitespace().take(budget).collect();
        if taken.is_empty() {
            continue;
        }
        budget -= taken.len();
        query.push(' ');
        query.push_str(&taken.join(" "));
    }
    query
}

/// Exact identifiers worth probing as substrings rather than trusting to rank:
/// paths, version numbers, quoted phrases, numbers, and tokens carrying enough
/// punctuation to be a name rather than a word.
///
/// These are held separately from the lexical query because truncating that
/// query must never be able to discard one (§9.1 step 1), and because
/// `instr` is case-sensitive: `AuthToken` and `authtoken` are different names.
fn exact_identifiers(user_input: &str, recent_turns: &[String]) -> Vec<String> {
    fn push(candidate: &str, found: &mut Vec<String>) {
        let candidate = candidate.trim();
        if candidate.is_empty()
            || candidate.chars().count() < 2
            || found.len() >= MAX_EXACT_TERMS
            || found.iter().any(|existing| existing == candidate)
        {
            return;
        }
        found.push(candidate.to_string());
    }

    let mut found: Vec<String> = Vec::new();

    for source in std::iter::once(user_input).chain(recent_turns.iter().rev().map(String::as_str)) {
        for quoted in quoted_spans(source) {
            push(quoted, &mut found);
        }
        for token in source.split_whitespace() {
            // Sentence punctuation is not part of a name, but a trailing `/` or
            // `)` inside one is, so only the outermost separators come off.
            let token =
                token.trim_matches(|c: char| matches!(c, ',' | ';' | '"' | '\'' | '(' | ')'));
            let token = token.trim_end_matches(['.', '?', '!', ':']);
            if is_exact_identifier(token) {
                push(token, &mut found);
            }
        }
        if found.len() >= MAX_EXACT_TERMS {
            break;
        }
    }
    found
}

/// Text inside matching double or single quotes. A quoted phrase is the
/// strongest possible signal that the user means these exact words.
fn quoted_spans(text: &str) -> Vec<&str> {
    let mut spans = Vec::new();
    for quote in ['"', '\''] {
        let mut rest = text;
        while let Some(open) = rest.find(quote) {
            let after = &rest[open + quote.len_utf8()..];
            let Some(close) = after.find(quote) else {
                break;
            };
            spans.push(&after[..close]);
            rest = &after[close + quote.len_utf8()..];
        }
    }
    spans
}

/// Whether a token is a name rather than a word.
fn is_exact_identifier(token: &str) -> bool {
    if token.is_empty() || token.chars().count() > 128 {
        return false;
    }
    let alphanumeric = token.chars().filter(|c| c.is_alphanumeric()).count();
    if alphanumeric == 0 {
        return false;
    }
    let punctuation = token
        .chars()
        .filter(|c| !c.is_alphanumeric() && !c.is_whitespace())
        .count();
    // Paths and namespaced names.
    if token.contains('/') || token.contains('\\') || token.contains("::") {
        return true;
    }
    // A digit next to a separator: versions, error codes, ids, prices.
    if token.chars().any(|c| c.is_ascii_digit()) && (punctuation > 0 || alphanumeric >= 2) {
        return true;
    }
    // Scripts the FTS index cannot segment. `conversation_search_fts` is
    // tokenized `unicode61`, which splits on separators; Japanese, Chinese,
    // Korean and Thai are written without them, so a whole clause arrives as one
    // token and a lexical query matches only when it is byte-identical. Probing
    // these as substrings is the only way a query in them reaches its passage.
    if token.chars().filter(|c| is_unsegmented_script(*c)).count() >= 2 {
        return true;
    }
    // Two or more separators inside one token is not prose.
    punctuation >= 2
}

/// Whether `c` belongs to a script written without word separators.
fn is_unsegmented_script(c: char) -> bool {
    matches!(c,
        '\u{3040}'..='\u{30FF}'      // Hiragana and Katakana
        | '\u{3400}'..='\u{4DBF}'    // CJK Extension A
        | '\u{4E00}'..='\u{9FFF}'    // CJK Unified Ideographs
        | '\u{AC00}'..='\u{D7AF}'    // Hangul syllables
        | '\u{0E00}'..='\u{0E7F}'    // Thai
        | '\u{F900}'..='\u{FAFF}'    // CJK compatibility ideographs
    )
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

//! What the assembler returns, and what it reports about how it got there.
//!
//! Design: `docs/design/2026-09-19-conversation-memory.md` §10.4, §13.

use super::budget::{BudgetAllocation, TokenAccounting};
use crate::application::ports::llm_port::CompletionInput;

/// One passage selected from the original transcript.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectedPassage {
    pub message_id: String,
    pub sequence: i64,
    pub role: crate::domain::conversation::memory::SourceRole,
    /// Exact source text. Never a paraphrase, and never shortened with an
    /// ellipsis while still being labelled verbatim.
    pub text: String,
}

/// Which retrieval modes ran, and what they found.
///
/// "Nothing matched" and "the index was unavailable" are separate facts. So is
/// "retrieval was not attempted". Collapsing them would let an absent result be
/// read as proof the user never said the thing.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RecallDiagnostics {
    pub lexical_ran: bool,
    pub semantic_ran: bool,
    pub candidates_considered: usize,
    pub passages_selected: usize,
    pub selected_message_ids: Vec<String>,
    /// Stable code, never raw query or transcript text.
    pub index_error: Option<String>,
}

impl RecallDiagnostics {
    /// True when retrieval ran and simply found nothing — as distinct from not
    /// having run, or having failed.
    pub fn is_clean_miss(&self) -> bool {
        (self.lexical_ran || self.semantic_ran)
            && self.index_error.is_none()
            && self.passages_selected == 0
    }
}

/// Token cost of every part of an assembled request (§13).
///
/// Each field is measured, not budgeted: this is what was actually spent, so a
/// diagnostic can be compared against the allocation that authorized it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ContextAccounting {
    pub model_identity: String,
    pub capacity: usize,
    pub input_budget: usize,
    pub output_reserved: usize,
    pub safety_margin: usize,
    pub fixed_policy: usize,
    pub tool_schemas: usize,
    pub current_input: usize,
    pub active_memory: usize,
    pub summary: usize,
    pub recent_history: usize,
    pub recall: usize,
    pub document_evidence: usize,
    /// Sum of everything above that is part of the request.
    pub total_input: usize,
    pub accounting_method: Option<TokenAccounting>,
    pub active_mandatory_count: usize,
    pub active_conflict_count: usize,
    pub optional_selected_count: usize,
    /// True when mandatory memory had to borrow from optional allocations.
    pub mandatory_borrowed: bool,
    /// Optional material dropped to make the request fit, by pool name.
    pub evicted: Vec<String>,
}

impl ContextAccounting {
    pub fn from_allocation(allocation: &BudgetAllocation, model_identity: &str) -> Self {
        Self {
            model_identity: model_identity.to_string(),
            capacity: allocation.capacity,
            input_budget: allocation.input_budget,
            output_reserved: allocation.output_reserved,
            safety_margin: allocation.safety_margin,
            accounting_method: Some(allocation.accounting),
            ..Default::default()
        }
    }

    /// Whether the measured request fits what the allocation permitted.
    pub fn fits(&self) -> bool {
        self.total_input <= self.input_budget
    }
}

/// A complete, bounded model input plus everything needed to audit it.
///
/// Not `PartialEq`: `CompletionInput` carries opaque provider JSON, and
/// comparing two plans structurally would compare that too. Tests assert on the
/// rendered roles and text instead.
#[derive(Debug, Clone)]
pub struct ContextPlan {
    /// Typed messages, in the §10.4 rendering order. Built as typed entries end
    /// to end: memory is never promoted into system instructions by string
    /// parsing, and raw user bytes are never trimmed.
    pub messages: Vec<CompletionInput>,
    /// Memory revision this plan was built from. Carried through every tool
    /// round so a round cannot silently mix two revisions.
    pub memory_revision: i64,
    /// Transcript revision at selection time. Checked again before sending.
    pub transcript_revision: i64,
    pub accounting: ContextAccounting,
    pub retrieval: RecallDiagnostics,
    /// True when unprocessed original messages could not be carried raw, so
    /// compaction must run before this conversation continues.
    ///
    /// The assembler reports it rather than triggering it: a compaction is a
    /// model call, and the assembler does not own one.
    pub compaction_required: bool,
    /// Output cap to apply to the request, so reserving output room is actually
    /// enforced rather than merely accounted for.
    pub max_output_tokens: usize,
    /// Every memory item whose text is in `messages`: mandatory items, and the
    /// optional ones that survived selection and overflow eviction. Recorded on
    /// the answer as the memory it was given, so it is taken from the plan
    /// itself rather than read back out of rendered prose.
    pub used_memory_ids: Vec<String>,
}

impl ContextPlan {
    /// Number of rendered messages, for diagnostics.
    pub fn message_count(&self) -> usize {
        self.messages.len()
    }

    /// The request's text as a tokenizer reads it, framing aside: every
    /// role and message body, call and result. What an exact count measures.
    pub fn tokenizable_text(&self) -> String {
        let mut text = String::new();
        for message in &self.messages {
            if !text.is_empty() {
                text.push('\n');
            }
            match message {
                CompletionInput::Message { role, content } => {
                    text.push_str(role);
                    text.push('\n');
                    text.push_str(content);
                }
                CompletionInput::ToolCall {
                    name, arguments, ..
                } => {
                    text.push_str(name);
                    text.push('\n');
                    text.push_str(&arguments.to_string());
                }
                CompletionInput::ToolResult { output, .. } => text.push_str(output),
                CompletionInput::Native { value } => text.push_str(&value.to_string()),
            }
        }
        text
    }

    /// Replace the estimated total with one counted by the provider's own
    /// tokenizer: `text_tokens` for [`Self::tokenizable_text`], plus the same
    /// per-message framing and tool schemas the estimate charged. The pool
    /// figures stay the estimates selection ran on; the total, which decides
    /// whether the request fits, is now exact.
    pub fn settle_exact_count(&mut self, text_tokens: usize, tool_schema_tokens: usize) {
        self.accounting.total_input = text_tokens
            .saturating_add(
                self.messages
                    .len()
                    .saturating_mul(super::MESSAGE_FRAMING_TOKENS),
            )
            .saturating_add(tool_schema_tokens);
        self.accounting.accounting_method = Some(TokenAccounting::Exact);
    }
}

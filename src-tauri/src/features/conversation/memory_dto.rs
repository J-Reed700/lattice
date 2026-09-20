//! Wire types for the conversation-memory details view.
//!
//! Design: `docs/design/2026-09-19-conversation-memory.md` §13.
//!
//! Read-only on purpose. There is no "edit memory" shape here and there is not
//! going to be one in v1: a free-form editor is a way to create a requirement
//! with no source behind it, and the entire value of this ledger is that every
//! authoritative item can be traced to something the user actually wrote. A
//! correction is an ordinary message and goes through the same validation as any
//! other.

use serde::{Deserialize, Serialize};

/// Request the memory details for one conversation.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct GetConversationMemoryRequestDto {
    pub conversation_id: String,
    /// Include superseded and resolved items, for "what was my original
    /// budget?". Defaults to false.
    #[serde(default)]
    pub include_history: Option<bool>,
}

/// One quoted passage behind a memory item.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct MemoryEvidenceDto {
    pub message_id: String,
    pub sequence: i64,
    /// `user`, `assistant` or `system`.
    pub role: String,
    /// `assertion`, `antecedent` or `transition`.
    pub purpose: String,
    pub start_byte: u32,
    pub end_byte: u32,
    /// The exact source text, resolved from the original message.
    ///
    /// `null` when the message was edited or deleted. The UI must render that
    /// absence rather than a cached quotation — a deleted passage does not come
    /// back through a details view.
    pub text: Option<String>,
}

/// One memory record.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ConversationMemoryItemDto {
    pub id: String,
    /// `constraint`, `goal`, `decision`, `user_fact`, `preference`,
    /// `open_question` or `unresolved_change`.
    pub kind: String,
    /// `active`, `superseded` or `resolved`.
    pub state: String,
    /// `supported` or `ambiguous`.
    pub review: String,
    /// Generated text for scanning the list. **Not** evidence: the UI must make
    /// it visually distinct from the quotations, or a generated sentence reads
    /// as something the user wrote.
    pub label: String,
    /// True when this item is loaded into every prompt while active.
    pub is_mandatory: bool,
    pub created_at_sequence: i64,
    pub changed_at_sequence: i64,
    pub superseded_by: Option<String>,
    pub related_item_ids: Vec<String>,
    pub evidence: Vec<MemoryEvidenceDto>,
}

/// What the memory layer can currently promise for a conversation.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ConversationMemoryDetailsDto {
    pub conversation_id: String,
    /// `ready`, `rebuild_required`, or `unsupported_schema`.
    ///
    /// Three states rather than a boolean because they call for different UI:
    /// rebuilding is temporary and self-healing, an unsupported layout needs the
    /// app updated, and neither should be described as working memory.
    pub mode: String,
    pub schema_version: i64,
    pub memory_revision: i64,
    pub transcript_revision: i64,
    /// Everything at or below this sequence has been submitted for extraction.
    /// Coverage of *processing*, not a claim that every fact was noticed.
    pub processed_through_sequence: i64,
    pub active_mandatory_count: i64,
    pub active_optional_count: i64,
    /// Active items whose interpretation is unsettled, plus any whose evidence
    /// no longer resolves. These are the ones worth a user's attention.
    pub conflict_count: i64,
    /// The working summary. Generated and fallible; the UI labels it so.
    pub summary: Option<String>,
    pub items: Vec<ConversationMemoryItemDto>,
    /// Superseded and resolved items, when `includeHistory` asked for them.
    pub history: Vec<ConversationMemoryItemDto>,
    pub last_error_code: Option<String>,
    pub extractor_model_identity: Option<String>,
    /// False while the staged-rollout switch is off. The UI must not advertise
    /// reliable memory when this is false (§18).
    pub feature_enabled: bool,
}

/// Memory accounting attached to a compaction response (§13).
///
/// `summaryTokens` on the compaction record keeps meaning the summary alone.
/// These are reported beside it rather than folded into it, because presenting
/// a summary-only compression ratio as total prompt savings overstates what
/// compaction achieved.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CompactionMemoryDto {
    pub memory_revision: i64,
    pub active_mandatory_count: i64,
    pub active_optional_count: i64,
    /// `ready`, `rebuild_required`, `unsupported_schema` or `degraded`.
    pub mode: String,
    /// Tokens the active memory block costs, separate from the summary.
    pub memory_tokens: i64,
    /// Source messages this pass actually submitted for extraction.
    pub processed_message_count: i64,
    /// True when a work cap stopped the pass short and more source remains.
    pub more_source_remains: bool,
}

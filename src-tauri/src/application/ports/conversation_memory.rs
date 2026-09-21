//! Ports for bounded conversation memory.
//!
//! Design: `docs/design/2026-09-19-conversation-memory.md` §14.
//!
//! Three capabilities, deliberately separate because they have different
//! failure modes and different consequences when unavailable:
//!
//! * [`ConversationMemoryPort`] — the authoritative ledger: a consistent
//!   snapshot read, bounded paging over original messages, and one atomic
//!   commit. If this is unavailable, memory cannot be updated at all.
//! * [`ConversationMemoryReadPort`] — recall over the original transcript.
//!   Optional evidence. If this is unavailable the turn still runs with its
//!   constraints intact; it simply reports that recall did not.
//!
//! Invalidation on a transcript rewrite has no port method on purpose. It is
//! enforced by SQL triggers in the schema, so it participates in whatever
//! transaction performed the rewrite and cannot be forgotten by a new call
//! site. See `trg_conversation_memory_invalidate_*` in the init migration.

use crate::domain::conversation_memory::{
    ConversationMemoryState, MemoryCommit, MemoryId, MemorySnapshot, SourceMessage, SourceRole,
};
use crate::shared::error::Result;
use async_trait::async_trait;

/// Ceilings on one source read. Both matter: a message limit alone does not
/// bound a conversation containing one enormous paste.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceReadLimits {
    pub max_messages: usize,
    /// Decoded UTF-8 bytes across the whole page.
    pub max_bytes: usize,
}

impl SourceReadLimits {
    /// The §14 default: 256 messages or 1 MiB, whichever comes first.
    pub const DEFAULT: Self = Self {
        max_messages: 256,
        max_bytes: 1024 * 1024,
    };

    pub fn new(max_messages: usize, max_bytes: usize) -> Self {
        Self {
            max_messages,
            max_bytes,
        }
    }
}

impl Default for SourceReadLimits {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// One page of original messages.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SourcePage {
    pub messages: Vec<SourceMessage>,
    /// True when a limit stopped the page short of `through_sequence`.
    /// Resumable: the caller continues from `next_after_sequence`.
    pub has_more: bool,
    /// Sequence to pass as `after_sequence` on the next call.
    pub next_after_sequence: i64,
}

/// A candidate passage found by recall.
#[derive(Debug, Clone, PartialEq)]
pub struct RecallCandidate {
    pub message_id: String,
    pub sequence: i64,
    pub role: SourceRole,
    /// Exact source substring. Never a denormalized copy from an index row —
    /// candidates are always resolved back to the original message.
    pub excerpt: String,
    /// Rank score, comparable only within one retrieval mode.
    pub score: f32,
    /// True when this came from exact identifier matching rather than ranking.
    /// These survive lexical-query truncation.
    pub exact_identifier: bool,
}

/// What recall returned, and which modes actually ran.
///
/// The distinction matters: "no hits" and "the index was unavailable" lead to
/// different answers, and neither means the user never said the thing.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RecallCandidates {
    pub candidates: Vec<RecallCandidate>,
    pub lexical_ran: bool,
    pub semantic_ran: bool,
    /// Stable code, never raw query or transcript text.
    pub index_error: Option<String>,
}

impl RecallCandidates {
    /// Recall that could not run at all, with the reason.
    pub fn unavailable(code: impl Into<String>) -> Self {
        Self {
            index_error: Some(code.into()),
            ..Default::default()
        }
    }
}

/// A span to read back, identified the way memory stores it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceSpanRef {
    pub message_id: String,
    pub start_byte: u32,
    pub end_byte: u32,
}

/// Resolved source text for one span.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedSpan {
    pub message_id: String,
    pub sequence: i64,
    pub role: SourceRole,
    /// `None` when the source changed or vanished. The caller must render the
    /// absence, never a cached quotation.
    pub text: Option<String>,
}

/// Summary fields written alongside a memory commit.
///
/// Kept as one value so the summary and the ledger cannot be written by two
/// separate calls and end up describing different revisions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SummaryUpdate {
    pub summary_text: String,
    pub up_to_message_id: String,
    pub original_message_count: i64,
    pub original_tokens: i64,
    pub summary_tokens: i64,
}

/// Everything one compaction wants to make durable, in one transaction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryCommitCandidate {
    pub commit: MemoryCommit,
    pub summary: Option<SummaryUpdate>,
    /// Source messages this pass read, recorded on the event row.
    pub source_message_ids: Vec<String>,
    pub transcript_revision: i64,
    pub extractor_model_identity: Option<String>,
    pub extractor_prompt_version: Option<String>,
    pub validator_version: Option<String>,
    /// Short operation label for the event log, e.g. `compact` or `rebuild`.
    pub operation: String,
}

/// Preconditions a commit is checked against.
///
/// An in-process lock stops duplicate work; these stop an *incorrect* publish,
/// which is a different problem and needs the database to arbitrate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryCommitPreconditions {
    pub conversation_id: String,
    pub expected_transcript_revision: i64,
    pub expected_memory_revision: i64,
    /// Idempotency key. Retrying a committed operation returns its result
    /// rather than applying it twice.
    pub operation_id: String,
}

/// What a successful commit produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommittedMemorySnapshot {
    pub memory_revision: i64,
    pub transcript_revision: i64,
    pub processed_through_sequence: i64,
    pub active_mandatory_count: usize,
    pub active_optional_count: usize,
    /// True when this call found an existing event for `operation_id` and
    /// returned it instead of writing again.
    pub was_already_committed: bool,
}

/// Why a commit did not happen.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum MemoryCommitError {
    #[error("conversation {0} not found")]
    NotFound(String),
    #[error(
        "transcript moved during extraction (expected revision {expected}, found {actual}); \
         retry from a fresh snapshot"
    )]
    TranscriptConflict { expected: i64, actual: i64 },
    #[error(
        "memory changed during extraction (expected revision {expected}, found {actual}); \
         retry from a fresh snapshot"
    )]
    MemoryConflict { expected: i64, actual: i64 },
    #[error("stored memory schema {0} is not supported by this build; a rebuild is required")]
    UnsupportedSchema(i64),
    #[error("evidence for item {0} no longer resolves against its source")]
    UnresolvableEvidence(String),
    #[error("{0} active items exceeds the host work limit")]
    TooManyItems(usize),
    #[error("database error: {0}")]
    Database(String),
}

impl MemoryCommitError {
    /// Stable code for `last_error_code` and structured logs.
    pub fn code(&self) -> &'static str {
        match self {
            Self::NotFound(_) => "conversation_not_found",
            Self::TranscriptConflict { .. } => "transcript_conflict",
            Self::MemoryConflict { .. } => "memory_conflict",
            Self::UnsupportedSchema(_) => "unsupported_schema",
            Self::UnresolvableEvidence(_) => "unresolvable_evidence",
            Self::TooManyItems(_) => "too_many_items",
            Self::Database(_) => "database_error",
        }
    }

    /// Whether one fresh-snapshot retry is worth attempting (§11.2).
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            Self::TranscriptConflict { .. } | Self::MemoryConflict { .. }
        )
    }
}

/// The authoritative ledger: consistent reads, bounded paging, atomic commit.
#[async_trait]
pub trait ConversationMemoryPort: Send + Sync {
    /// One consistent read of state, active items with their evidence, the
    /// working summary, and both revisions.
    ///
    /// A single call on purpose. Several independent reads can interleave with
    /// a commit and hand back the summary from revision N beside the ledger
    /// from N+1 — a pairing that has never been true (§11.1).
    async fn load_snapshot(&self, conversation_id: &str) -> Result<MemorySnapshot>;

    /// Original messages in `(after_sequence, through_sequence]`, oldest first,
    /// stopping at whichever limit binds first.
    async fn page_source_messages(
        &self,
        conversation_id: &str,
        after_sequence: i64,
        through_sequence: i64,
        limits: SourceReadLimits,
    ) -> Result<SourcePage>;

    /// Resolve stored spans back to source text.
    ///
    /// A span whose source changed comes back with `text: None` rather than an
    /// approximation, so a caller cannot accidentally render a quotation the
    /// user no longer has (invariant 15).
    async fn read_source_spans(
        &self,
        conversation_id: &str,
        spans: &[SourceSpanRef],
        limits: SourceReadLimits,
    ) -> Result<Vec<ResolvedSpan>>;

    /// Apply a validated candidate under `preconditions`, or fail without
    /// writing anything.
    async fn commit_memory(
        &self,
        preconditions: &MemoryCommitPreconditions,
        candidate: &MemoryCommitCandidate,
    ) -> std::result::Result<CommittedMemorySnapshot, MemoryCommitError>;

    /// Record that an attempt failed, without touching items or the watermark.
    ///
    /// Separate from `commit_memory` because a failed maintenance pass must not
    /// be able to mark an already-completed chat turn failed (invariant 16).
    async fn record_memory_error(&self, conversation_id: &str, code: &str) -> Result<()>;

    /// Mark a conversation as needing a rebuild, e.g. after a restore found an
    /// unsupported stored layout.
    async fn mark_rebuild_required(&self, conversation_id: &str, code: &str) -> Result<()>;

    /// Page inactive items for the history view. Active items come from the
    /// snapshot; this is the correction history behind them.
    async fn page_inactive_items(
        &self,
        conversation_id: &str,
        offset: i64,
        limit: i64,
    ) -> Result<Vec<crate::domain::conversation_memory::MemoryItem>>;

    /// Current state row without materializing items — cheap enough to consult
    /// on every turn.
    async fn load_state(&self, conversation_id: &str) -> Result<ConversationMemoryState>;
}

/// Recall over this conversation's original transcript (§9).
///
/// Scoped to one conversation by construction. The conversation identity comes
/// from trusted turn-execution context; a model-supplied id is never accepted,
/// which is what stops a tool call reaching into another thread.
#[async_trait]
pub trait ConversationMemoryReadPort: Send + Sync {
    /// Lexical and exact-identifier candidates, ranked.
    async fn search_source_messages(
        &self,
        conversation_id: &str,
        query: &str,
        exact_terms: &[String],
        limit: usize,
    ) -> Result<RecallCandidates>;

    /// Messages immediately around `sequence`, for explaining a short reply.
    async fn read_adjacent_turns(
        &self,
        conversation_id: &str,
        sequence: i64,
        before: usize,
        after: usize,
        limits: SourceReadLimits,
    ) -> Result<Vec<SourceMessage>>;

    /// Read specific messages by id, scoped to the conversation. Foreign ids
    /// are dropped rather than returned.
    async fn read_messages(
        &self,
        conversation_id: &str,
        message_ids: &[String],
        limits: SourceReadLimits,
    ) -> Result<Vec<SourceMessage>>;

    /// Read a contiguous sequence interval.
    async fn read_sequence_range(
        &self,
        conversation_id: &str,
        from_sequence: i64,
        to_sequence: i64,
        limits: SourceReadLimits,
    ) -> Result<SourcePage>;

    /// Search generated item labels, for the memory details view. Labels
    /// improve findability; they never stand in for evidence, and mandatory
    /// items never depend on this search.
    async fn search_memory_labels(
        &self,
        conversation_id: &str,
        query: &str,
        limit: usize,
    ) -> Result<Vec<MemoryId>>;
}

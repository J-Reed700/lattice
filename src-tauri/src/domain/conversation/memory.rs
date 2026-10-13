//! # Bounded conversation memory
//!
//! Durable, source-backed memory for a single conversation, so a long thread can
//! survive repeated compaction without a recursively rewritten summary becoming
//! the only record of what the user asked for.
//!
//! Design: `docs/design/2026-09-19-conversation-memory.md`.
//!
//! ## What this module owns
//!
//! The *values* and the *pure rules*: item identity, evidence spans, the
//! transitions an item may undergo, and the deterministic validation that turns
//! an untrusted model proposal into something a repository is allowed to commit.
//! Orchestration (calling a model, paging the transcript) belongs to the
//! application layer; SQL belongs to the repository.
//!
//! ## The guarantee
//!
//! Every authoritative memory item is backed by an [`EvidenceSpan`]: a byte
//! range inside an original message of *this* conversation, whose content still
//! hashes to what it hashed to when the span was recorded. A quotation is never
//! stored as text — it is resolved from the source on read, so deleting the
//! source deletes the quotation.
//!
//! This proves provenance. It does **not** prove that the model found every
//! constraint, or read the one it found correctly. Those are measured by the
//! evaluation suite, never asserted here.

use crate::shared::error::{AppError, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

mod apply;
mod patch;
mod redaction;
mod validation;

pub use apply::{apply_patch, MemoryCommit, SemanticVerdict};
pub use patch::{
    MemoryPatch, MemoryValidationError, ProposedEvidence, ProposedItem, ProposedTransition,
    ValidatedAddition, ValidatedPatch, ValidatedTransition, ValidatedTransitionTarget,
};
pub use redaction::{redact_suppressed, SuppressedSpan, FORGOTTEN_MARKER};
pub use validation::{parse_patch, validate_patch, SourceIndex};

/// Version of the persisted memory layout. Bump when a stored shape changes in
/// a way older code would misread; an unknown version forces a rebuild rather
/// than being parsed as empty memory (invariant 14).
pub const MEMORY_SCHEMA_VERSION: i64 = 1;

/// Prefix on every stored digest, so the hash function is part of the value and
/// a future change is detectable rather than silently compared across schemes.
pub const DIGEST_SCHEME: &str = "sha256:";

// ---------------------------------------------------------------------------
// Limits (§7.5). Named constants so tests assert against the contract, not a
// magic number repeated at the call site.
// ---------------------------------------------------------------------------

/// Maximum structured-output bytes accepted before parsing is attempted.
pub const MAX_PROPOSAL_BYTES: usize = 64 * 1024;
/// Maximum add + transition operations in one model response.
pub const MAX_OPERATIONS_PER_RESPONSE: usize = 32;
/// Maximum evidence spans attached to a single item.
pub const MAX_EVIDENCE_PER_ITEM: usize = 4;
/// Maximum bytes in one proposed quotation. Over-limit evidence is rejected,
/// never silently clipped — a clipped negation is a reversed requirement.
pub const MAX_QUOTE_BYTES: usize = 2048;
/// Maximum characters in a generated display label.
pub const MAX_LABEL_CHARS: usize = 200;
/// Maximum conflict/antecedent links recorded on one item.
pub const MAX_RELATED_ITEMS: usize = 8;
/// Host work limit when materializing active items. Exceeding it is an explicit
/// overflow error, never a silent `LIMIT`.
pub const MAX_ACTIVE_ITEMS: usize = 512;

// ---------------------------------------------------------------------------
// Identity
// ---------------------------------------------------------------------------

/// Host-generated, immutable identifier for a memory item.
///
/// The model never picks one: it proposes `candidate_id`s scoped to a single
/// response, and the host assigns durable IDs only after validation. That is
/// what stops old IDs being reused to silently change an item's meaning
/// (invariant 7).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MemoryId(String);

impl MemoryId {
    /// Mint a fresh identifier.
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }

    /// Rebuild an identifier read back from storage.
    pub fn from_string(value: impl Into<String>) -> Result<Self> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(AppError::InvalidInput("Memory id cannot be empty".into()));
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for MemoryId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for MemoryId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

// ---------------------------------------------------------------------------
// Classification
// ---------------------------------------------------------------------------

/// What kind of thing an item records.
///
/// The split that matters is [`MemoryKind::is_mandatory`]: mandatory items are
/// loaded into every prompt while active and are never evicted to make room for
/// optional material (invariant 12). Everything else is retrievable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryKind {
    /// A requirement or restriction, including negative ones ("do not deploy")
    /// and conditional ones ("not until I approve").
    Constraint,
    /// Something the user is trying to achieve.
    Goal,
    /// A settled choice that later work depends on.
    Decision,
    /// A fact the user asserted about themselves or their situation.
    UserFact,
    /// A stated preference. Mandatory only when the wording made it a
    /// requirement — that upgrade is a semantic judgement, not a string match.
    Preference,
    /// Something left unanswered.
    OpenQuestion,
    /// A fact about the world the conversation established from an explicit
    /// source — a cited page, a named document, a URL. The one kind whose
    /// authority may be an assistant passage, because that passage carries
    /// its provenance with it: later turns may rely on the fact without
    /// fetching the source again.
    EstablishedFact,
    /// Host-created. Records that valid source text could not support a
    /// confident transition, keeping both passages visible without pretending
    /// the ambiguity is settled.
    UnresolvedChange,
}

impl MemoryKind {
    /// Whether an *active* item of this kind must appear in every prompt.
    ///
    /// `UnresolvedChange` is mandatory because the alternative — hiding a
    /// conflict until someone asks — is how a restriction quietly stops being
    /// applied.
    pub fn is_mandatory(self) -> bool {
        matches!(
            self,
            Self::Constraint | Self::Goal | Self::Decision | Self::UnresolvedChange
        )
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Constraint => "constraint",
            Self::Goal => "goal",
            Self::Decision => "decision",
            Self::UserFact => "user_fact",
            Self::Preference => "preference",
            Self::OpenQuestion => "open_question",
            Self::EstablishedFact => "established_fact",
            Self::UnresolvedChange => "unresolved_change",
        }
    }
}

impl std::str::FromStr for MemoryKind {
    type Err = AppError;

    fn from_str(s: &str) -> Result<Self> {
        match s {
            "constraint" => Ok(Self::Constraint),
            "goal" => Ok(Self::Goal),
            "decision" => Ok(Self::Decision),
            "user_fact" => Ok(Self::UserFact),
            "preference" => Ok(Self::Preference),
            "open_question" => Ok(Self::OpenQuestion),
            "established_fact" => Ok(Self::EstablishedFact),
            "unresolved_change" => Ok(Self::UnresolvedChange),
            other => Err(AppError::InvalidInput(format!(
                "Unknown memory kind: {other}"
            ))),
        }
    }
}

/// Lifecycle position of an item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryState {
    /// Applies now.
    Active,
    /// Replaced by a later, source-backed item.
    Superseded,
    /// No longer outstanding (an answered question, a met goal).
    Resolved,
}

impl MemoryState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Superseded => "superseded",
            Self::Resolved => "resolved",
        }
    }
}

impl std::str::FromStr for MemoryState {
    type Err = AppError;

    fn from_str(s: &str) -> Result<Self> {
        match s {
            "active" => Ok(Self::Active),
            "superseded" => Ok(Self::Superseded),
            "resolved" => Ok(Self::Resolved),
            other => Err(AppError::InvalidInput(format!(
                "Unknown memory state: {other}"
            ))),
        }
    }
}

/// Whether the semantic review backed this item's interpretation.
///
/// Deliberately not a confidence score. A number invites a threshold, and a
/// threshold is a rule for making restrictions disappear quietly (§5.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryReview {
    /// A reviewer agreed the evidence supports this reading.
    Supported,
    /// The evidence is valid but its meaning is unsettled. Still mandatory when
    /// the item is mandatory; rendered as the raw passages, not a conclusion.
    Ambiguous,
}

impl MemoryReview {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Supported => "supported",
            Self::Ambiguous => "ambiguous",
        }
    }
}

impl std::str::FromStr for MemoryReview {
    type Err = AppError;

    fn from_str(s: &str) -> Result<Self> {
        match s {
            "supported" => Ok(Self::Supported),
            "ambiguous" => Ok(Self::Ambiguous),
            other => Err(AppError::InvalidInput(format!(
                "Unknown memory review: {other}"
            ))),
        }
    }
}

/// Why a span is attached to an item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidencePurpose {
    /// The passage that states the thing. A user-memory item needs at least one
    /// of these from a *user* message.
    Assertion,
    /// Context that makes a short reply legible — the assistant question a
    /// "yes, option B" was answering. Explains; never authorizes.
    Antecedent,
    /// The later passage that supersedes or resolves an earlier item.
    Transition,
}

impl EvidencePurpose {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Assertion => "assertion",
            Self::Antecedent => "antecedent",
            Self::Transition => "transition",
        }
    }
}

impl std::str::FromStr for EvidencePurpose {
    type Err = AppError;

    fn from_str(s: &str) -> Result<Self> {
        match s {
            "assertion" => Ok(Self::Assertion),
            "antecedent" => Ok(Self::Antecedent),
            "transition" => Ok(Self::Transition),
            other => Err(AppError::InvalidInput(format!(
                "Unknown evidence purpose: {other}"
            ))),
        }
    }
}

// ---------------------------------------------------------------------------
// Evidence
// ---------------------------------------------------------------------------

/// A byte range inside one original message.
///
/// Offsets are into the message's exact UTF-8 bytes, with no normalization of
/// whitespace, Unicode form, newlines, or case anywhere in this module. The
/// digest is of the whole message at the time the span was recorded: if the
/// message is edited the span stops resolving, which is what makes an edit
/// invalidate derived memory rather than silently re-point it at new text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidenceSpan {
    /// Message this span reads from. Always in the owning conversation.
    pub message_id: String,
    /// Sequence of that message, carried so ordering rules need no extra read.
    pub sequence: i64,
    /// Role of that message. A user assertion cannot come from an assistant.
    pub role: SourceRole,
    /// Inclusive start, on a UTF-8 character boundary.
    pub start_byte: u32,
    /// Exclusive end, on a UTF-8 character boundary. Always `> start_byte`.
    pub end_byte: u32,
    /// `sha256:`-prefixed digest of the *whole* message content.
    pub content_digest: String,
    pub purpose: EvidencePurpose,
}

impl EvidenceSpan {
    /// Resolve this span against live message content.
    ///
    /// Returns `None` when the content no longer hashes to `content_digest` or
    /// the offsets no longer land on character boundaries. A caller that gets
    /// `None` must treat the item as unsupported — never fall back to a cached
    /// copy of the quotation, because that is how deleted text comes back
    /// (invariant 15).
    pub fn resolve<'a>(&self, content: &'a str) -> Option<&'a str> {
        if compute_digest(content) != self.content_digest {
            return None;
        }
        let (start, end) = (self.start_byte as usize, self.end_byte as usize);
        if end <= start || end > content.len() {
            return None;
        }
        if !content.is_char_boundary(start) || !content.is_char_boundary(end) {
            return None;
        }
        Some(&content[start..end])
    }

    pub fn byte_len(&self) -> usize {
        self.end_byte.saturating_sub(self.start_byte) as usize
    }
}

/// Role of a source message, restated here so the domain does not depend on the
/// conversation aggregate's representation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceRole {
    User,
    Assistant,
    System,
}

impl SourceRole {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Assistant => "assistant",
            Self::System => "system",
        }
    }
}

impl std::str::FromStr for SourceRole {
    type Err = AppError;

    fn from_str(s: &str) -> Result<Self> {
        match s {
            "user" => Ok(Self::User),
            "assistant" => Ok(Self::Assistant),
            "system" => Ok(Self::System),
            other => Err(AppError::InvalidInput(format!("Unknown role: {other}"))),
        }
    }
}

/// The one framing for a generated summary anywhere in a prompt.
///
/// Three code paths put a working summary in front of a model — the chat
/// assembler, the QA context manager and the context-window builder — and they
/// used to word it two different ways, one of which never said the text was
/// written by a model. A summary presented without that caveat reads as
/// transcript, and the model then treats a paraphrase as something the user
/// actually said. That is the failure this single definition prevents.
///
/// The `[generated summary` opening is load-bearing as well as informative: the
/// assembler finds the summary by that prefix when it has to evict something.
/// Reword inside the bracket freely; changing how it starts changes what can be
/// evicted under pressure.
pub fn frame_generated_summary(summary: &str) -> String {
    format!(
        "[generated summary of earlier conversation \u{2014} written by a model, may be \
         incomplete or wrong; original messages remain searchable]\n{}",
        summary.trim()
    )
}

/// Digest of exact message content, scheme prefix included.
pub fn compute_digest(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    format!("{DIGEST_SCHEME}{:x}", hasher.finalize())
}

// ---------------------------------------------------------------------------
// Items
// ---------------------------------------------------------------------------

/// One durable memory record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryItem {
    pub id: MemoryId,
    pub conversation_id: String,
    pub kind: MemoryKind,
    pub state: MemoryState,
    /// Short generated text for search and display. **Not** evidence: renderers
    /// must mark it as a label so it cannot be mistaken for a quotation.
    pub label: String,
    pub evidence: Vec<EvidenceSpan>,
    /// Lowest assertion sequence. Derived from evidence in host code; a model
    /// cannot assert when something was said.
    pub created_at_sequence: i64,
    /// Highest sequence among all this item's evidence, including transitions.
    pub changed_at_sequence: i64,
    pub superseded_by: Option<MemoryId>,
    pub revision: i64,
    pub review: MemoryReview,
    /// Bounded conflict/antecedent links. A conflict link is not a supersession.
    pub related_item_ids: Vec<MemoryId>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl MemoryItem {
    /// Whether this item must be in the prompt right now.
    pub fn is_mandatory_now(&self) -> bool {
        self.state == MemoryState::Active && self.kind.is_mandatory()
    }

    /// Spans that state the item, as opposed to explaining or retiring it.
    pub fn assertions(&self) -> impl Iterator<Item = &EvidenceSpan> {
        self.evidence
            .iter()
            .filter(|span| span.purpose == EvidencePurpose::Assertion)
    }

    /// True when at least one assertion came from the user.
    ///
    /// Enforced on the way in, and re-checked on the way out: an item that lost
    /// its user assertion through an edit is not a user requirement any more.
    pub fn has_user_assertion(&self) -> bool {
        self.assertions().any(|span| span.role == SourceRole::User)
    }
}

// ---------------------------------------------------------------------------
// Conversation-level state
// ---------------------------------------------------------------------------

/// Whether stored memory can be used as-is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryValidity {
    /// Usable.
    Ready,
    /// Source material changed underneath it, or the stored schema is unknown.
    /// Memory is not consulted until it has been rebuilt from originals.
    RebuildRequired,
}

impl MemoryValidity {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ready => "ready",
            Self::RebuildRequired => "rebuild_required",
        }
    }
}

impl std::str::FromStr for MemoryValidity {
    type Err = AppError;

    fn from_str(s: &str) -> Result<Self> {
        match s {
            "ready" => Ok(Self::Ready),
            "rebuild_required" => Ok(Self::RebuildRequired),
            other => Err(AppError::InvalidInput(format!(
                "Unknown memory validity: {other}"
            ))),
        }
    }
}

/// Per-conversation memory bookkeeping (§5.3).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConversationMemoryState {
    pub conversation_id: String,
    pub schema_version: i64,
    pub memory_revision: i64,
    /// Transcript revision the last successful build read.
    pub source_transcript_revision: i64,
    /// Everything at or below this sequence has been *submitted* for
    /// extraction. A watermark records processing, not that the model noticed
    /// anything in particular.
    pub processed_through_sequence: i64,
    pub validity: MemoryValidity,
    /// Machine-readable code only. Never raw transcript text.
    pub last_error_code: Option<String>,
    pub extractor_model_identity: Option<String>,
    pub extractor_prompt_version: Option<String>,
    pub validator_version: Option<String>,
    pub updated_at: DateTime<Utc>,
}

impl ConversationMemoryState {
    /// The state a conversation has before anything has been extracted.
    pub fn empty(conversation_id: impl Into<String>) -> Self {
        Self {
            conversation_id: conversation_id.into(),
            schema_version: MEMORY_SCHEMA_VERSION,
            memory_revision: 0,
            source_transcript_revision: 0,
            processed_through_sequence: 0,
            validity: MemoryValidity::Ready,
            last_error_code: None,
            extractor_model_identity: None,
            extractor_prompt_version: None,
            validator_version: None,
            updated_at: Utc::now(),
        }
    }

    /// Whether the stored layout is one this build understands.
    ///
    /// A *newer* schema is not read as empty memory: that would silently drop
    /// every constraint a later version recorded (invariant 14).
    pub fn is_schema_supported(&self) -> bool {
        self.schema_version == MEMORY_SCHEMA_VERSION
    }
}

/// One original message, as the memory layer sees it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceMessage {
    pub id: String,
    pub conversation_id: String,
    pub sequence: i64,
    pub role: SourceRole,
    pub content: String,
    pub content_digest: String,
    /// `pending`, `completed` or `failed`, from the message row.
    pub status: String,
}

impl SourceMessage {
    /// Whether this message may supply evidence.
    ///
    /// A *failed assistant output* is excluded: it is not an answer that was
    /// returned. A pending or failed *user* message is not — the user still
    /// said it, and a generation failure must not erase their "do not send"
    /// (§7.2). This is the deliberate divergence from the blanket
    /// `is_completed()` history filter, and it is tested.
    pub fn is_eligible_source(&self) -> bool {
        match self.role {
            SourceRole::User | SourceRole::System => true,
            SourceRole::Assistant => self.status == "completed",
        }
    }

    /// Whether the recorded digest still matches the content.
    pub fn digest_matches(&self) -> bool {
        compute_digest(&self.content) == self.content_digest
    }
}

/// A consistent read of everything the memory layer needs (§14).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemorySnapshot {
    pub state: ConversationMemoryState,
    pub transcript_revision: i64,
    /// Active items only. Inactive history is paged separately.
    pub active_items: Vec<MemoryItem>,
    /// Working summary text, if a compaction has produced one.
    pub summary: Option<String>,
    /// Sequence of the message the summary was folded through (its
    /// `up_to_message_id`). `None` when there is no summary, or when its
    /// boundary cannot be placed.
    ///
    /// Normally the summary ends at the watermark. A delete or a user
    /// correction can drop the summary while the ledger stays usable, and
    /// source between this point and the watermark is then covered by
    /// extraction but by no summary until it is folded again.
    pub summary_through_sequence: Option<i64>,
    /// Sequence of the newest message in the transcript.
    pub latest_sequence: i64,
}

impl MemorySnapshot {
    /// The items that must reach every prompt, oldest assertion first so the
    /// rendered order matches the order the user said them in.
    pub fn mandatory_items(&self) -> Vec<&MemoryItem> {
        let mut items: Vec<&MemoryItem> = self
            .active_items
            .iter()
            .filter(|item| item.is_mandatory_now())
            .collect();
        items.sort_by_key(|item| (item.created_at_sequence, item.id.clone()));
        items
    }

    /// Active, non-mandatory items: candidates for the optional pool.
    pub fn optional_items(&self) -> Vec<&MemoryItem> {
        let mut items: Vec<&MemoryItem> = self
            .active_items
            .iter()
            .filter(|item| item.state == MemoryState::Active && !item.kind.is_mandatory())
            .collect();
        items.sort_by_key(|item| (item.created_at_sequence, item.id.clone()));
        items
    }

    /// Whether memory may be consulted at all.
    pub fn is_usable(&self) -> bool {
        self.state.validity == MemoryValidity::Ready && self.state.is_schema_supported()
    }
}

#[cfg(test)]
mod tests;

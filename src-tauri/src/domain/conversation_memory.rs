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
use std::collections::{HashMap, HashSet};

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

// ---------------------------------------------------------------------------
// Model proposals (§6.1) — untrusted input
// ---------------------------------------------------------------------------

/// A patch as returned by the extractor. Every field here is untrusted.
///
/// `deny_unknown_fields` throughout: an unrecognized key means the model is
/// answering a schema we did not ask for, and guessing which half to keep is
/// how a malformed response becomes a half-applied one (§6.2 rule 2).
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MemoryPatch {
    pub schema_version: i64,
    #[serde(default)]
    pub add: Vec<ProposedItem>,
    #[serde(default)]
    pub transitions: Vec<ProposedTransition>,
    /// Every segment the model was handed, echoed back. A short list means the
    /// response is truncated or the model skipped input — detectable, unlike a
    /// semantic omission.
    #[serde(default)]
    pub processed_segment_ids: Vec<String>,
}

/// A proposed new item.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProposedItem {
    /// Scoped to this one response; transitions may reference it as a
    /// replacement. Never persisted.
    pub candidate_id: String,
    pub kind: String,
    pub label: String,
    pub evidence: Vec<ProposedEvidence>,
}

/// A proposed quotation. The host computes offsets; the model supplies text.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProposedEvidence {
    pub message_id: String,
    /// Exact source substring. Anything that is not found verbatim is rejected;
    /// there is no fuzzy match, because a near-match is a different sentence.
    pub quote: String,
    /// Zero-based index among identical matches. May be omitted when unique.
    #[serde(default)]
    pub occurrence: Option<u32>,
    pub purpose: String,
}

/// A proposed state change to an existing item or to an earlier candidate in
/// this same response.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProposedTransition {
    /// Durable ID of an existing active item, or `candidate_id` of an addition
    /// in this response. Same-response targets let one chronological batch
    /// retain an original value as history and supersede it with a correction.
    pub item_id: String,
    /// `superseded` or `resolved`.
    pub state: String,
    /// `candidate_id` of the item replacing it, when there is one.
    #[serde(default)]
    pub replacement_candidate_id: Option<String>,
    /// Later user passage that justifies the change.
    pub evidence: Vec<ProposedEvidence>,
}

// ---------------------------------------------------------------------------
// Validation outcome
// ---------------------------------------------------------------------------

/// Why a patch was rejected. Codes are stable and contain no transcript text,
/// so they can be logged and persisted in `last_error_code`.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum MemoryValidationError {
    #[error("response exceeds {MAX_PROPOSAL_BYTES} bytes")]
    ResponseTooLarge,
    #[error("response is not valid JSON for the memory patch schema: {0}")]
    Malformed(String),
    #[error("unsupported patch schema_version {0}")]
    UnsupportedSchemaVersion(i64),
    #[error("{0} operations exceeds the limit of {MAX_OPERATIONS_PER_RESPONSE}")]
    TooManyOperations(usize),
    #[error("item '{0}' has {1} evidence spans, limit is {MAX_EVIDENCE_PER_ITEM}")]
    TooMuchEvidence(String, usize),
    #[error("item '{0}' has no evidence")]
    NoEvidence(String),
    #[error("quote in '{0}' is {1} bytes, limit is {MAX_QUOTE_BYTES}")]
    QuoteTooLong(String, usize),
    #[error("label in '{0}' exceeds {MAX_LABEL_CHARS} characters")]
    LabelTooLong(String),
    #[error("duplicate candidate_id '{0}'")]
    DuplicateCandidate(String),
    #[error("candidates '{0}' and '{1}' propose the same memory item")]
    DuplicateAddition(String, String),
    #[error("message '{0}' is not an eligible source in this conversation")]
    UnknownSource(String),
    #[error("quote for message '{0}' does not occur in that message")]
    QuoteNotFound(String),
    #[error("quote for message '{0}' has no occurrence {1}")]
    OccurrenceNotFound(String, u32),
    #[error("quote for message '{0}' is ambiguous; {1} occurrences and none selected")]
    AmbiguousOccurrence(String, usize),
    #[error("message '{0}' changed since the extraction snapshot")]
    StaleSource(String),
    #[error("item '{0}' has no user assertion; assistant or tool text cannot create one")]
    NoUserAssertion(String),
    #[error("item '{0}' uses assistant or system text as authoritative evidence")]
    NonUserAuthority(String),
    #[error("item '{item}' uses evidence purpose '{actual}' where '{expected}' is required")]
    InvalidEvidencePurpose {
        item: String,
        expected: &'static str,
        actual: String,
    },
    #[error("message '{0}' is a failed assistant output and cannot be evidence")]
    IneligibleSource(String),
    #[error("transition targets unknown or inactive item '{0}'")]
    UnknownTransitionTarget(String),
    #[error("transition evidence for '{0}' is not later than the assertion it retires")]
    TransitionNotLater(String),
    #[error("transition for '{0}' names itself as its replacement")]
    SelfSupersession(String),
    #[error("transition for '{0}' would create a supersession cycle")]
    SupersessionCycle(String),
    #[error("conflicting transitions for item '{0}'")]
    ConflictingTransitions(String),
    #[error("transition for '{0}' names unknown replacement candidate '{1}'")]
    UnknownReplacement(String, String),
    #[error("superseded transition for '{0}' does not name a replacement candidate")]
    SupersessionWithoutReplacement(String),
    #[error("unknown {0} value '{1}'")]
    UnknownEnum(&'static str, String),
    #[error("model reported {reported} of {expected} submitted segments")]
    IncompleteCoverage { reported: usize, expected: usize },
}

impl MemoryValidationError {
    /// Stable short code for logs and `last_error_code`.
    pub fn code(&self) -> &'static str {
        match self {
            Self::ResponseTooLarge => "response_too_large",
            Self::Malformed(_) => "malformed_json",
            Self::UnsupportedSchemaVersion(_) => "unsupported_schema_version",
            Self::TooManyOperations(_) => "too_many_operations",
            Self::TooMuchEvidence(_, _) => "too_much_evidence",
            Self::NoEvidence(_) => "no_evidence",
            Self::QuoteTooLong(_, _) => "quote_too_long",
            Self::LabelTooLong(_) => "label_too_long",
            Self::DuplicateCandidate(_) => "duplicate_candidate",
            Self::DuplicateAddition(_, _) => "duplicate_addition",
            Self::UnknownSource(_) => "unknown_source",
            Self::QuoteNotFound(_) => "quote_not_found",
            Self::OccurrenceNotFound(_, _) => "occurrence_not_found",
            Self::AmbiguousOccurrence(_, _) => "ambiguous_occurrence",
            Self::StaleSource(_) => "stale_source",
            Self::NoUserAssertion(_) => "no_user_assertion",
            Self::NonUserAuthority(_) => "non_user_authority",
            Self::InvalidEvidencePurpose { .. } => "invalid_evidence_purpose",
            Self::IneligibleSource(_) => "ineligible_source",
            Self::UnknownTransitionTarget(_) => "unknown_transition_target",
            Self::TransitionNotLater(_) => "transition_not_later",
            Self::SelfSupersession(_) => "self_supersession",
            Self::SupersessionCycle(_) => "supersession_cycle",
            Self::ConflictingTransitions(_) => "conflicting_transitions",
            Self::UnknownReplacement(_, _) => "unknown_replacement",
            Self::SupersessionWithoutReplacement(_) => "supersession_without_replacement",
            Self::UnknownEnum(_, _) => "unknown_enum",
            Self::IncompleteCoverage { .. } => "incomplete_coverage",
        }
    }
}

/// A new item that survived deterministic validation and is waiting for
/// semantic review before it can be committed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedAddition {
    /// The response-scoped ID, kept so transitions can name this addition.
    pub candidate_id: String,
    pub kind: MemoryKind,
    pub label: String,
    pub evidence: Vec<EvidenceSpan>,
}

impl ValidatedAddition {
    pub fn has_user_assertion(&self) -> bool {
        self.evidence
            .iter()
            .any(|span| span.purpose == EvidencePurpose::Assertion && span.role == SourceRole::User)
    }

    /// Lowest assertion sequence, or lowest sequence when there is somehow no
    /// assertion (the caller has already rejected that case for user memory).
    pub fn created_at_sequence(&self) -> i64 {
        self.evidence
            .iter()
            .filter(|span| span.purpose == EvidencePurpose::Assertion)
            .map(|span| span.sequence)
            .min()
            .or_else(|| self.evidence.iter().map(|span| span.sequence).min())
            .unwrap_or(0)
    }

    pub fn changed_at_sequence(&self) -> i64 {
        self.evidence
            .iter()
            .map(|span| span.sequence)
            .max()
            .unwrap_or(0)
    }
}

/// A state change that survived deterministic validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedTransition {
    pub target: ValidatedTransitionTarget,
    pub state: MemoryState,
    pub replacement_candidate_id: Option<String>,
    pub evidence: Vec<EvidenceSpan>,
}

/// A transition target after deterministic validation has established where
/// it comes from.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ValidatedTransitionTarget {
    Existing(MemoryId),
    Candidate(String),
}

impl ValidatedTransitionTarget {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Existing(id) => id.as_str(),
            Self::Candidate(id) => id.as_str(),
        }
    }
}

/// Everything one extraction pass proposes, after deterministic checks.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ValidatedPatch {
    pub additions: Vec<ValidatedAddition>,
    pub transitions: Vec<ValidatedTransition>,
    pub processed_segment_ids: Vec<String>,
}

// ---------------------------------------------------------------------------
// Deterministic validation (§6.2)
// ---------------------------------------------------------------------------

/// Sources an extraction pass was allowed to quote, indexed for lookup.
pub struct SourceIndex<'a> {
    by_id: HashMap<&'a str, &'a SourceMessage>,
}

impl<'a> SourceIndex<'a> {
    pub fn new(messages: &'a [SourceMessage]) -> Self {
        Self {
            by_id: messages
                .iter()
                .map(|message| (message.id.as_str(), message))
                .collect(),
        }
    }

    pub fn get(&self, id: &str) -> Option<&'a SourceMessage> {
        self.by_id.get(id).copied()
    }
}

/// Byte offsets of the `occurrence`-th exact match of `needle` in `haystack`.
///
/// Exact substring search over bytes, then a boundary check. Searching bytes
/// rather than characters is deliberate: it is the only way to be sure the
/// offsets we store are the offsets the text actually has.
fn locate_quote(
    haystack: &str,
    needle: &str,
    occurrence: Option<u32>,
    message_id: &str,
) -> std::result::Result<(u32, u32), MemoryValidationError> {
    if needle.is_empty() {
        return Err(MemoryValidationError::QuoteNotFound(message_id.to_string()));
    }
    let mut starts = Vec::new();
    let mut from = 0usize;
    while let Some(found) = haystack[from..].find(needle) {
        let absolute = from + found;
        starts.push(absolute);
        // Overlapping matches are still distinct occurrences, so advance by one
        // character rather than by the needle's length.
        from = absolute
            + haystack[absolute..]
                .chars()
                .next()
                .map_or(1, char::len_utf8);
        if from >= haystack.len() {
            break;
        }
    }
    if starts.is_empty() {
        return Err(MemoryValidationError::QuoteNotFound(message_id.to_string()));
    }
    let index = match occurrence {
        Some(index) => index as usize,
        None if starts.len() == 1 => 0,
        None => {
            return Err(MemoryValidationError::AmbiguousOccurrence(
                message_id.to_string(),
                starts.len(),
            ))
        }
    };
    let start = *starts.get(index).ok_or_else(|| {
        MemoryValidationError::OccurrenceNotFound(
            message_id.to_string(),
            occurrence.unwrap_or_default(),
        )
    })?;
    let end = start + needle.len();
    // `find` returns character-aligned offsets for valid UTF-8, but the check is
    // cheap and this is the invariant every later read depends on.
    if !haystack.is_char_boundary(start) || !haystack.is_char_boundary(end) {
        return Err(MemoryValidationError::QuoteNotFound(message_id.to_string()));
    }
    Ok((start as u32, end as u32))
}

/// Turn one proposed quotation into a span, or say exactly why it cannot be one.
fn validate_evidence(
    proposed: &ProposedEvidence,
    sources: &SourceIndex<'_>,
    owner: &str,
) -> std::result::Result<EvidenceSpan, MemoryValidationError> {
    if proposed.quote.len() > MAX_QUOTE_BYTES {
        return Err(MemoryValidationError::QuoteTooLong(
            owner.to_string(),
            proposed.quote.len(),
        ));
    }
    let purpose: EvidencePurpose = proposed
        .purpose
        .parse()
        .map_err(|_| MemoryValidationError::UnknownEnum("purpose", proposed.purpose.clone()))?;

    let message = sources
        .get(&proposed.message_id)
        .ok_or_else(|| MemoryValidationError::UnknownSource(proposed.message_id.clone()))?;

    if !message.is_eligible_source() {
        return Err(MemoryValidationError::IneligibleSource(
            proposed.message_id.clone(),
        ));
    }
    // The snapshot's digest is what the extractor was shown. If the live row no
    // longer matches it, the text moved under us mid-job.
    if !message.digest_matches() {
        return Err(MemoryValidationError::StaleSource(
            proposed.message_id.clone(),
        ));
    }

    let (start_byte, end_byte) = locate_quote(
        &message.content,
        &proposed.quote,
        proposed.occurrence,
        &proposed.message_id,
    )?;

    Ok(EvidenceSpan {
        message_id: message.id.clone(),
        sequence: message.sequence,
        role: message.role,
        start_byte,
        end_byte,
        content_digest: message.content_digest.clone(),
        purpose,
    })
}

/// Everything §6.2 requires, applied to one parsed patch.
///
/// The patch is accepted or rejected as a unit. There is no partial application:
/// a half-applied patch is a ledger that claims a coverage it does not have.
pub fn validate_patch(
    patch: &MemoryPatch,
    sources: &SourceIndex<'_>,
    active_items: &[MemoryItem],
    submitted_segment_ids: &[String],
) -> std::result::Result<ValidatedPatch, MemoryValidationError> {
    if patch.schema_version != MEMORY_SCHEMA_VERSION {
        return Err(MemoryValidationError::UnsupportedSchemaVersion(
            patch.schema_version,
        ));
    }
    let operations = patch.add.len() + patch.transitions.len();
    if operations > MAX_OPERATIONS_PER_RESPONSE {
        return Err(MemoryValidationError::TooManyOperations(operations));
    }

    // --- additions -------------------------------------------------------
    let mut seen_candidates: HashSet<&str> = HashSet::new();
    let mut additions = Vec::with_capacity(patch.add.len());
    for proposed in &patch.add {
        if !seen_candidates.insert(proposed.candidate_id.as_str()) {
            return Err(MemoryValidationError::DuplicateCandidate(
                proposed.candidate_id.clone(),
            ));
        }
        if proposed.label.chars().count() > MAX_LABEL_CHARS {
            return Err(MemoryValidationError::LabelTooLong(
                proposed.candidate_id.clone(),
            ));
        }
        if proposed.evidence.is_empty() {
            return Err(MemoryValidationError::NoEvidence(
                proposed.candidate_id.clone(),
            ));
        }
        if proposed.evidence.len() > MAX_EVIDENCE_PER_ITEM {
            return Err(MemoryValidationError::TooMuchEvidence(
                proposed.candidate_id.clone(),
                proposed.evidence.len(),
            ));
        }
        let kind: MemoryKind = proposed
            .kind
            .parse()
            .map_err(|_| MemoryValidationError::UnknownEnum("kind", proposed.kind.clone()))?;
        // The host owns this kind. A model claiming to have found an unresolved
        // change is a model deciding its own uncertainty is settled policy.
        if kind == MemoryKind::UnresolvedChange {
            return Err(MemoryValidationError::UnknownEnum(
                "kind",
                proposed.kind.clone(),
            ));
        }

        let mut evidence = Vec::with_capacity(proposed.evidence.len());
        for item in &proposed.evidence {
            let span = validate_evidence(item, sources, &proposed.candidate_id)?;
            match span.purpose {
                EvidencePurpose::Assertion if span.role != SourceRole::User => {
                    return Err(MemoryValidationError::NonUserAuthority(
                        proposed.candidate_id.clone(),
                    ));
                }
                EvidencePurpose::Assertion | EvidencePurpose::Antecedent => evidence.push(span),
                EvidencePurpose::Transition => {
                    return Err(MemoryValidationError::InvalidEvidencePurpose {
                        item: proposed.candidate_id.clone(),
                        expected: "assertion or antecedent",
                        actual: span.purpose.as_str().to_string(),
                    });
                }
            }
        }

        let addition = ValidatedAddition {
            candidate_id: proposed.candidate_id.clone(),
            kind,
            label: proposed.label.trim().to_string(),
            evidence,
        };
        // Rule 7: assistant text, tool output and quoted third parties can
        // explain a user memory but cannot create one.
        if !addition.has_user_assertion() {
            return Err(MemoryValidationError::NoUserAssertion(
                proposed.candidate_id.clone(),
            ));
        }
        if let Some(existing) = additions.iter().find(|existing: &&ValidatedAddition| {
            existing.kind == addition.kind
                && existing.label.eq_ignore_ascii_case(&addition.label)
                && existing.evidence == addition.evidence
        }) {
            return Err(MemoryValidationError::DuplicateAddition(
                existing.candidate_id.clone(),
                addition.candidate_id.clone(),
            ));
        }
        additions.push(addition);
    }

    // --- transitions -----------------------------------------------------
    let active_by_id: HashMap<&str, &MemoryItem> = active_items
        .iter()
        .filter(|item| item.state == MemoryState::Active)
        .map(|item| (item.id.as_str(), item))
        .collect();
    let candidates_by_id: HashMap<&str, &ValidatedAddition> = additions
        .iter()
        .map(|addition| (addition.candidate_id.as_str(), addition))
        .collect();

    let mut targeted: HashSet<&str> = HashSet::new();
    let mut transitions = Vec::with_capacity(patch.transitions.len());
    for proposed in &patch.transitions {
        let (target, target_changed_at) =
            if let Some(item) = active_by_id.get(proposed.item_id.as_str()) {
                (
                    ValidatedTransitionTarget::Existing(item.id.clone()),
                    item.changed_at_sequence,
                )
            } else if let Some(candidate) = candidates_by_id.get(proposed.item_id.as_str()) {
                (
                    ValidatedTransitionTarget::Candidate(candidate.candidate_id.clone()),
                    candidate.changed_at_sequence(),
                )
            } else {
                return Err(MemoryValidationError::UnknownTransitionTarget(
                    proposed.item_id.clone(),
                ));
            };
        if !targeted.insert(proposed.item_id.as_str()) {
            return Err(MemoryValidationError::ConflictingTransitions(
                proposed.item_id.clone(),
            ));
        }
        let state: MemoryState = proposed
            .state
            .parse()
            .map_err(|_| MemoryValidationError::UnknownEnum("state", proposed.state.clone()))?;
        if state == MemoryState::Active {
            return Err(MemoryValidationError::UnknownEnum(
                "state",
                proposed.state.clone(),
            ));
        }
        if state == MemoryState::Superseded && proposed.replacement_candidate_id.is_none() {
            return Err(MemoryValidationError::SupersessionWithoutReplacement(
                proposed.item_id.clone(),
            ));
        }
        if proposed.evidence.is_empty() {
            return Err(MemoryValidationError::NoEvidence(proposed.item_id.clone()));
        }
        if proposed.evidence.len() > MAX_EVIDENCE_PER_ITEM {
            return Err(MemoryValidationError::TooMuchEvidence(
                proposed.item_id.clone(),
                proposed.evidence.len(),
            ));
        }
        if let Some(replacement) = &proposed.replacement_candidate_id {
            if !candidates_by_id.contains_key(replacement.as_str()) {
                return Err(MemoryValidationError::UnknownReplacement(
                    proposed.item_id.clone(),
                    replacement.clone(),
                ));
            }
            if target.as_str() == replacement {
                return Err(MemoryValidationError::SelfSupersession(
                    proposed.item_id.clone(),
                ));
            }
        }

        let mut evidence = Vec::with_capacity(proposed.evidence.len());
        for item in &proposed.evidence {
            let span = validate_evidence(item, sources, &proposed.item_id)?;
            if span.purpose != EvidencePurpose::Transition {
                return Err(MemoryValidationError::InvalidEvidencePurpose {
                    item: proposed.item_id.clone(),
                    expected: "transition",
                    actual: span.purpose.as_str().to_string(),
                });
            }
            if span.role != SourceRole::User {
                return Err(MemoryValidationError::NonUserAuthority(
                    proposed.item_id.clone(),
                ));
            }
            evidence.push(span);
        }
        // Rule 9: a correction has to come *after* what it corrects. Without
        // this, wording similar to an earlier message could retire a later one.
        let latest = evidence
            .iter()
            .map(|span| span.sequence)
            .max()
            .unwrap_or_default();
        if latest <= target_changed_at {
            return Err(MemoryValidationError::TransitionNotLater(
                proposed.item_id.clone(),
            ));
        }
        // A user retirement needs a user source, for the same reason an
        // assertion does.
        transitions.push(ValidatedTransition {
            target,
            state,
            replacement_candidate_id: proposed.replacement_candidate_id.clone(),
            evidence,
        });
    }

    check_supersession_acyclic(&transitions, active_items)?;

    // --- coverage --------------------------------------------------------
    // Rule 12. This catches a truncated or skipped response. It cannot catch a
    // model that read every segment and noticed nothing in them.
    if !submitted_segment_ids.is_empty() {
        let reported: HashSet<&str> = patch
            .processed_segment_ids
            .iter()
            .map(String::as_str)
            .collect();
        let missing = submitted_segment_ids
            .iter()
            .filter(|id| !reported.contains(id.as_str()))
            .count();
        if missing > 0 {
            return Err(MemoryValidationError::IncompleteCoverage {
                reported: submitted_segment_ids.len() - missing,
                expected: submitted_segment_ids.len(),
            });
        }
    }

    Ok(ValidatedPatch {
        additions,
        transitions,
        processed_segment_ids: patch.processed_segment_ids.clone(),
    })
}

/// Reject self-replacement and any cycle through the existing chain.
///
/// New additions have no durable ID yet, so a proposed replacement can only
/// close a loop by pointing back through items that already exist — which is
/// what this walks.
fn check_supersession_acyclic(
    transitions: &[ValidatedTransition],
    active_items: &[MemoryItem],
) -> std::result::Result<(), MemoryValidationError> {
    let existing: HashMap<&str, Option<&MemoryId>> = active_items
        .iter()
        .map(|item| (item.id.as_str(), item.superseded_by.as_ref()))
        .collect();

    let proposed: HashMap<&str, &str> = transitions
        .iter()
        .filter_map(|transition| {
            transition
                .replacement_candidate_id
                .as_deref()
                .map(|replacement| (transition.target.as_str(), replacement))
        })
        .collect();

    for transition in transitions {
        // Walk both the proposed same-response edges and the stored durable
        // chain. Candidate-to-candidate transitions make cycles possible
        // before either candidate has a durable ID, so both graphs matter.
        let mut seen = HashSet::new();
        let mut cursor = Some(transition.target.as_str());
        while let Some(current) = cursor {
            if !seen.insert(current) {
                return Err(MemoryValidationError::SupersessionCycle(
                    transition.target.as_str().to_string(),
                ));
            }
            cursor = proposed.get(current).copied().or_else(|| {
                existing
                    .get(current)
                    .and_then(|next| next.map(MemoryId::as_str))
            });
        }
    }
    Ok(())
}

/// Parse a model response into a patch, tolerating exactly one Markdown fence.
///
/// No other repair: trimming a malformed response until it parses is how a
/// truncated list of constraints becomes an accepted one (§6.2 rule 2).
pub fn parse_patch(raw: &str) -> std::result::Result<MemoryPatch, MemoryValidationError> {
    if raw.len() > MAX_PROPOSAL_BYTES {
        return Err(MemoryValidationError::ResponseTooLarge);
    }
    let trimmed = raw.trim();
    let body = match trimmed.strip_prefix("```") {
        Some(rest) => {
            // ```json\n...\n``` — drop the language tag and the closing fence.
            let rest = rest.split_once('\n').map_or(rest, |(_, body)| body);
            rest.trim_end()
                .strip_suffix("```")
                .ok_or_else(|| MemoryValidationError::Malformed("unterminated code fence".into()))?
        }
        None => trimmed,
    };
    serde_json::from_str(body.trim())
        .map_err(|error| MemoryValidationError::Malformed(error.to_string()))
}

// ---------------------------------------------------------------------------
// Applying a patch to a ledger (§8)
// ---------------------------------------------------------------------------

/// The complete, ordered set of changes to commit in one transaction.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MemoryCommit {
    /// Brand new items, IDs already assigned.
    pub inserts: Vec<MemoryItem>,
    /// Existing items whose state/`superseded_by` changes, plus any transition
    /// evidence appended to them.
    pub updates: Vec<MemoryItem>,
    /// New working summary, when this commit also rewrites it.
    pub summary: Option<String>,
    /// Watermark this commit advances to.
    pub processed_through_sequence: i64,
}

impl MemoryCommit {
    pub fn is_empty(&self) -> bool {
        self.inserts.is_empty() && self.updates.is_empty() && self.summary.is_none()
    }
}

/// Verdict from the separate semantic review step (§6.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SemanticVerdict {
    /// The evidence supports the proposed reading.
    Supported,
    /// The spans are real but the meaning is unsettled.
    Ambiguous,
    /// The proposal misreads its own evidence.
    Unsupported,
}

/// Fold a validated patch and its reviews into a committable change set.
///
/// The rules this encodes, all of them from §8:
///
/// * An item the model did not mention stays exactly as it was. Omission is
///   never deletion (invariant 6).
/// * An `Unsupported` addition is dropped; an `Ambiguous` one is kept but marked
///   [`MemoryReview::Ambiguous`], because throwing away a requirement we could
///   not interpret is worse than carrying it with its raw passages.
/// * An `Ambiguous` *transition* does not retire anything. It produces a
///   host-owned [`MemoryKind::UnresolvedChange`] holding both the old item's
///   evidence and the proposed transition evidence, so the conflict is visible
///   instead of resolved by guess. An `Unsupported` transition is ignored; its
///   source remains covered by the working summary without letting the rejected
///   operation stall other supported memory from the same batch.
pub fn apply_patch(
    conversation_id: &str,
    patch: &ValidatedPatch,
    active_items: &[MemoryItem],
    addition_verdicts: &HashMap<String, SemanticVerdict>,
    transition_verdicts: &HashMap<String, SemanticVerdict>,
    next_revision: i64,
    processed_through_sequence: i64,
) -> MemoryCommit {
    let now = Utc::now();
    let mut commit = MemoryCommit {
        processed_through_sequence,
        ..Default::default()
    };

    // Assign durable IDs first: a transition may name an addition as its
    // replacement, and that link has to point at the ID we actually persist.
    let mut assigned: HashMap<&str, MemoryId> = HashMap::new();
    for addition in &patch.additions {
        let verdict = addition_verdicts
            .get(&addition.candidate_id)
            .copied()
            .unwrap_or(SemanticVerdict::Supported);
        if verdict == SemanticVerdict::Unsupported {
            continue;
        }
        let id = MemoryId::new();
        assigned.insert(addition.candidate_id.as_str(), id.clone());
        commit.inserts.push(MemoryItem {
            id,
            conversation_id: conversation_id.to_string(),
            kind: addition.kind,
            state: MemoryState::Active,
            label: addition.label.clone(),
            evidence: addition.evidence.clone(),
            created_at_sequence: addition.created_at_sequence(),
            changed_at_sequence: addition.changed_at_sequence(),
            superseded_by: None,
            revision: next_revision,
            review: match verdict {
                SemanticVerdict::Supported => MemoryReview::Supported,
                _ => MemoryReview::Ambiguous,
            },
            related_item_ids: Vec::new(),
            created_at: now,
            updated_at: now,
        });
    }

    let by_id: HashMap<&str, &MemoryItem> = active_items
        .iter()
        .map(|item| (item.id.as_str(), item))
        .collect();

    for transition in &patch.transitions {
        // A replacement transition is one atomic claim. If semantic review
        // rejected the proposed replacement, retiring the old item would leave
        // the ledger with neither side active. Ignore the transition instead.
        if transition
            .replacement_candidate_id
            .as_deref()
            .is_some_and(|candidate| !assigned.contains_key(candidate))
        {
            continue;
        }
        let target_id = match &transition.target {
            ValidatedTransitionTarget::Existing(id) => id.clone(),
            ValidatedTransitionTarget::Candidate(candidate) => {
                let Some(id) = assigned.get(candidate.as_str()) else {
                    // The target addition was rejected by semantic review, so
                    // there is no item whose state can be changed.
                    continue;
                };
                id.clone()
            }
        };
        let existing = match &transition.target {
            ValidatedTransitionTarget::Existing(id) => {
                let Some(item) = by_id.get(id.as_str()) else {
                    continue;
                };
                (*item).clone()
            }
            ValidatedTransitionTarget::Candidate(_) => {
                let Some(item) = commit.inserts.iter().find(|item| item.id == target_id) else {
                    continue;
                };
                item.clone()
            }
        };
        let verdict = transition_verdicts
            .get(transition.target.as_str())
            .copied()
            .unwrap_or(SemanticVerdict::Ambiguous);

        if verdict == SemanticVerdict::Supported {
            let mut updated = existing.clone();
            updated.state = transition.state;
            updated.superseded_by = transition
                .replacement_candidate_id
                .as_deref()
                .and_then(|candidate| assigned.get(candidate).cloned());
            updated.changed_at_sequence = transition
                .evidence
                .iter()
                .map(|span| span.sequence)
                .max()
                .unwrap_or(existing.changed_at_sequence)
                .max(existing.changed_at_sequence);
            // The transition passage joins the item's own evidence, so the
            // history view can show what retired it without a second lookup.
            updated.evidence.extend(transition.evidence.iter().cloned());
            updated.evidence.truncate(MAX_EVIDENCE_PER_ITEM * 2);
            updated.revision = next_revision;
            updated.updated_at = now;
            match transition.target {
                ValidatedTransitionTarget::Existing(_) => commit.updates.push(updated),
                ValidatedTransitionTarget::Candidate(_) => {
                    if let Some(pending) =
                        commit.inserts.iter_mut().find(|item| item.id == target_id)
                    {
                        *pending = updated;
                    }
                }
            }
            continue;
        }

        if verdict == SemanticVerdict::Unsupported {
            continue;
        }

        // The evidence is real but its meaning is ambiguous: record the
        // conflict without changing the existing item.
        let mut evidence = existing.evidence.clone();
        evidence.extend(transition.evidence.iter().cloned());
        evidence.truncate(MAX_EVIDENCE_PER_ITEM * 2);
        let mut related = vec![existing.id.clone()];
        if let Some(replacement) = transition
            .replacement_candidate_id
            .as_deref()
            .and_then(|candidate| assigned.get(candidate).cloned())
        {
            related.push(replacement);
        }
        related.truncate(MAX_RELATED_ITEMS);

        let changed_at = evidence
            .iter()
            .map(|span| span.sequence)
            .max()
            .unwrap_or(existing.changed_at_sequence);
        commit.inserts.push(MemoryItem {
            id: MemoryId::new(),
            conversation_id: conversation_id.to_string(),
            kind: MemoryKind::UnresolvedChange,
            state: MemoryState::Active,
            label: format!("Unresolved change to: {}", existing.label),
            evidence,
            created_at_sequence: existing.created_at_sequence,
            changed_at_sequence: changed_at,
            superseded_by: None,
            revision: next_revision,
            review: MemoryReview::Ambiguous,
            related_item_ids: related,
            created_at: now,
            updated_at: now,
        });
        // The original item is untouched. A restriction stays in force until a
        // correction we actually understood retires it.
    }

    commit
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message(id: &str, sequence: i64, role: SourceRole, content: &str) -> SourceMessage {
        SourceMessage {
            id: id.to_string(),
            conversation_id: "c1".into(),
            sequence,
            role,
            content: content.to_string(),
            content_digest: compute_digest(content),
            status: "completed".into(),
        }
    }

    fn evidence(message_id: &str, quote: &str, purpose: &str) -> ProposedEvidence {
        ProposedEvidence {
            message_id: message_id.into(),
            quote: quote.into(),
            occurrence: None,
            purpose: purpose.into(),
        }
    }

    fn patch(add: Vec<ProposedItem>, transitions: Vec<ProposedTransition>) -> MemoryPatch {
        MemoryPatch {
            schema_version: MEMORY_SCHEMA_VERSION,
            add,
            transitions,
            processed_segment_ids: Vec::new(),
        }
    }

    fn constraint(candidate: &str, label: &str, evidence: Vec<ProposedEvidence>) -> ProposedItem {
        ProposedItem {
            candidate_id: candidate.into(),
            kind: "constraint".into(),
            label: label.into(),
            evidence,
        }
    }

    fn item(id: &str, kind: MemoryKind, sequence: i64, label: &str) -> MemoryItem {
        MemoryItem {
            id: MemoryId::from_string(id).unwrap(),
            conversation_id: "c1".into(),
            kind,
            state: MemoryState::Active,
            label: label.into(),
            evidence: vec![EvidenceSpan {
                message_id: "m1".into(),
                sequence,
                role: SourceRole::User,
                start_byte: 0,
                end_byte: 4,
                content_digest: compute_digest("word"),
                purpose: EvidencePurpose::Assertion,
            }],
            created_at_sequence: sequence,
            changed_at_sequence: sequence,
            superseded_by: None,
            revision: 1,
            review: MemoryReview::Supported,
            related_item_ids: Vec::new(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn a_quote_resolves_to_the_exact_source_bytes() {
        let content = "Do not deploy until I approve. Ship the café build → prod.";
        let messages = vec![message("m1", 1, SourceRole::User, content)];
        let sources = SourceIndex::new(&messages);

        let validated = validate_patch(
            &patch(
                vec![constraint(
                    "c1",
                    "Approval required",
                    vec![evidence(
                        "m1",
                        "Do not deploy until I approve.",
                        "assertion",
                    )],
                )],
                vec![],
            ),
            &sources,
            &[],
            &[],
        )
        .expect("valid patch");

        let span = &validated.additions[0].evidence[0];
        assert_eq!(
            span.resolve(content),
            Some("Do not deploy until I approve.")
        );

        // Multi-byte characters keep their offsets: the span is bytes, not chars.
        let unicode = validate_patch(
            &patch(
                vec![constraint(
                    "c2",
                    "Build target",
                    vec![evidence("m1", "café build → prod", "assertion")],
                )],
                vec![],
            ),
            &sources,
            &[],
            &[],
        )
        .expect("valid patch");
        assert_eq!(
            unicode.additions[0].evidence[0].resolve(content),
            Some("café build → prod")
        );
    }

    #[test]
    fn an_invented_quote_is_never_fuzzy_matched_into_a_real_source() {
        let messages = vec![message(
            "m1",
            1,
            SourceRole::User,
            "Do not deploy until I approve.",
        )];
        let sources = SourceIndex::new(&messages);

        // One word changed. Nothing about this is close enough to accept.
        let error = validate_patch(
            &patch(
                vec![constraint(
                    "c1",
                    "Invented",
                    vec![evidence(
                        "m1",
                        "Do not deploy until we approve.",
                        "assertion",
                    )],
                )],
                vec![],
            ),
            &sources,
            &[],
            &[],
        )
        .unwrap_err();
        assert_eq!(error.code(), "quote_not_found");

        // A foreign message id is not resolvable at all.
        let error = validate_patch(
            &patch(
                vec![constraint(
                    "c1",
                    "Foreign",
                    vec![evidence("m99", "Do not deploy", "assertion")],
                )],
                vec![],
            ),
            &sources,
            &[],
            &[],
        )
        .unwrap_err();
        assert_eq!(error.code(), "unknown_source");
    }

    #[test]
    fn an_assistant_approval_cannot_become_a_user_constraint() {
        let messages = vec![
            message("m1", 1, SourceRole::User, "Do not deploy."),
            message("m2", 2, SourceRole::Assistant, "Deployment approved."),
        ];
        let sources = SourceIndex::new(&messages);

        let error = validate_patch(
            &patch(
                vec![constraint(
                    "c1",
                    "Deployment allowed",
                    vec![evidence("m2", "Deployment approved.", "assertion")],
                )],
                vec![],
            ),
            &sources,
            &[],
            &[],
        )
        .unwrap_err();
        assert_eq!(error.code(), "non_user_authority");
    }

    #[test]
    fn an_assistant_quote_cannot_ride_along_as_assertion_authority() {
        let messages = vec![
            message("m1", 1, SourceRole::User, "Do not deploy."),
            message("m2", 2, SourceRole::Assistant, "Understood. Do not deploy."),
        ];
        let sources = SourceIndex::new(&messages);

        let error = validate_patch(
            &patch(
                vec![constraint(
                    "c1",
                    "Deployment prohibited",
                    vec![
                        evidence("m1", "Do not deploy.", "assertion"),
                        evidence("m2", "Understood. Do not deploy.", "assertion"),
                    ],
                )],
                vec![],
            ),
            &sources,
            &[],
            &[],
        )
        .unwrap_err();
        assert_eq!(error.code(), "non_user_authority");
    }

    #[test]
    fn an_assistant_antecedent_can_explain_a_short_user_assertion() {
        let messages = vec![
            message(
                "m1",
                1,
                SourceRole::Assistant,
                "Should the deployment target option B?",
            ),
            message("m2", 2, SourceRole::User, "Yes, option B."),
        ];
        let sources = SourceIndex::new(&messages);

        let validated = validate_patch(
            &patch(
                vec![constraint(
                    "c1",
                    "Use deployment option B",
                    vec![
                        evidence("m1", "Should the deployment target option B?", "antecedent"),
                        evidence("m2", "Yes, option B.", "assertion"),
                    ],
                )],
                vec![],
            ),
            &sources,
            &[],
            &[],
        )
        .expect("assistant antecedent plus user assertion is valid");

        assert_eq!(validated.additions[0].evidence.len(), 2);
        assert_eq!(
            validated.additions[0].evidence[0].purpose,
            EvidencePurpose::Antecedent
        );
    }

    #[test]
    fn transitions_require_user_transition_evidence() {
        let messages = vec![
            message("m1", 1, SourceRole::User, "Do not deploy."),
            message("m2", 5, SourceRole::User, "Deployment is allowed now."),
            message("m3", 6, SourceRole::Assistant, "Deployment is allowed now."),
        ];
        let sources = SourceIndex::new(&messages);
        let existing = vec![item("i1", MemoryKind::Constraint, 1, "Do not deploy")];

        let wrong_purpose = validate_patch(
            &patch(
                vec![],
                vec![ProposedTransition {
                    item_id: "i1".into(),
                    state: "resolved".into(),
                    replacement_candidate_id: None,
                    evidence: vec![evidence("m2", "Deployment is allowed now.", "assertion")],
                }],
            ),
            &sources,
            &existing,
            &[],
        )
        .unwrap_err();
        assert_eq!(wrong_purpose.code(), "invalid_evidence_purpose");

        let assistant_transition = validate_patch(
            &patch(
                vec![],
                vec![ProposedTransition {
                    item_id: "i1".into(),
                    state: "resolved".into(),
                    replacement_candidate_id: None,
                    evidence: vec![evidence("m3", "Deployment is allowed now.", "transition")],
                }],
            ),
            &sources,
            &existing,
            &[],
        )
        .unwrap_err();
        assert_eq!(assistant_transition.code(), "non_user_authority");
    }

    #[test]
    fn a_failed_assistant_output_is_not_evidence_but_a_pending_user_message_is() {
        let mut failed = message("m2", 2, SourceRole::Assistant, "Half an answer.");
        failed.status = "failed".into();
        let mut pending = message("m3", 3, SourceRole::User, "Do not send the email.");
        pending.status = "pending".into();
        assert!(!failed.is_eligible_source());
        // A generation failure must not erase what the user asked for.
        assert!(pending.is_eligible_source());
    }

    #[test]
    fn an_ambiguous_repeated_quote_must_say_which_occurrence() {
        let content = "use Rust. Actually, use Rust.";
        let messages = vec![message("m1", 1, SourceRole::User, content)];
        let sources = SourceIndex::new(&messages);

        let error = validate_patch(
            &patch(
                vec![constraint(
                    "c1",
                    "Language",
                    vec![evidence("m1", "use Rust", "assertion")],
                )],
                vec![],
            ),
            &sources,
            &[],
            &[],
        )
        .unwrap_err();
        assert_eq!(error.code(), "ambiguous_occurrence");

        // Naming the occurrence resolves it, and the offsets differ.
        let mut selected = evidence("m1", "use Rust", "assertion");
        selected.occurrence = Some(1);
        let validated = validate_patch(
            &patch(vec![constraint("c1", "Language", vec![selected])], vec![]),
            &sources,
            &[],
            &[],
        )
        .expect("valid patch");
        assert_eq!(validated.additions[0].evidence[0].start_byte, 20);
    }

    #[test]
    fn a_transition_needs_later_user_evidence_than_what_it_retires() {
        let messages = vec![
            message("m1", 1, SourceRole::User, "Budget is $120."),
            message(
                "m2",
                2,
                SourceRole::User,
                "Correction: $80 for this project.",
            ),
            message("m0", 0, SourceRole::User, "Earlier unrelated text."),
        ];
        let sources = SourceIndex::new(&messages);
        let existing = vec![item("i1", MemoryKind::Decision, 1, "Budget $120")];

        // Evidence that predates the item cannot retire it.
        let error = validate_patch(
            &patch(
                vec![],
                vec![ProposedTransition {
                    item_id: "i1".into(),
                    state: "resolved".into(),
                    replacement_candidate_id: None,
                    evidence: vec![evidence("m0", "Earlier unrelated text.", "transition")],
                }],
            ),
            &sources,
            &existing,
            &[],
        )
        .unwrap_err();
        assert_eq!(error.code(), "transition_not_later");

        // A later user correction is accepted.
        let validated = validate_patch(
            &patch(
                vec![constraint(
                    "c1",
                    "Budget $80",
                    vec![evidence(
                        "m2",
                        "Correction: $80 for this project.",
                        "assertion",
                    )],
                )],
                vec![ProposedTransition {
                    item_id: "i1".into(),
                    state: "superseded".into(),
                    replacement_candidate_id: Some("c1".into()),
                    evidence: vec![evidence("m2", "Correction: $80", "transition")],
                }],
            ),
            &sources,
            &existing,
            &[],
        )
        .expect("valid patch");
        assert_eq!(validated.transitions.len(), 1);
    }

    #[test]
    fn duplicate_additions_and_unlinked_supersessions_are_rejected() {
        let messages = vec![
            message("m1", 1, SourceRole::User, "Use a fixed seed."),
            message(
                "m2",
                2,
                SourceRole::User,
                "Restated: every output must use a fixed seed.",
            ),
        ];
        let sources = SourceIndex::new(&messages);
        let duplicate = ProposedItem {
            candidate_id: "second".into(),
            kind: "constraint".into(),
            label: "Every output must use a fixed seed".into(),
            evidence: vec![evidence(
                "m2",
                "Restated: every output must use a fixed seed.",
                "assertion",
            )],
        };
        let error = validate_patch(
            &patch(
                vec![
                    ProposedItem {
                        candidate_id: "first".into(),
                        ..duplicate.clone()
                    },
                    duplicate,
                ],
                vec![],
            ),
            &sources,
            &[],
            &[],
        )
        .unwrap_err();
        assert_eq!(error.code(), "duplicate_addition");

        let existing = vec![item("i1", MemoryKind::Constraint, 1, "Use a fixed seed")];
        let error = validate_patch(
            &patch(
                vec![],
                vec![ProposedTransition {
                    item_id: "i1".into(),
                    state: "superseded".into(),
                    replacement_candidate_id: None,
                    evidence: vec![evidence(
                        "m2",
                        "Restated: every output must use a fixed seed.",
                        "transition",
                    )],
                }],
            ),
            &sources,
            &existing,
            &[],
        )
        .unwrap_err();
        assert_eq!(error.code(), "supersession_without_replacement");
    }

    #[test]
    fn omitting_an_item_leaves_it_active_and_unchanged() {
        let existing = vec![
            item("i1", MemoryKind::Constraint, 1, "Do not deploy"),
            item("i2", MemoryKind::Goal, 2, "Ship the importer"),
        ];
        let commit = apply_patch(
            "c1",
            &ValidatedPatch::default(),
            &existing,
            &HashMap::new(),
            &HashMap::new(),
            2,
            10,
        );
        assert!(commit.inserts.is_empty());
        assert!(commit.updates.is_empty());
    }

    #[test]
    fn an_unsupported_retirement_is_ignored_instead_of_inventing_a_conflict() {
        let messages = vec![
            message("m1", 1, SourceRole::User, "Do not deploy."),
            message("m2", 5, SourceRole::User, "Looks good."),
        ];
        let sources = SourceIndex::new(&messages);
        let existing = vec![item("i1", MemoryKind::Constraint, 1, "Do not deploy")];

        let validated = validate_patch(
            &patch(
                vec![],
                vec![ProposedTransition {
                    item_id: "i1".into(),
                    state: "resolved".into(),
                    replacement_candidate_id: None,
                    evidence: vec![evidence("m2", "Looks good.", "transition")],
                }],
            ),
            &sources,
            &existing,
            &[],
        )
        .expect("spans are real even though the reading is not");

        let mut verdicts = HashMap::new();
        verdicts.insert("i1".to_string(), SemanticVerdict::Unsupported);
        let commit = apply_patch(
            "c1",
            &validated,
            &existing,
            &HashMap::new(),
            &verdicts,
            2,
            10,
        );

        // The restriction is untouched and an extractor hallucination does not
        // become a durable conflict.
        assert!(commit.updates.is_empty());
        assert!(commit.inserts.is_empty());
    }

    #[test]
    fn an_ambiguous_retirement_preserves_both_sides_as_a_conflict() {
        let messages = vec![
            message("m1", 1, SourceRole::User, "Do not deploy."),
            message("m2", 5, SourceRole::User, "Maybe staging is okay."),
        ];
        let sources = SourceIndex::new(&messages);
        let existing = vec![item("i1", MemoryKind::Constraint, 1, "Do not deploy")];
        let validated = validate_patch(
            &patch(
                vec![],
                vec![ProposedTransition {
                    item_id: "i1".into(),
                    state: "resolved".into(),
                    replacement_candidate_id: None,
                    evidence: vec![evidence("m2", "Maybe staging is okay.", "transition")],
                }],
            ),
            &sources,
            &existing,
            &[],
        )
        .expect("ambiguous source spans are still valid evidence");

        let mut verdicts = HashMap::new();
        verdicts.insert("i1".to_string(), SemanticVerdict::Ambiguous);
        let commit = apply_patch(
            "c1",
            &validated,
            &existing,
            &HashMap::new(),
            &verdicts,
            2,
            10,
        );

        assert!(commit.updates.is_empty());
        assert_eq!(commit.inserts.len(), 1);
        let conflict = &commit.inserts[0];
        assert_eq!(conflict.kind, MemoryKind::UnresolvedChange);
        assert_eq!(conflict.review, MemoryReview::Ambiguous);
        assert_eq!(conflict.related_item_ids[0].as_str(), "i1");
    }

    #[test]
    fn a_supported_retirement_links_the_replacement_it_actually_persisted() {
        let messages = vec![
            message("m1", 1, SourceRole::User, "Use Python."),
            message("m2", 4, SourceRole::User, "Actually use Rust."),
        ];
        let sources = SourceIndex::new(&messages);
        let existing = vec![item("i1", MemoryKind::Decision, 1, "Use Python")];

        let validated = validate_patch(
            &patch(
                vec![ProposedItem {
                    candidate_id: "c1".into(),
                    kind: "decision".into(),
                    label: "Use Rust".into(),
                    evidence: vec![evidence("m2", "Actually use Rust.", "assertion")],
                }],
                vec![ProposedTransition {
                    item_id: "i1".into(),
                    state: "superseded".into(),
                    replacement_candidate_id: Some("c1".into()),
                    evidence: vec![evidence("m2", "Actually use Rust.", "transition")],
                }],
            ),
            &sources,
            &existing,
            &[],
        )
        .expect("valid patch");

        let mut verdicts = HashMap::new();
        verdicts.insert("i1".to_string(), SemanticVerdict::Supported);
        let commit = apply_patch(
            "c1",
            &validated,
            &existing,
            &HashMap::new(),
            &verdicts,
            2,
            10,
        );

        let replacement = &commit.inserts[0];
        let retired = &commit.updates[0];
        assert_eq!(retired.state, MemoryState::Superseded);
        assert_eq!(retired.superseded_by.as_ref(), Some(&replacement.id));
    }

    #[test]
    fn a_rejected_replacement_cannot_retire_the_item_it_would_replace() {
        let messages = vec![
            message("m1", 1, SourceRole::User, "Never deploy on Friday."),
            message("m2", 4, SourceRole::User, "Deploy on Friday from now on."),
        ];
        let sources = SourceIndex::new(&messages);
        let existing = vec![item(
            "i1",
            MemoryKind::Constraint,
            1,
            "Never deploy on Friday",
        )];
        let validated = validate_patch(
            &patch(
                vec![ProposedItem {
                    candidate_id: "new-rule".into(),
                    kind: "constraint".into(),
                    label: "Deploy on Friday".into(),
                    evidence: vec![evidence("m2", "Deploy on Friday from now on.", "assertion")],
                }],
                vec![ProposedTransition {
                    item_id: "i1".into(),
                    state: "superseded".into(),
                    replacement_candidate_id: Some("new-rule".into()),
                    evidence: vec![evidence(
                        "m2",
                        "Deploy on Friday from now on.",
                        "transition",
                    )],
                }],
            ),
            &sources,
            &existing,
            &[],
        )
        .expect("valid structural patch");

        let additions = HashMap::from([("new-rule".to_string(), SemanticVerdict::Unsupported)]);
        let transitions = HashMap::from([("i1".to_string(), SemanticVerdict::Supported)]);
        let commit = apply_patch("c1", &validated, &existing, &additions, &transitions, 2, 4);

        assert!(commit.inserts.is_empty());
        assert!(commit.updates.is_empty());
    }

    #[test]
    fn a_same_response_candidate_can_be_superseded_by_a_later_candidate() {
        let messages = vec![
            message("m1", 1, SourceRole::User, "Budget is $120."),
            message("m2", 4, SourceRole::User, "Correction: budget is $80."),
        ];
        let sources = SourceIndex::new(&messages);
        let validated = validate_patch(
            &patch(
                vec![
                    ProposedItem {
                        candidate_id: "old-budget".into(),
                        kind: "decision".into(),
                        label: "Budget $120".into(),
                        evidence: vec![evidence("m1", "Budget is $120.", "assertion")],
                    },
                    ProposedItem {
                        candidate_id: "new-budget".into(),
                        kind: "decision".into(),
                        label: "Budget $80".into(),
                        evidence: vec![evidence("m2", "Correction: budget is $80.", "assertion")],
                    },
                ],
                vec![ProposedTransition {
                    item_id: "old-budget".into(),
                    state: "superseded".into(),
                    replacement_candidate_id: Some("new-budget".into()),
                    evidence: vec![evidence("m2", "Correction: budget is $80.", "transition")],
                }],
            ),
            &sources,
            &[],
            &[],
        )
        .expect("same-response candidate target is valid");

        assert!(matches!(
            &validated.transitions[0].target,
            ValidatedTransitionTarget::Candidate(id) if id == "old-budget"
        ));
        let addition_verdicts = HashMap::from([
            ("old-budget".to_string(), SemanticVerdict::Supported),
            ("new-budget".to_string(), SemanticVerdict::Supported),
        ]);
        let transition_verdicts =
            HashMap::from([("old-budget".to_string(), SemanticVerdict::Supported)]);
        let commit = apply_patch(
            "c1",
            &validated,
            &[],
            &addition_verdicts,
            &transition_verdicts,
            1,
            4,
        );

        assert!(commit.updates.is_empty());
        let old = commit
            .inserts
            .iter()
            .find(|item| item.label == "Budget $120")
            .expect("old value remains readable as history");
        let new = commit
            .inserts
            .iter()
            .find(|item| item.label == "Budget $80")
            .expect("replacement is stored");
        assert_eq!(old.state, MemoryState::Superseded);
        assert_eq!(old.superseded_by.as_ref(), Some(&new.id));
        assert_eq!(new.state, MemoryState::Active);
    }

    #[test]
    fn self_replacement_and_conflicting_transitions_are_rejected() {
        let messages = vec![
            message("m1", 1, SourceRole::User, "Do not deploy."),
            message("m2", 5, SourceRole::User, "Deploy staging only."),
        ];
        let sources = SourceIndex::new(&messages);
        let existing = vec![item("i1", MemoryKind::Constraint, 1, "Do not deploy")];

        let transition = |state: &str| ProposedTransition {
            item_id: "i1".into(),
            state: state.into(),
            replacement_candidate_id: None,
            evidence: vec![evidence("m2", "Deploy staging only.", "transition")],
        };
        let error = validate_patch(
            &patch(vec![], vec![transition("resolved"), transition("resolved")]),
            &sources,
            &existing,
            &[],
        )
        .unwrap_err();
        assert_eq!(error.code(), "conflicting_transitions");

        let error = validate_patch(
            &patch(
                vec![],
                vec![ProposedTransition {
                    item_id: "i1".into(),
                    state: "superseded".into(),
                    replacement_candidate_id: Some("i1".into()),
                    evidence: vec![evidence("m2", "Deploy staging only.", "transition")],
                }],
            ),
            &sources,
            &existing,
            &[],
        )
        .unwrap_err();
        // Named as its own replacement, but "i1" is not a candidate in this
        // response, so the unknown-replacement check fires first. Either way it
        // never commits.
        assert_eq!(error.code(), "unknown_replacement");
    }

    #[test]
    fn an_incomplete_coverage_report_is_detected() {
        let messages = vec![message("m1", 1, SourceRole::User, "Do not deploy.")];
        let sources = SourceIndex::new(&messages);
        let mut input = patch(vec![], vec![]);
        input.processed_segment_ids = vec!["m1:s0".into()];

        let error = validate_patch(
            &input,
            &sources,
            &[],
            &["m1:s0".to_string(), "m1:s1".to_string()],
        )
        .unwrap_err();
        assert_eq!(error.code(), "incomplete_coverage");
    }

    #[test]
    fn limits_are_enforced_before_anything_is_allocated() {
        let messages = vec![message("m1", 1, SourceRole::User, "Do not deploy.")];
        let sources = SourceIndex::new(&messages);

        let many: Vec<_> = (0..MAX_OPERATIONS_PER_RESPONSE + 1)
            .map(|i| {
                constraint(
                    &format!("c{i}"),
                    "x",
                    vec![evidence("m1", "Do not deploy.", "assertion")],
                )
            })
            .collect();
        assert_eq!(
            validate_patch(&patch(many, vec![]), &sources, &[], &[])
                .unwrap_err()
                .code(),
            "too_many_operations"
        );

        let long = "x".repeat(MAX_QUOTE_BYTES + 1);
        assert_eq!(
            validate_patch(
                &patch(
                    vec![constraint(
                        "c1",
                        "x",
                        vec![evidence("m1", &long, "assertion")]
                    )],
                    vec![]
                ),
                &sources,
                &[],
                &[]
            )
            .unwrap_err()
            .code(),
            "quote_too_long"
        );

        assert_eq!(
            parse_patch(&"x".repeat(MAX_PROPOSAL_BYTES + 1))
                .unwrap_err()
                .code(),
            "response_too_large"
        );
    }

    #[test]
    fn parsing_accepts_one_fence_and_nothing_looser() {
        let body = r#"{"schema_version":1,"add":[],"transitions":[],"processed_segment_ids":[]}"#;
        assert!(parse_patch(body).is_ok());
        assert!(parse_patch(&format!("```json\n{body}\n```")).is_ok());
        // Commentary around the JSON is not repaired into acceptance.
        assert!(parse_patch(&format!("Here you go:\n{body}")).is_err());
        // An unknown field means the model answered a different schema.
        assert!(parse_patch(r#"{"schema_version":1,"notes":"hi"}"#).is_err());
        // A newer patch version is refused rather than partially understood.
        let messages: Vec<SourceMessage> = Vec::new();
        let sources = SourceIndex::new(&messages);
        let mut future = patch(vec![], vec![]);
        future.schema_version = MEMORY_SCHEMA_VERSION + 1;
        assert_eq!(
            validate_patch(&future, &sources, &[], &[])
                .unwrap_err()
                .code(),
            "unsupported_schema_version"
        );
    }

    #[test]
    fn an_edited_source_stops_resolving_rather_than_re_pointing() {
        let original = "Do not deploy until I approve.";
        let messages = vec![message("m1", 1, SourceRole::User, original)];
        let sources = SourceIndex::new(&messages);
        let validated = validate_patch(
            &patch(
                vec![constraint(
                    "c1",
                    "Approval required",
                    vec![evidence("m1", "Do not deploy", "assertion")],
                )],
                vec![],
            ),
            &sources,
            &[],
            &[],
        )
        .unwrap();
        let span = &validated.additions[0].evidence[0];

        assert!(span.resolve(original).is_some());
        // Same offsets, different text: the digest check refuses to hand back
        // bytes the user never wrote.
        assert_eq!(span.resolve("You may deploy without approval."), None);
    }

    #[test]
    fn mandatory_kinds_are_the_ones_that_must_never_be_evicted() {
        for kind in [
            MemoryKind::Constraint,
            MemoryKind::Goal,
            MemoryKind::Decision,
            MemoryKind::UnresolvedChange,
        ] {
            assert!(kind.is_mandatory(), "{kind:?} must be mandatory");
        }
        for kind in [
            MemoryKind::UserFact,
            MemoryKind::Preference,
            MemoryKind::OpenQuestion,
        ] {
            assert!(!kind.is_mandatory(), "{kind:?} must be optional");
        }
    }

    #[test]
    fn an_unknown_stored_schema_is_not_read_as_empty_memory() {
        let mut state = ConversationMemoryState::empty("c1");
        assert!(state.is_schema_supported());
        state.schema_version = MEMORY_SCHEMA_VERSION + 1;
        assert!(!state.is_schema_supported());

        let snapshot = MemorySnapshot {
            state,
            transcript_revision: 1,
            active_items: vec![item("i1", MemoryKind::Constraint, 1, "Do not deploy")],
            summary: None,
            latest_sequence: 1,
        };
        assert!(!snapshot.is_usable());
    }
}

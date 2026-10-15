//! Untrusted patch proposals (§6.1) and the validated outcome they become.
use super::{
    EvidencePurpose, EvidenceSpan, MemoryId, MemoryKind, MemoryState, SourceRole,
    MAX_EVIDENCE_PER_ITEM, MAX_LABEL_CHARS, MAX_OPERATIONS_PER_RESPONSE, MAX_PROPOSAL_BYTES,
    MAX_QUOTE_BYTES,
};
use serde::{Deserialize, Serialize};

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
    #[error("established_fact '{0}' has no assertion; a sourced statement is what makes it one")]
    NoAssertion(String),
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
            Self::NoAssertion(_) => "no_assertion",
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

    /// Any assertion at all, regardless of who said it. That is the bar for
    /// [`MemoryKind::EstablishedFact`], whose authority is the sourced
    /// passage itself rather than the person quoting it.
    pub fn has_assertion(&self) -> bool {
        self.evidence
            .iter()
            .any(|span| span.purpose == EvidencePurpose::Assertion)
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

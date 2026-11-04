//! Lesson claim review: every factual claim a lesson makes, what the machine
//! and the user did about it, and the rules that decide what happens next.
//!
//! A claim gets at most two machine repairs in its life, whatever evidence
//! later checks bring; after that it waits for the user, who resolves it with
//! an action that is recorded, undoable and never re-litigated by the machine
//! unless the sentence or a cited source changes. A lesson with any unresolved
//! claim is not ready. A claim the acceptance policy has decided
//! ([`ClaimEvent::Stepped`]) is read from its latest decision instead, and
//! the two-repair budget does not apply to it (acceptance design §2.9). Pure
//! types and rules, no I/O: the learning feature stores the ledger through
//! `application::ports::claim_review::ClaimReviewStore`.
//!
//! Storage forms: [`ClaimState`] and [`ClaimEvent`] are internally tagged
//! (`state`, `kind`; the event tag is also the events table's `kind` column);
//! the values nested in them use serde's default external tagging.
//!
//! Design: docs/design/2026-10-16-lesson-claim-review.md §3, §5, §6, §14, §15.

mod ledger;
mod replay;
mod rules;
#[cfg(test)]
mod tests;
mod trust;

pub use ledger::{
    CheckBasis, ClaimActionKind, ClaimLedgerEntry, EventProvenance, LedgerError, RecordedEvent,
};
pub use rules::{
    end_state, identities, new_information, relitigate, reopen_cause, ClaimCounts, ExtractedClaim,
    Identities, NewInformation, Observation, SourceChange,
};
pub use trust::{
    trust_outcomes, CalibrationOutcome, GateVerdict, SupportedBy, TrustAssessment, TrustCause,
    TrustVerdict,
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::domain::learning::acceptance::{self, Goal, Step, Then};
use crate::domain::provenance::{Bar, CountedSupport};

/// Machine repairs a claim gets in its life, whatever evidence later checks
/// bring: one correction, then one escalated rewrite that may remove the
/// sentence (§3; claim classes design §3.2).
pub const MACHINE_ATTEMPTS: u8 = 2;

/// Lower-case hex SHA-256 of `text`: quote bindings and passage digests.
pub fn text_sha256(text: &str) -> String {
    hex::encode(Sha256::digest(text.as_bytes()))
}

/// A claim's identity: a uuid assigned at first extraction and kept across
/// re-extractions by [`identities`].
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ClaimId(pub String);

impl ClaimId {
    pub fn generate() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }
}

/// Which kind of lesson unit a claim was extracted from. `AnswerKey` is the
/// explanation or correct index of a question.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnitKind {
    Block,
    Question,
    AnswerKey,
}

impl UnitKind {
    /// The `unit_kind` column's value.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Block => "block",
            Self::Question => "question",
            Self::AnswerKey => "answer_key",
        }
    }

    pub fn from_column(value: &str) -> Option<Self> {
        [Self::Block, Self::Question, Self::AnswerKey]
            .into_iter()
            .find(|kind| kind.as_str() == value)
    }
}

/// Where a claim is in the lesson.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClaimLocation {
    /// The report's unit index: blocks first, then questions.
    pub unit_index: usize,
    pub unit_kind: UnitKind,
    /// The unit field, e.g. `body`, `title`, `rubric/2/description`, `prompt`,
    /// `options/1`, `explanation`.
    pub field: String,
    /// The exact lesson text the claim was extracted from.
    pub quote: String,
    /// Byte span of `quote` in the field. A claim located to its sentence
    /// (`lattice_core::shared::text::claim_sentence`) keeps the span the
    /// locator found, so a short sentence that recurs elsewhere in the field
    /// still has its one place; otherwise the span of the quote's only
    /// occurrence, and `None` when it occurs more than once (or not at all),
    /// so no single place can be named.
    pub span: Option<(usize, usize)>,
}

impl ClaimLocation {
    /// A location whose span is found in the field's current text.
    pub fn locate(
        unit_index: usize,
        unit_kind: UnitKind,
        field: impl Into<String>,
        quote: impl Into<String>,
        field_text: &str,
    ) -> Self {
        let quote = quote.into();
        Self {
            span: span_of(field_text, &quote),
            unit_index,
            unit_kind,
            field: field.into(),
            quote,
        }
    }
}

/// The byte span of `quote` in `text` when it occurs exactly once. Overlapping
/// occurrences count, so `"aa"` in `"aaa"` has no span.
pub fn span_of(text: &str, quote: &str) -> Option<(usize, usize)> {
    let start = text.find(quote).filter(|_| !quote.is_empty())?;
    let next = start + quote.chars().next().map_or(0, char::len_utf8);
    match text.get(next..).and_then(|rest| rest.find(quote)) {
        Some(_) => None,
        None => Some((start, start + quote.len())),
    }
}

/// What a statement needs to be checked for (claim classes design §3.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Class {
    /// Attributes something to a source or states a specific: checked
    /// against its cited passage.
    Sourced,
    /// The lesson's own explanation: checked for consistency with its
    /// section's passages and for falsity.
    Reasoning,
    /// Standard knowledge with no specific: checked closed-book.
    Common,
    /// Scenario, rubric, option or remark about the sources: not a claim.
    Internal,
}

impl Class {
    /// How much checking the class gets: the stricter, the more.
    pub fn strictness(self) -> u8 {
        match self {
            Self::Sourced => 3,
            Self::Reasoning => 2,
            Self::Common => 1,
            Self::Internal => 0,
        }
    }

    /// Whether this class, decided `by` an authority, takes the place of the
    /// one a statement holds: a stronger authority's always does, the same
    /// authority's only when it is stricter, so nothing makes a statement's
    /// check lighter but an authority above the one that gave its class. What
    /// a check found always does: the policy moves a statement by evidence in
    /// either direction, and the latest move holds.
    pub fn prevails(self, by: ClassifiedBy, held: Option<(Class, ClassifiedBy)>) -> bool {
        held.is_none_or(|(class, held_by)| {
            by == ClassifiedBy::Evidence
                || by > held_by
                || (by == held_by && self.strictness() > class.strictness())
        })
    }
}

/// Who decided a statement's class, weakest first. The model only splits
/// what the rule layer leaves, and a sentence the writer cited is Sourced
/// whatever anyone says later, so a class is never replaced by a weaker
/// authority's (claim classes design §3.1). What a check found outranks them
/// all: a statement that changes class because of it is the policy's to move
/// (acceptance design §2.9), either way, and its latest move holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClassifiedBy {
    Model,
    Rule,
    Writer,
    Evidence,
}

/// Why a claim is not verified, in honest words, one per case.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerdictKind {
    /// Evidence was compared and none supports the claim.
    Unsupported,
    /// A passage says otherwise.
    Contradicted,
    /// Retrieval found no passage for the claim's terms in any saved source.
    NoSource,
    /// The checker could not answer: the window, an unusable answer, or no
    /// runtime for a code example. A local check with this result teaches
    /// nothing about the claim (claim classes design §3.2): see
    /// [`CheckResult::is_unjudged`].
    CouldNotJudge,
    /// The Verifier audit disagrees with the local checker's "supported"
    /// (§14.5). The local side is always "verified": a human verdict the
    /// Verifier disputes is reopened instead ([`ReopenCause::VerifierDispute`]).
    Disputed { verifier: Box<VerdictKind> },
    /// The checker found support, but the supports fail the bar for the
    /// claim's kind (source provenance §6, decision 1): `bar` is the rule,
    /// `have` each support as the gate counted it.
    InsufficientStanding { bar: Bar, have: Vec<CountedSupport> },
}

impl VerdictKind {
    /// The `verdict_kind` column's value.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Unsupported => "unsupported",
            Self::Contradicted => "contradicted",
            Self::NoSource => "no_source",
            Self::CouldNotJudge => "could_not_judge",
            Self::Disputed { .. } => "disputed",
            Self::InsufficientStanding { .. } => "insufficient_standing",
        }
    }

    /// A failure the machine answers, by a rewrite and then by taking the
    /// claim's sentence out (claim classes design §3.2): no passage supports
    /// the claim, one contradicts it, or none was found. A checker that gave
    /// no answer is not a failure of the claim, and a claim whose support is
    /// below the bar is never rewritten or removed for lack of standing
    /// (claim sources design §10.4): research may look for a stronger
    /// source, then it waits for the user.
    pub fn is_repairable(&self) -> bool {
        matches!(
            self,
            Self::Unsupported | Self::Contradicted | Self::NoSource
        )
    }

    /// A passage, the local checker's or the Verifier's, says otherwise.
    /// Confirming such a claim needs a note (D3).
    pub fn is_contradiction(&self) -> bool {
        match self {
            Self::Contradicted => true,
            Self::Disputed { verifier } => verifier.is_contradiction(),
            _ => false,
        }
    }
}

/// One passage a check compared, pinned to an immutable source version.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct EvidenceSeen {
    pub source_version_id: String,
    pub start_byte: usize,
    pub end_byte: usize,
    pub text_sha256: String,
}

/// What a check depended on: the policy, the checker model and the passages
/// compared, in any order. Equal fingerprints mean nothing new to judge.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct EvidenceFingerprint(pub String);

impl EvidenceFingerprint {
    pub fn of(policy: &str, checker_model: &str, evidence: &[EvidenceSeen]) -> Self {
        let mut seen: Vec<&EvidenceSeen> = evidence.iter().collect();
        seen.sort();
        seen.dedup();
        let mut digest = Sha256::new();
        for part in [policy, checker_model] {
            digest.update(part.len().to_le_bytes());
            digest.update(part.as_bytes());
        }
        for passage in seen {
            digest.update(passage.source_version_id.len().to_le_bytes());
            digest.update(passage.source_version_id.as_bytes());
            digest.update(passage.start_byte.to_le_bytes());
            digest.update(passage.end_byte.to_le_bytes());
            digest.update(passage.text_sha256.as_bytes());
        }
        Self(hex::encode(digest.finalize()))
    }
}

/// Where a claim stands. Replayed from its events ([`ClaimLedgerEntry`]),
/// never written directly.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum ClaimState {
    /// Extracted and not yet checked; the machine owns it.
    Unchecked,
    /// The machine may still act.
    Open {
        kind: VerdictKind,
        reason: String,
        machine_attempts_left: u8,
    },
    /// Waiting for the user; the machine acts again only on new information.
    Queued { kind: VerdictKind, reason: String },
    /// Machine-verified against a saved source (or an execution).
    ResolvedByEvidence {
        source_version_id: String,
        supporting_quote: String,
    },
    /// A Reasoning statement its section's passages do not contradict and
    /// that asserts no specific they lack (claim classes design §3.11). Like
    /// a verification it is the machine's, and the lesson may publish with it.
    Consistent { at: i64 },
    /// The user's verdict.
    Confirmed {
        at: i64,
        note: Option<String>,
        bound: HumanBinding,
    },
    /// The sentence or claim was deleted.
    Removed { at: i64, by: Actor },
    /// The text changed; the successor claim owns the state. `None` is the
    /// user's edit that left no successor, "removed by the edit": the
    /// machine's change with none is [`ClaimState::Removed`]. `by`: who
    /// changed the text, the user's edit or the machine's repair (claim
    /// sources design §11).
    Edited {
        at: i64,
        successor: Option<ClaimId>,
        #[serde(default)]
        by: Actor,
    },
    /// A Common statement the knowledge judge passed: standard knowledge,
    /// checked against no source (claim classes design §3.11).
    GeneralKnowledge { at: i64 },
    /// A closed claim reopened; it waits for the user like `Queued`.
    Reopened {
        cause: ReopenCause,
        previous: Box<ClaimState>,
    },
}

impl ClaimState {
    /// Unchecked or Open: the machine has work left on this claim.
    pub fn is_machine_owned(&self) -> bool {
        matches!(self, Self::Unchecked | Self::Open { .. })
    }

    /// Queued or Reopened: only the user (or new information) moves it.
    pub fn awaits_user(&self) -> bool {
        matches!(self, Self::Queued { .. } | Self::Reopened { .. })
    }

    /// Closed in one of the ways a lesson may publish with (§2.1): verified,
    /// found consistent, passed as common knowledge, confirmed, removed or
    /// edited (the successor answers for it).
    pub fn is_resolved(&self) -> bool {
        !self.is_machine_owned() && !self.awaits_user()
    }

    /// A verdict no machine check changes: the user's, or a sentence that is
    /// gone. Only [`ClaimEvent::Reopened`], a Verifier dispute or the user
    /// moves it.
    pub fn is_settled(&self) -> bool {
        matches!(
            self,
            Self::Confirmed { .. } | Self::Removed { .. } | Self::Edited { .. }
        )
    }

    /// The failure an Open or Queued claim carries.
    pub fn verdict(&self) -> Option<&VerdictKind> {
        match self {
            Self::Open { kind, .. } | Self::Queued { kind, .. } => Some(kind),
            _ => None,
        }
    }
}

/// Who did something. Defaults to the machine: an event recorded before
/// it said who acted was the machine's.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Actor {
    #[default]
    Machine,
    User,
}

impl Actor {
    /// The events table's `actor` column.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Machine => "machine",
            Self::User => "user",
        }
    }

    pub fn from_column(value: &str) -> Option<Self> {
        [Self::Machine, Self::User]
            .into_iter()
            .find(|actor| actor.as_str() == value)
    }
}

/// What a user's confirmation is bound to: the quote as it read, and the
/// source versions the user picked (possibly none).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HumanBinding {
    pub quote_sha256: String,
    pub source_version_ids: Vec<String>,
}

impl HumanBinding {
    pub fn new(quote: &str, source_version_ids: Vec<String>) -> Self {
        Self {
            quote_sha256: text_sha256(quote),
            source_version_ids,
        }
    }
}

/// Why a closed claim was reopened.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReopenCause {
    SentenceChanged,
    /// The source version that resolved or was bound to the claim.
    SourceDeleted(String),
    SourceReplaced {
        from: String,
        to: String,
    },
    Unconfirmed,
    VerifierDispute,
    /// An override or a calibrated bar changed, and the gate's re-decision
    /// no longer verifies the claim (source provenance §18.7).
    TrustChanged(TrustCause),
}

/// Which checker judged: the Learning model, or the Verifier audit (§14).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Checker {
    Local { model: String },
    Verifier { model: String },
}

/// One check of one claim.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Check {
    pub checker: Checker,
    pub result: CheckResult,
    pub reason: String,
    pub fingerprint: EvidenceFingerprint,
    pub evidence: Vec<EvidenceSeen>,
    /// The checker's configured context window when it judged; a different
    /// window is new information (§3).
    pub window_tokens: usize,
}

/// A check's verdict. `Supported` names the passage that entails the claim.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckResult {
    Supported {
        source_version_id: String,
        supporting_quote: String,
    },
    /// The claim stands, but not on what its section cites: none of the
    /// `cited` source versions supports it, and a passage of another
    /// course source does (two-pass claim pages, course evidence index
    /// §5.1). The claim is verified by the found passage; the lesson's
    /// citation is repaired to it.
    SupportedElsewhere {
        cited: Vec<String>,
        source_version_id: String,
        supporting_quote: String,
    },
    /// A Reasoning statement the passages compared do not contradict, and
    /// that asserts no specific they lack: it rests on no one passage (claim
    /// classes design §3.1).
    Consistent,
    /// A Common statement every sample of the knowledge judge called true and
    /// common: it rests on no passage at all (claim classes design §3.1).
    KnownCommon,
    /// Asked alone, the model's samples did not call the statement false: a
    /// passed falsity veto, which is not the unanimity `KnownCommon` records
    /// (acceptance design §4).
    NotFalse,
    Failed {
        kind: VerdictKind,
    },
}

impl CheckResult {
    /// The checker gave no answer: its reply could not be read, or the claim
    /// and its passage did not fit its window. Nothing is learned about the
    /// claim, which keeps its text and its attempts (claim classes design
    /// §3.2).
    pub fn is_unjudged(&self) -> bool {
        matches!(
            self,
            Self::Failed {
                kind: VerdictKind::CouldNotJudge
            }
        )
    }

    /// The passage that entails the claim, as its source version and
    /// quotation: wherever it was found, the claim stands on it. `None` for a
    /// failed check, and for a pass that rests on no passage.
    pub fn support(&self) -> Option<(&str, &str)> {
        match self {
            Self::Supported {
                source_version_id,
                supporting_quote,
            }
            | Self::SupportedElsewhere {
                source_version_id,
                supporting_quote,
                ..
            } => Some((source_version_id, supporting_quote)),
            Self::Consistent | Self::KnownCommon | Self::NotFalse | Self::Failed { .. } => None,
        }
    }
}

/// A user's resolution of one claim (§5.1). Attaching evidence and undoing
/// are their own events.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UserAction {
    Confirm { bound: HumanBinding },
    Unconfirm,
    Edit { before: String, after: String },
    Remove,
    ResearchWithHint { hint: String },
}

/// What came of one machine repair of a claim.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RepairOutcome {
    /// The sentence was rewritten; the next check decides.
    Rewritten,
    /// The sentence was deleted.
    Removed,
    /// The repair's output was not used; the attempt still counts.
    Rejected { reason: String },
    /// The user edited the unit meanwhile; the result was discarded and the
    /// attempt is not charged (§9).
    SupersededByUser,
}

/// What the user attached as evidence (§5.4).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Provenance {
    SavedPassage { start_byte: usize, end_byte: usize },
    Link { url: String },
    Document { document_id: String },
    PastedText,
}

/// The targeted check of a claim against a source the user attached.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttachOutcome {
    Checked(Check),
    /// No passage of the source mentions the claim's terms.
    NoPassage,
}

/// Who added a learning source: the `added_by` column (§5.4, §14.5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceAddedBy {
    User,
    Research,
    Outline,
    Import,
    Verifier,
}

impl SourceAddedBy {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Research => "research",
            Self::Outline => "outline",
            Self::Import => "import",
            Self::Verifier => "verifier",
        }
    }

    pub fn from_column(value: &str) -> Option<Self> {
        [
            Self::User,
            Self::Research,
            Self::Outline,
            Self::Import,
            Self::Verifier,
        ]
        .into_iter()
        .find(|added_by| added_by.as_str() == value)
    }
}

/// Why a lesson's preparation stopped (§8.4): the review row's
/// `stop_reason_json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StopReason {
    pub code: String,
    pub message: String,
    pub fix: String,
    pub at: i64,
    pub request: Option<RequestFacts>,
}

/// The numbers that explain a refused or rejected model request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequestFacts {
    pub prompt_tokens_exact: Option<usize>,
    pub prompt_tokens_estimated: Option<usize>,
    pub window_tokens: usize,
    pub output_reserved: usize,
    pub server_kind: String,
    pub server_message_sanitized: Option<String>,
    pub attempts: u32,
    /// The preparation phase and stage text when it stopped.
    pub step: String,
}

/// One entry of a claim's append-only audit trail.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ClaimEvent {
    Extracted {
        location: ClaimLocation,
        statement: String,
    },
    Checked(Check),
    Researched {
        queries: Vec<String>,
        /// Source version ids the research saved.
        sources_added: Vec<String>,
        passages_found: usize,
        /// What the round cost per claim it researched, in milliseconds:
        /// its time from start to result over its claims. Pass B weighs it
        /// as the cost of researching one claim on the web (course evidence
        /// index D5). `None` when no round ran, and on events written before
        /// rounds were timed.
        #[serde(default)]
        round_ms_per_claim: Option<u64>,
    },
    RepairAttempted {
        attempt: u8,
        escalated: bool,
        before: String,
        after: Option<String>,
        outcome: RepairOutcome,
        /// What the repair was asked to achieve (acceptance design §2.9);
        /// `None` on repairs recorded before the policy named a goal.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        goal: Option<Goal>,
    },
    Executed {
        observation_id: String,
        passed: bool,
        evidence: String,
    },
    /// The machine moved the claim's section citation to the passage that
    /// supports the claim, found elsewhere in the course (course evidence
    /// index §5.1). The claim's words are unchanged, so no repair attempt is
    /// charged; the next check reads the new citation as the section's own.
    CitationRepaired {
        /// The source versions the section cited, none of which supported it.
        cited: Vec<String>,
        /// The source version it cites now, and the quotation it carries.
        source_version_id: String,
        quote: String,
        /// The lesson did not number the source before: it was added to the
        /// lesson's sources to be cited.
        added_source: bool,
    },
    /// The machine dropped the sentence's citation: the claim's parts are all
    /// common and the passages it cited state none (acceptance design §2.1).
    /// The claim's words are unchanged, so no repair attempt is charged; the
    /// next check reads it without the citation.
    CitationDropped {
        /// The source versions the sentence cited.
        cited: Vec<String>,
    },
    UserAction {
        action: UserAction,
        note: Option<String>,
    },
    EvidenceAttached {
        source_version_id: String,
        provenance: Provenance,
        result: AttachOutcome,
    },
    /// Re-extraction found the claim's successor (`None`: no successor, the
    /// sentence went with the change). `by`: whose change, the user's edit
    /// or the machine's repair; the machine's with no successor removes the
    /// claim, as it does a claim on a sentence cut for another's failure.
    Superseded {
        successor: Option<ClaimId>,
        #[serde(default)]
        by: Actor,
    },
    /// The claim was born of a repair of `parent` (claim sources design
    /// §9.1): a sentence the rewrite added beside the parent's successor. It
    /// starts with the parent's remaining attempts, so a split sentence adds
    /// no budget.
    BornOfRepair {
        parent: ClaimId,
        attempts_left: u8,
    },
    Reopened {
        cause: ReopenCause,
    },
    /// The statement's class and who decided it (claim classes design §3.1).
    Classified {
        class: Class,
        by: ClassifiedBy,
    },
    Undone {
        event_ordinal: u32,
    },
    Stopped {
        reason: StopReason,
    },
    /// What the provenance gate weighed for the claim. With no `cause` it
    /// follows the local check the gate decided; with one it is the gate's
    /// re-decision after a trust change, made without a model call.
    /// `reason` is the gate's explanation, the claim's reason when it fails.
    StandingAssessed {
        trust: Box<TrustAssessment>,
        reason: String,
        cause: Option<TrustCause>,
    },
    /// What the acceptance policy made of one machine step, recorded beside
    /// the step's own event (acceptance design §2.9). A machine-owned claim's
    /// state is read from its latest decision.
    Stepped {
        /// The policy's version, [`acceptance::STANDARD_POLICY`] for the
        /// standard one. A claim is never continued under another.
        policy: String,
        step: Step,
        outcome: acceptance::Outcome,
        /// Where the policy sent the statement.
        then: Then,
        /// The step's own reason, which the claim shows while it is open.
        reason: String,
        /// A rewrite is reachable from `then`: the Open state's attempt left.
        rewrite_ahead: bool,
    },
}

impl ClaimEvent {
    /// The events table's `kind` column: the serde tag.
    pub fn kind_name(&self) -> &'static str {
        match self {
            Self::Extracted { .. } => "extracted",
            Self::Checked(_) => "checked",
            Self::Researched { .. } => "researched",
            Self::RepairAttempted { .. } => "repair_attempted",
            Self::Executed { .. } => "executed",
            Self::CitationRepaired { .. } => "citation_repaired",
            Self::CitationDropped { .. } => "citation_dropped",
            Self::UserAction { .. } => "user_action",
            Self::EvidenceAttached { .. } => "evidence_attached",
            Self::Superseded { .. } => "superseded",
            Self::BornOfRepair { .. } => "born_of_repair",
            Self::Reopened { .. } => "reopened",
            Self::Classified { .. } => "classified",
            Self::Undone { .. } => "undone",
            Self::Stopped { .. } => "stopped",
            Self::StandingAssessed { .. } => "standing_assessed",
            Self::Stepped { .. } => "stepped",
        }
    }
}

/// A lesson's review state: the `learning_lesson_reviews.state` column.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LessonReviewState {
    Verifying,
    NeedsReview,
    Stopped,
    Ready,
}

impl LessonReviewState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Verifying => "verifying",
            Self::NeedsReview => "needs_review",
            Self::Stopped => "stopped",
            Self::Ready => "ready",
        }
    }

    pub fn from_column(value: &str) -> Option<Self> {
        [
            Self::Verifying,
            Self::NeedsReview,
            Self::Stopped,
            Self::Ready,
        ]
        .into_iter()
        .find(|state| state.as_str() == value)
    }
}

/// A lesson's review row, as written by the job.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LessonReview {
    pub program_id: String,
    pub job_id: String,
    pub state: LessonReviewState,
    pub draft_sha256: String,
    pub checker_model: String,
    pub stop_reason: Option<StopReason>,
}

/// The lesson review's optimistic-concurrency revision; every append and
/// review write advances it (§9).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Revision(pub i64);

/// Everything stored about a lesson's claims.
#[derive(Debug, Clone, PartialEq)]
pub struct Ledger {
    pub lesson_id: String,
    /// `None` until the first verification pass writes the review row.
    pub review: Option<LessonReview>,
    pub revision: Revision,
    pub claims: Vec<ClaimLedgerEntry>,
}

//! The claim review as the frontend sees it (§7.3): camelCase structs and
//! internally tagged unions. Pure mirrors of the domain convert with `From`;
//! the review and its items are assembled by the claim review service, which
//! knows unit titles and source titles.

mod history;

pub use history::{
    AttachOutcomeDto, CheckDto, CheckResultDto, CheckerDto, CheckerRoleDto, ClaimEventDto,
    ClaimEventPayloadDto, EvidenceSeenDto, ProvenanceDto, UserActionDto,
};

use lattice_core::domain::learning::claim_review::{
    Actor, ClaimActionKind, ClaimCounts, ClaimState, LessonReviewState, ReopenCause, RepairOutcome,
    RequestFacts, SourceAddedBy, StopReason, UnitKind, VerdictKind,
};
use serde::{Deserialize, Serialize};

use crate::features::learning::lesson_evidence::LearningEvidencePassageDto;
use crate::features::learning::runtime::lab_runtime::LearningLabExecutionResult;

/// Everything the review panel shows for one lesson.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LessonClaimReviewDto {
    pub lesson_id: String,
    pub program_id: String,
    pub state: LessonReviewStateDto,
    /// Send back as `expected_revision` with an action.
    pub revision: i64,
    pub counts: ClaimCountsDto,
    /// Verification passes run on a new candidate text (§6.3).
    pub pass: u32,
    pub stop_reason: Option<StopReasonDto>,
    pub claims: Vec<ClaimReviewItemDto>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum LessonReviewStateDto {
    Verifying,
    NeedsReview,
    Stopped,
    Ready,
}

impl From<LessonReviewState> for LessonReviewStateDto {
    fn from(state: LessonReviewState) -> Self {
        match state {
            LessonReviewState::Verifying => Self::Verifying,
            LessonReviewState::NeedsReview => Self::NeedsReview,
            LessonReviewState::Stopped => Self::Stopped,
            LessonReviewState::Ready => Self::Ready,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ClaimCountsDto {
    pub verified: usize,
    pub confirmed: usize,
    pub general_knowledge: usize,
    pub queued: usize,
    pub open: usize,
    pub removed: usize,
}

impl From<ClaimCounts> for ClaimCountsDto {
    fn from(counts: ClaimCounts) -> Self {
        Self {
            verified: counts.verified,
            confirmed: counts.confirmed,
            general_knowledge: counts.general_knowledge,
            queued: counts.queued,
            open: counts.open,
            removed: counts.removed,
        }
    }
}

/// One claim card.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ClaimReviewItemDto {
    pub claim_id: String,
    pub location: ClaimLocationDto,
    pub statement: String,
    pub state: ClaimStateDto,
    /// The latest failing verdict, kept after the claim closes.
    pub verdict_kind: Option<VerdictKindDto>,
    pub reason: Option<String>,
    /// What was compared.
    pub evidence: Vec<ClaimEvidencePassageDto>,
    pub supporting_quote: Option<String>,
    pub research: Vec<ResearchAttemptDto>,
    pub repairs: Vec<RepairAttemptDto>,
    pub executions: Vec<ExecutionDto>,
    pub human: Option<HumanVerdictDto>,
    pub available_actions: Vec<ClaimActionKindDto>,
    pub history: Vec<ClaimEventDto>,
    /// The local checker's latest verdict: the card's "Local checker said"
    /// column (§14.5). Null before the claim's first check.
    pub local_check: Option<CheckerVerdictDto>,
    /// The Verifier audit's latest verdict: the "Verifier said" column. Null
    /// until the audit has checked the claim.
    pub verifier_check: Option<CheckerVerdictDto>,
}

/// One checker's latest verdict on a claim.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CheckerVerdictDto {
    pub checker: CheckerDto,
    /// `supported` with the passage and quote it rests on, or `failed`
    /// with its kind.
    pub result: CheckResultDto,
    pub reason: String,
    /// The source a `supported` result quotes: for the Verifier, possibly a
    /// page it fetched during the audit (`addedBy: verifier`).
    pub quoted_source: Option<SourceRefDto>,
    /// The passages the checker compared.
    pub passages: Vec<ClaimEvidencePassageDto>,
    pub at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ClaimLocationDto {
    pub unit_index: usize,
    pub unit_kind: UnitKindDto,
    pub unit_title: String,
    pub field: String,
    pub quote: String,
    /// Byte span of `quote` in the field; null when it occurs more than once.
    pub span: Option<(usize, usize)>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum UnitKindDto {
    Block,
    Question,
    AnswerKey,
}

impl From<UnitKind> for UnitKindDto {
    fn from(kind: UnitKind) -> Self {
        match kind {
            UnitKind::Block => Self::Block,
            UnitKind::Question => Self::Question,
            UnitKind::AnswerKey => Self::AnswerKey,
        }
    }
}

/// Where a claim stands. The verdict, reason, supporting passage and the
/// user's note are the item's own fields, so they are not repeated here.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum ClaimStateDto {
    Unchecked,
    Open {
        #[serde(rename = "machineAttemptsLeft")]
        machine_attempts_left: u8,
    },
    Queued,
    ResolvedByEvidence,
    Confirmed {
        at: i64,
    },
    Removed {
        at: i64,
        by: ActorDto,
    },
    Edited {
        at: i64,
        successor: Option<String>,
    },
    GeneralKnowledge {
        at: i64,
        by: ActorDto,
    },
    Reopened {
        cause: ReopenCauseDto,
        previous: Box<ClaimStateDto>,
    },
}

impl From<&ClaimState> for ClaimStateDto {
    fn from(state: &ClaimState) -> Self {
        match state {
            ClaimState::Unchecked => Self::Unchecked,
            ClaimState::Open {
                machine_attempts_left,
                ..
            } => Self::Open {
                machine_attempts_left: *machine_attempts_left,
            },
            ClaimState::Queued { .. } => Self::Queued,
            ClaimState::ResolvedByEvidence { .. } => Self::ResolvedByEvidence,
            ClaimState::Confirmed { at, .. } => Self::Confirmed { at: *at },
            ClaimState::Removed { at, by } => Self::Removed {
                at: *at,
                by: (*by).into(),
            },
            ClaimState::Edited { at, successor } => Self::Edited {
                at: *at,
                successor: successor.as_ref().map(|id| id.0.clone()),
            },
            ClaimState::GeneralKnowledge { at, by } => Self::GeneralKnowledge {
                at: *at,
                by: (*by).into(),
            },
            ClaimState::Reopened { cause, previous } => Self::Reopened {
                cause: cause.into(),
                previous: Box::new(previous.as_ref().into()),
            },
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum VerdictKindDto {
    Unsupported,
    Contradicted,
    NoSource,
    CouldNotJudge,
    GeneralKnowledgeProposed,
    /// The Verifier's verdict on a claim the local checker verified.
    Disputed {
        verifier: Box<VerdictKindDto>,
    },
}

impl From<&VerdictKind> for VerdictKindDto {
    fn from(kind: &VerdictKind) -> Self {
        match kind {
            VerdictKind::Unsupported => Self::Unsupported,
            VerdictKind::Contradicted => Self::Contradicted,
            VerdictKind::NoSource => Self::NoSource,
            VerdictKind::CouldNotJudge => Self::CouldNotJudge,
            VerdictKind::GeneralKnowledgeProposed => Self::GeneralKnowledgeProposed,
            VerdictKind::Disputed { verifier } => Self::Disputed {
                verifier: Box::new(verifier.as_ref().into()),
            },
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum ActorDto {
    Machine,
    User,
}

impl From<Actor> for ActorDto {
    fn from(actor: Actor) -> Self {
        match actor {
            Actor::Machine => Self::Machine,
            Actor::User => Self::User,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(tag = "cause", rename_all = "snake_case")]
pub enum ReopenCauseDto {
    SentenceChanged,
    SourceDeleted {
        #[serde(rename = "sourceVersionId")]
        source_version_id: String,
    },
    SourceReplaced {
        from: String,
        to: String,
    },
    Unconfirmed,
    VerifierDispute,
}

impl From<&ReopenCause> for ReopenCauseDto {
    fn from(cause: &ReopenCause) -> Self {
        match cause {
            ReopenCause::SentenceChanged => Self::SentenceChanged,
            ReopenCause::SourceDeleted(version) => Self::SourceDeleted {
                source_version_id: version.clone(),
            },
            ReopenCause::SourceReplaced { from, to } => Self::SourceReplaced {
                from: from.clone(),
                to: to.clone(),
            },
            ReopenCause::Unconfirmed => Self::Unconfirmed,
            ReopenCause::VerifierDispute => Self::VerifierDispute,
        }
    }
}

/// A compared passage and who added its source; `addedBy` is null for an
/// execution, which has no source.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ClaimEvidencePassageDto {
    pub passage: LearningEvidencePassageDto,
    pub added_by: Option<SourceAddedByDto>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum SourceAddedByDto {
    User,
    Research,
    Outline,
    Import,
    Verifier,
}

impl From<SourceAddedBy> for SourceAddedByDto {
    fn from(added_by: SourceAddedBy) -> Self {
        match added_by {
            SourceAddedBy::User => Self::User,
            SourceAddedBy::Research => Self::Research,
            SourceAddedBy::Outline => Self::Outline,
            SourceAddedBy::Import => Self::Import,
            SourceAddedBy::Verifier => Self::Verifier,
        }
    }
}

impl From<SourceAddedByDto> for SourceAddedBy {
    fn from(added_by: SourceAddedByDto) -> Self {
        match added_by {
            SourceAddedByDto::User => Self::User,
            SourceAddedByDto::Research => Self::Research,
            SourceAddedByDto::Outline => Self::Outline,
            SourceAddedByDto::Import => Self::Import,
            SourceAddedByDto::Verifier => Self::Verifier,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SourceRefDto {
    pub source_version_id: String,
    pub title: String,
    pub url: Option<String>,
    pub added_by: SourceAddedByDto,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ResearchAttemptDto {
    pub queries: Vec<String>,
    pub sources_added: Vec<SourceRefDto>,
    pub passages_found: usize,
    pub at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RepairAttemptDto {
    pub attempt: u8,
    pub escalated: bool,
    pub before: String,
    pub after: Option<String>,
    pub outcome: RepairOutcomeDto,
    pub at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum RepairOutcomeDto {
    Rewritten,
    Removed,
    Rejected { reason: String },
    SupersededByUser,
}

impl From<&RepairOutcome> for RepairOutcomeDto {
    fn from(outcome: &RepairOutcome) -> Self {
        match outcome {
            RepairOutcome::Rewritten => Self::Rewritten,
            RepairOutcome::Removed => Self::Removed,
            RepairOutcome::Rejected { reason } => Self::Rejected {
                reason: reason.clone(),
            },
            RepairOutcome::SupersededByUser => Self::SupersededByUser,
        }
    }
}

/// A code example's run, as the checker saw it (§5.5).
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionDto {
    pub observation_id: String,
    pub runtime: String,
    pub code: String,
    pub result: LearningLabExecutionResult,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct HumanVerdictDto {
    pub at: i64,
    pub note: Option<String>,
    pub bound_sources: Vec<SourceRefDto>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum ClaimActionKindDto {
    Confirm,
    Edit,
    Remove,
    GeneralKnowledge,
    AttachEvidence,
    ResearchWithHint,
    Unconfirm,
    Undo,
}

impl From<ClaimActionKind> for ClaimActionKindDto {
    fn from(action: ClaimActionKind) -> Self {
        match action {
            ClaimActionKind::Confirm => Self::Confirm,
            ClaimActionKind::Edit => Self::Edit,
            ClaimActionKind::Remove => Self::Remove,
            ClaimActionKind::GeneralKnowledge => Self::GeneralKnowledge,
            ClaimActionKind::AttachEvidence => Self::AttachEvidence,
            ClaimActionKind::ResearchWithHint => Self::ResearchWithHint,
            ClaimActionKind::Unconfirm => Self::Unconfirm,
            ClaimActionKind::Undo => Self::Undo,
        }
    }
}

/// What the user asked for on a claim (§5.1, §5.4); `resolve_learning_claim`
/// carries it with the lesson's `expected_revision`.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum ClaimActionRequestDto {
    Confirm {
        note: Option<String>,
        #[serde(rename = "sourceVersionIds")]
        source_version_ids: Vec<String>,
    },
    Unconfirm,
    Edit {
        before: String,
        after: String,
    },
    Remove,
    GeneralKnowledge,
    AcceptAllGeneralKnowledge,
    ResearchWithHint {
        hint: String,
    },
    AttachSavedSource {
        #[serde(rename = "sourceVersionId")]
        source_version_id: String,
        #[serde(rename = "startByte")]
        start_byte: usize,
        #[serde(rename = "endByte")]
        end_byte: usize,
    },
    AttachLink {
        url: String,
    },
    AttachDocument {
        #[serde(rename = "documentId")]
        document_id: String,
    },
    AttachText {
        title: String,
        text: String,
    },
    Undo,
}

/// Why a lesson's preparation stopped, verbatim for "Copy details" (§8.4).
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct StopReasonDto {
    pub code: String,
    pub message: String,
    pub fix: String,
    pub at: i64,
    pub request: Option<RequestFactsDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RequestFactsDto {
    pub prompt_tokens_exact: Option<usize>,
    pub prompt_tokens_estimated: Option<usize>,
    pub window_tokens: usize,
    pub output_reserved: usize,
    pub server_kind: String,
    pub server_message_sanitized: Option<String>,
    pub attempts: u32,
    pub step: String,
}

impl From<&StopReason> for StopReasonDto {
    fn from(reason: &StopReason) -> Self {
        Self {
            code: reason.code.clone(),
            message: reason.message.clone(),
            fix: reason.fix.clone(),
            at: reason.at,
            request: reason.request.as_ref().map(RequestFactsDto::from),
        }
    }
}

impl From<&RequestFacts> for RequestFactsDto {
    fn from(facts: &RequestFacts) -> Self {
        Self {
            prompt_tokens_exact: facts.prompt_tokens_exact,
            prompt_tokens_estimated: facts.prompt_tokens_estimated,
            window_tokens: facts.window_tokens,
            output_reserved: facts.output_reserved,
            server_kind: facts.server_kind.clone(),
            server_message_sanitized: facts.server_message_sanitized.clone(),
            attempts: facts.attempts,
            step: facts.step.clone(),
        }
    }
}

//! A claim's audit trail as the card's History shows it: one entry per
//! stored event, with who, when, under which job and checker.

use lattice_core::domain::learning::claim_review::{
    AttachOutcome, Check, CheckResult, Checker, ClaimEvent, EvidenceSeen, Provenance,
    RecordedEvent, UserAction,
};
use serde::{Deserialize, Serialize};

use super::{
    ActorDto, ReopenCauseDto, RepairOutcomeDto, StandingAssessedDto, StopReasonDto, VerdictKindDto,
};

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ClaimEventDto {
    pub ordinal: u32,
    pub actor: ActorDto,
    pub at: i64,
    pub job_id: Option<String>,
    pub app_version: String,
    pub checker_model: Option<String>,
    pub event: ClaimEventPayloadDto,
}

impl From<&RecordedEvent> for ClaimEventDto {
    fn from(record: &RecordedEvent) -> Self {
        Self {
            ordinal: record.ordinal,
            actor: record.provenance.actor.into(),
            at: record.provenance.at,
            job_id: record.provenance.job_id.clone(),
            app_version: record.provenance.app_version.clone(),
            checker_model: record.provenance.checker_model.clone(),
            event: (&record.event).into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ClaimEventPayloadDto {
    Extracted {
        field: String,
        quote: String,
        span: Option<(usize, usize)>,
        statement: String,
    },
    Checked {
        check: CheckDto,
    },
    Researched {
        queries: Vec<String>,
        #[serde(rename = "sourcesAdded")]
        sources_added: Vec<String>,
        #[serde(rename = "passagesFound")]
        passages_found: usize,
    },
    RepairAttempted {
        attempt: u8,
        escalated: bool,
        before: String,
        after: Option<String>,
        outcome: RepairOutcomeDto,
    },
    Executed {
        #[serde(rename = "observationId")]
        observation_id: String,
        passed: bool,
        evidence: String,
    },
    /// The section's citation moved to the passage that supports the claim,
    /// found elsewhere in the course; `addedSource` when the lesson had to
    /// number that source to cite it.
    CitationRepaired {
        cited: Vec<String>,
        #[serde(rename = "sourceVersionId")]
        source_version_id: String,
        quote: String,
        #[serde(rename = "addedSource")]
        added_source: bool,
    },
    UserAction {
        action: UserActionDto,
        note: Option<String>,
    },
    EvidenceAttached {
        #[serde(rename = "sourceVersionId")]
        source_version_id: String,
        provenance: ProvenanceDto,
        result: AttachOutcomeDto,
    },
    Superseded {
        successor: Option<String>,
    },
    Reopened {
        cause: ReopenCauseDto,
    },
    Undone {
        #[serde(rename = "eventOrdinal")]
        event_ordinal: u32,
    },
    Stopped {
        reason: StopReasonDto,
    },
    /// What the provenance gate weighed (source provenance §18.7).
    StandingAssessed {
        assessment: StandingAssessedDto,
    },
}

impl From<&ClaimEvent> for ClaimEventPayloadDto {
    fn from(event: &ClaimEvent) -> Self {
        match event {
            ClaimEvent::Extracted {
                location,
                statement,
            } => Self::Extracted {
                field: location.field.clone(),
                quote: location.quote.clone(),
                span: location.span,
                statement: statement.clone(),
            },
            ClaimEvent::Checked(check) => Self::Checked {
                check: check.into(),
            },
            ClaimEvent::Researched {
                queries,
                sources_added,
                passages_found,
                ..
            } => Self::Researched {
                queries: queries.clone(),
                sources_added: sources_added.clone(),
                passages_found: *passages_found,
            },
            ClaimEvent::RepairAttempted {
                attempt,
                escalated,
                before,
                after,
                outcome,
            } => Self::RepairAttempted {
                attempt: *attempt,
                escalated: *escalated,
                before: before.clone(),
                after: after.clone(),
                outcome: outcome.into(),
            },
            ClaimEvent::Executed {
                observation_id,
                passed,
                evidence,
            } => Self::Executed {
                observation_id: observation_id.clone(),
                passed: *passed,
                evidence: evidence.clone(),
            },
            ClaimEvent::CitationRepaired {
                cited,
                source_version_id,
                quote,
                added_source,
            } => Self::CitationRepaired {
                cited: cited.clone(),
                source_version_id: source_version_id.clone(),
                quote: quote.clone(),
                added_source: *added_source,
            },
            ClaimEvent::UserAction { action, note } => Self::UserAction {
                action: action.into(),
                note: note.clone(),
            },
            ClaimEvent::EvidenceAttached {
                source_version_id,
                provenance,
                result,
            } => Self::EvidenceAttached {
                source_version_id: source_version_id.clone(),
                provenance: provenance.into(),
                result: result.into(),
            },
            ClaimEvent::Superseded { successor } => Self::Superseded {
                successor: successor.as_ref().map(|id| id.0.clone()),
            },
            ClaimEvent::Reopened { cause } => Self::Reopened {
                cause: cause.into(),
            },
            ClaimEvent::Undone { event_ordinal } => Self::Undone {
                event_ordinal: *event_ordinal,
            },
            ClaimEvent::Stopped { reason } => Self::Stopped {
                reason: reason.into(),
            },
            ClaimEvent::StandingAssessed {
                trust,
                reason,
                cause,
            } => Self::StandingAssessed {
                assessment: StandingAssessedDto::of(trust, reason, cause.as_ref()),
            },
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CheckDto {
    pub checker: CheckerDto,
    pub result: CheckResultDto,
    pub reason: String,
    pub fingerprint: String,
    pub evidence: Vec<EvidenceSeenDto>,
    pub window_tokens: usize,
}

impl From<&Check> for CheckDto {
    fn from(check: &Check) -> Self {
        Self {
            checker: (&check.checker).into(),
            result: (&check.result).into(),
            reason: check.reason.clone(),
            fingerprint: check.fingerprint.0.clone(),
            evidence: check.evidence.iter().map(EvidenceSeenDto::from).collect(),
            window_tokens: check.window_tokens,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CheckerDto {
    pub role: CheckerRoleDto,
    pub model: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum CheckerRoleDto {
    Local,
    Verifier,
}

impl From<&Checker> for CheckerDto {
    fn from(checker: &Checker) -> Self {
        let (role, model) = match checker {
            Checker::Local { model } => (CheckerRoleDto::Local, model),
            Checker::Verifier { model } => (CheckerRoleDto::Verifier, model),
        };
        Self {
            role,
            model: model.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(tag = "verdict", rename_all = "snake_case")]
pub enum CheckResultDto {
    Supported {
        #[serde(rename = "sourceVersionId")]
        source_version_id: String,
        #[serde(rename = "supportingQuote")]
        supporting_quote: String,
    },
    /// Supported by a passage of another course source than the ones its
    /// section cites; the lesson's citation is repaired to it.
    SupportedElsewhere {
        cited: Vec<String>,
        #[serde(rename = "sourceVersionId")]
        source_version_id: String,
        #[serde(rename = "supportingQuote")]
        supporting_quote: String,
    },
    Failed {
        kind: VerdictKindDto,
    },
}

impl From<&CheckResult> for CheckResultDto {
    fn from(result: &CheckResult) -> Self {
        match result {
            CheckResult::Supported {
                source_version_id,
                supporting_quote,
            } => Self::Supported {
                source_version_id: source_version_id.clone(),
                supporting_quote: supporting_quote.clone(),
            },
            CheckResult::SupportedElsewhere {
                cited,
                source_version_id,
                supporting_quote,
            } => Self::SupportedElsewhere {
                cited: cited.clone(),
                source_version_id: source_version_id.clone(),
                supporting_quote: supporting_quote.clone(),
            },
            CheckResult::Failed { kind } => Self::Failed { kind: kind.into() },
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceSeenDto {
    pub source_version_id: String,
    pub start_byte: usize,
    pub end_byte: usize,
    pub text_sha256: String,
}

impl From<&EvidenceSeen> for EvidenceSeenDto {
    fn from(seen: &EvidenceSeen) -> Self {
        Self {
            source_version_id: seen.source_version_id.clone(),
            start_byte: seen.start_byte,
            end_byte: seen.end_byte,
            text_sha256: seen.text_sha256.clone(),
        }
    }
}

/// A recorded user action. A confirmation shows the sources it was bound
/// to; the quote digest it is bound to stays on the backend.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum UserActionDto {
    Confirm {
        #[serde(rename = "boundSourceVersionIds")]
        bound_source_version_ids: Vec<String>,
    },
    Unconfirm,
    Edit {
        before: String,
        after: String,
    },
    Remove,
    GeneralKnowledge,
    ResearchWithHint {
        hint: String,
    },
}

impl From<&UserAction> for UserActionDto {
    fn from(action: &UserAction) -> Self {
        match action {
            UserAction::Confirm { bound } => Self::Confirm {
                bound_source_version_ids: bound.source_version_ids.clone(),
            },
            UserAction::Unconfirm => Self::Unconfirm,
            UserAction::Edit { before, after } => Self::Edit {
                before: before.clone(),
                after: after.clone(),
            },
            UserAction::Remove => Self::Remove,
            UserAction::GeneralKnowledge => Self::GeneralKnowledge,
            UserAction::ResearchWithHint { hint } => Self::ResearchWithHint { hint: hint.clone() },
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(tag = "input", rename_all = "snake_case")]
pub enum ProvenanceDto {
    SavedPassage {
        #[serde(rename = "startByte")]
        start_byte: usize,
        #[serde(rename = "endByte")]
        end_byte: usize,
    },
    Link {
        url: String,
    },
    Document {
        #[serde(rename = "documentId")]
        document_id: String,
    },
    PastedText,
}

impl From<&Provenance> for ProvenanceDto {
    fn from(provenance: &Provenance) -> Self {
        match provenance {
            Provenance::SavedPassage {
                start_byte,
                end_byte,
            } => Self::SavedPassage {
                start_byte: *start_byte,
                end_byte: *end_byte,
            },
            Provenance::Link { url } => Self::Link { url: url.clone() },
            Provenance::Document { document_id } => Self::Document {
                document_id: document_id.clone(),
            },
            Provenance::PastedText => Self::PastedText,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum AttachOutcomeDto {
    Checked { check: CheckDto },
    NoPassage,
}

impl From<&AttachOutcome> for AttachOutcomeDto {
    fn from(outcome: &AttachOutcome) -> Self {
        match outcome {
            AttachOutcome::Checked(check) => Self::Checked {
                check: check.into(),
            },
            AttachOutcome::NoPassage => Self::NoPassage,
        }
    }
}

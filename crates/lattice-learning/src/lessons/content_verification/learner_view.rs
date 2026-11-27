//! The private report owns its public projection. No assessment text is copied
//! into a learner DTO; malformed data is unavailable, never a checked claim.
use super::cited_text::{number, Numbered};
use super::claim_ledger::stands_in_part;
use super::provenance_gate::FindingTrust;
use super::resolution::{ended_mark, standing, Placed, Resolution, Standing};
use super::support::{anchor_dto, cited};
use super::{ClaimVerdict, EvidencePassage, Finding, LessonVerificationReport};
use crate::{
    dto::{LearningLessonDto, LearningSourceTrustDto, LearningSufficiencyDto},
    lesson_evidence::{
        LearningCitationDto, LearningClaimEvidenceDto, LearningClaimPartDto,
        LearningClaimPartKindDto, LearningClaimResolutionDto, LearningEvidencePassageDto,
        LearningLessonEvidenceDto, LearningSectionTextDto, LearningStatementClassDto,
        SavedEvidenceVersion, UnverifiedReasonDto,
    },
    references::trust::dto_map,
};
use lattice_core::application::services::claim_verification::PartKind;
use lattice_core::domain::learning::claim_review::{
    ClaimState, Class, Ledger, ReopenCause, UnverifiedReason,
};
use lattice_core::shared::error::{AppError, Result};
use std::collections::{HashMap, HashSet};

/// A lesson's claims as its ledger holds them now, by claim id.
#[derive(Default)]
pub(crate) struct ClaimStates {
    states: HashMap<String, ClaimState>,
    /// Each claim's id, unit and quote.
    places: Vec<(String, usize, String)>,
}

impl ClaimStates {
    pub(crate) fn of(ledger: &Ledger) -> Self {
        Self {
            states: ledger
                .claims
                .iter()
                .map(|entry| (entry.claim_id().0.clone(), entry.state().clone()))
                .collect(),
            places: ledger
                .claims
                .iter()
                .map(|entry| {
                    let location = entry.location();
                    (
                        entry.claim_id().0.clone(),
                        location.unit_index,
                        location.quote.clone(),
                    )
                })
                .collect(),
        }
    }
}

/// The level of `source_version_id` for the claim's subject, when it
/// supports the claim.
fn passage_trust(trust: &FindingTrust, source_version_id: &str) -> Option<LearningSourceTrustDto> {
    trust
        .supports
        .iter()
        .find(|support| support.source_version_id == source_version_id)
        .map(|support| dto_map::support_trust(&trust.profile, support))
}

/// Whether the claim's support met the bar for its kind (§18.9).
fn sufficiency(trust: &FindingTrust) -> LearningSufficiencyDto {
    dto_map::sufficiency(&trust.profile, &trust.decision, trust.available)
}

/// The class the learner is told of; a statement about the lesson itself, which
/// the model sorted Internal and the veto passed, is told none.
pub(super) fn statement_class(class: Class) -> Option<LearningStatementClassDto> {
    match class {
        Class::Sourced => Some(LearningStatementClassDto::Sourced),
        Class::Reasoning => Some(LearningStatementClassDto::Reasoning),
        Class::Common => Some(LearningStatementClassDto::Common),
        Class::Internal => None,
    }
}

/// The DTO of `passage` as its saved version names it; a passage that is not
/// a run and whose version is not saved is unavailable.
fn passage_dto(
    passage: &EvidencePassage,
    by_id: &HashMap<&str, &SavedEvidenceVersion>,
    trust: Option<LearningSourceTrustDto>,
) -> Result<LearningEvidencePassageDto> {
    let version = by_id.get(passage.source_id.as_str());
    if version.is_none() && !passage.is_run() {
        return Err(unavailable());
    }
    if passage.text.is_empty() || passage.end_byte < passage.start_byte {
        return Err(unavailable());
    }
    Ok(LearningEvidencePassageDto {
        source_version_id: passage.source_id.clone(),
        title: version.map_or_else(
            || "Executed example".into(),
            |version| version.title.clone(),
        ),
        url: version.and_then(|version| {
            version
                .resolved_url
                .clone()
                .or_else(|| version.requested_url.clone())
        }),
        text: passage.text.clone(),
        start_byte: passage.start_byte,
        end_byte: passage.end_byte,
        retrieval_kind: passage.retrieval_kind.clone(),
        trust,
    })
}

fn part_kind(kind: PartKind) -> LearningClaimPartKindDto {
    match kind {
        PartKind::Stated => LearningClaimPartKindDto::Stated,
        PartKind::Follows => LearningClaimPartKindDto::Follows,
        PartKind::Common => LearningClaimPartKindDto::Common,
        PartKind::Own => LearningClaimPartKindDto::Own,
        PartKind::SpecificMissing => LearningClaimPartKindDto::SpecificMissing,
        PartKind::Contradicted => LearningClaimPartKindDto::Contradicted,
    }
}

/// What the learner is told of a claim: `stood`, its resolution as it stands;
/// `verified`, the ledger verified it; `verdict`, its finding's; `source_removed`,
/// a source it stood on is gone (`in_ledger`: the ledger knows the claim); and
/// `sentence`, why another claim on its sentence was published not verified.
pub(super) fn told(
    stood: Option<&Resolution>,
    verified: bool,
    verdict: ClaimVerdict,
    (source_removed, in_ledger): (bool, bool),
    sentence: Option<UnverifiedReason>,
) -> (
    LearningClaimResolutionDto,
    Option<String>,
    Option<UnverifiedReasonDto>,
) {
    use LearningClaimResolutionDto::{ConfirmedByYou, NotVerified, SourceRemoved, Verified};
    match stood {
        _ if in_ledger && source_removed => (SourceRemoved, None, None),
        Some(Resolution::Confirmed { note, .. }) => (ConfirmedByYou, note.clone(), None),
        Some(Resolution::Unverified { reason }) => (NotVerified, None, Some((*reason).into())),
        None if verdict != ClaimVerdict::Supported && !verified => {
            let reason = verdict
                .failure(true)
                .map_or(UnverifiedReason::Unsupported, |kind| {
                    UnverifiedReason::of(&kind)
                });
            (NotVerified, None, Some(reason.into()))
        }
        None if source_removed => (SourceRemoved, None, None),
        None => match sentence {
            Some(reason) => (NotVerified, None, Some(reason.into())),
            None => (Verified, None, None),
        },
    }
}

/// Each sentence that carries a claim published not verified, and why.
pub(super) fn unverified_sentences<'f>(
    claims: impl Iterator<Item = (&'f Finding, Option<Resolution>)>,
) -> HashMap<(usize, &'f str), UnverifiedReason> {
    claims
        .filter_map(|(finding, resolution)| match resolution {
            Some(Resolution::Unverified { reason }) => {
                Some(((finding.unit, finding.quote.as_str()), reason))
            }
            _ => None,
        })
        .collect()
}

fn unavailable() -> AppError {
    AppError::InvalidState("Saved lesson evidence is unavailable: its report or source bindings do not match this lesson.".into())
}

/// What the learner is told of a finding follows its claim as the ledger holds
/// it now; a finding whose claim the ledger does not know keeps the resolution
/// it was published with. The second is whether the ledger says the machine
/// verified it.
pub(super) fn resolved(
    finding: &Finding,
    states: &ClaimStates,
    finding_claims: &HashSet<&str>,
) -> (Option<Resolution>, bool) {
    let Some(state) = finding
        .claim_id
        .as_ref()
        .and_then(|id| states.states.get(id))
    else {
        return (finding.resolution.clone(), false);
    };
    let (own, verified) = match standing(state, &|id| states.states.get(&id.0)) {
        Standing::Resolved(resolution) => (Some(resolution), false),
        Standing::Verified => (None, true),
        Standing::Unsettled => (
            Some(Resolution::Unverified {
                reason: UnverifiedReason::Unchecked,
            }),
            false,
        ),
        Standing::Gone => (finding.resolution.clone(), false),
    };
    match own {
        Some(own) => (Some(own), verified),
        None => match ended_mark(
            states.places.iter().filter_map(|(id, unit, quote)| {
                Some(Placed {
                    id,
                    unit: *unit,
                    quote,
                    state: states.states.get(id)?,
                })
            }),
            &|id| finding_claims.contains(id),
            finding.unit,
            &finding.quote,
        ) {
            Some(mark) => (Some(mark), false),
            None => (None, verified),
        },
    }
}

impl LessonVerificationReport {
    pub(crate) fn learner_view(
        &self,
        lesson: &LearningLessonDto,
        versions: &[SavedEvidenceVersion],
        states: &ClaimStates,
    ) -> Result<LearningLessonEvidenceDto> {
        let blocks = lesson
            .blocks
            .iter()
            .map(serde_json::to_value)
            .collect::<serde_json::Result<Vec<_>>>()?;
        if self.lesson_id != lesson.id
            || self.policy.is_empty()
            || self.content_sha256.is_empty()
            || self.checker_model.is_empty()
            || self.findings.iter().any(|finding| {
                finding.quote.trim().is_empty()
                    || finding.statement.trim().is_empty()
                    || blocks
                        .get(finding.unit)
                        .is_some_and(|block| !stands_in_part(block, &finding.quote))
            })
        {
            return Err(unavailable());
        }
        let by_id: HashMap<_, _> = versions
            .iter()
            .map(|version| (version.id.as_str(), version))
            .collect();
        let active: HashSet<_> = versions
            .iter()
            .filter(|version| version.deleted_at.is_none())
            .filter_map(|version| version.active_version_id.as_deref())
            .collect();
        let expected: HashSet<_> = self
            .sources
            .iter()
            .map(|source| source.id.as_str())
            .collect();
        let sources_current = active == expected
            && self.sources.iter().all(|source| {
                by_id
                    .get(source.id.as_str())
                    .is_some_and(|version| version.content_sha256 == source.sha256)
            });
        // A statement the model sorted Internal ends as no claim: the learner reads it as lesson text alone.
        let claims = || self.findings.iter().filter(|finding| finding.is_claim());
        let finding_claims: HashSet<&str> = self
            .findings
            .iter()
            .filter_map(|finding| finding.claim_id.as_deref())
            .collect();
        // A sentence that carries a claim published not verified reads not
        // verified, whatever another claim on it found.
        let unverified = unverified_sentences(
            claims().map(|finding| (finding, resolved(finding, states, &finding_claims).0)),
        );
        let teaching: Vec<&Finding> = claims()
            .filter(|finding| finding.unit < lesson.blocks.len())
            .collect();
        let bodies: Vec<&str> = lesson
            .blocks
            .iter()
            .map(|block| {
                if block.check.is_some() {
                    ""
                } else {
                    block.body.as_str()
                }
            })
            .collect();
        let Numbered {
            passages: numbered,
            cites,
            sections,
        } = number(&bodies, &teaching);
        let citations = numbered
            .iter()
            .enumerate()
            .map(|(index, passage)| {
                Ok(LearningCitationDto {
                    number: u32::try_from(index + 1).unwrap_or(u32::MAX),
                    source_id: by_id
                        .get(passage.source_id.as_str())
                        .filter(|_| !passage.is_run())
                        .map(|version| version.source_id.clone()),
                    passage: passage_dto(passage, &by_id, None)?,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let teaching_claims = teaching
            .iter()
            .copied()
            .enumerate()
            .map(|(index, finding)| {
                let passages = finding
                    .evidence
                    .iter()
                    .map(|passage| {
                        passage_dto(
                            passage,
                            &by_id,
                            finding
                                .trust
                                .as_deref()
                                .and_then(|trust| passage_trust(trust, &passage.source_id)),
                        )
                    })
                    .collect::<Result<Vec<_>>>()?;
                let ledger_state = finding
                    .claim_id
                    .as_ref()
                    .and_then(|id| states.states.get(id));
                let source_removed = match ledger_state {
                    Some(state) => matches!(
                        state,
                        ClaimState::Reopened {
                            cause: ReopenCause::SourceDeleted(_),
                            ..
                        }
                    ),
                    None => finding.evidence.iter().any(|passage| {
                        by_id
                            .get(passage.source_id.as_str())
                            .is_some_and(|version| version.deleted_at.is_some())
                    }),
                };
                let (stood, verified) = resolved(finding, states, &finding_claims);
                let (resolution, note, unverified_reason) = told(
                    stood.as_ref(),
                    verified,
                    finding.verdict,
                    (source_removed, ledger_state.is_some()),
                    unverified
                        .get(&(finding.unit, finding.quote.as_str()))
                        .copied(),
                );
                Ok(LearningClaimEvidenceDto {
                    section_index: finding.unit,
                    content_quote: finding.quote.clone(),
                    claim: finding.statement.clone(),
                    verdict: finding.verdict.as_str().into(),
                    reason: finding.reason.clone(),
                    supporting_quote: finding.supporting_quote.clone(),
                    passages,
                    resolution,
                    note,
                    unverified_reason,
                    claim_id: finding.claim_id.clone(),
                    cites: cites.get(index).cloned().unwrap_or_default(),
                    sufficiency: finding.trust.as_deref().map(sufficiency),
                    class: finding
                        .class
                        .and_then(|classed| statement_class(classed.class)),
                    label: finding.label.as_ref().and_then(dto_map::label_name),
                    parts: finding
                        .two_pass
                        .parts
                        .iter()
                        .map(|part| LearningClaimPartDto {
                            part: part.part.clone(),
                            kind: part_kind(part.kind),
                            source_version_id: part.source_id.clone().or_else(|| {
                                part.anchors
                                    .first()
                                    .map(|anchor| anchor.source_version_id.clone())
                            }),
                            anchors: part
                                .anchors
                                .iter()
                                .filter_map(|anchor| {
                                    finding
                                        .evidence
                                        .iter()
                                        .find_map(|passage| anchor_dto(passage, anchor))
                                })
                                .collect(),
                        })
                        .collect(),
                    relation: cited(finding).map(|cited| cited.relation.into()),
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let confirmed_count = teaching_claims
            .iter()
            .filter(|claim| claim.resolution == LearningClaimResolutionDto::ConfirmedByYou)
            .count();
        Ok(LearningLessonEvidenceDto {
            policy: self.policy.clone(),
            checked_at: self.checked_at,
            checker_model: self.checker_model.clone(),
            retrieval_mode: self.retrieval_mode.clone(),
            embedding_model: self.embedding_model.clone(),
            content_sha256: self.content_sha256.clone(),
            claim_count: teaching_claims.len(),
            confirmed_count,
            executed_examples: self.executions.len(),
            unexecuted_languages: self.unexecuted_languages.clone(),
            sources_current,
            teaching_claims,
            citations,
            sections: sections
                .into_iter()
                .enumerate()
                .map(|(section_index, pieces)| LearningSectionTextDto {
                    section_index,
                    pieces,
                })
                .collect(),
        })
    }
}

#[cfg(test)]
#[path = "learner_view_support_tests.rs"]
mod support_tests;

#[cfg(test)]
#[path = "learner_view_tests.rs"]
mod tests;

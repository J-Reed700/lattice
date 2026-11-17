//! The private report owns its public projection. No assessment text is copied
//! into a learner DTO; malformed data is unavailable, never a checked claim.
use super::cited_text::{number, Numbered};
use super::claim_ledger::stands_in_part;
use super::provenance_gate::FindingTrust;
use super::resolution::{ended_mark, standing, Placed, Resolution, Standing};
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
                            source_version_id: part.source_id.clone(),
                        })
                        .collect(),
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
mod tests {
    use super::*;
    use crate::content_verification::{tests::attest, CheckedPart, Classed, EvidencePassage};
    use crate::dto::{LearningBlockDto, LearningBlockKind, PreparedLearningLesson};
    use crate::reference_collection::PassageKind;
    use lattice_core::domain::learning::claim_review::ClassifiedBy;
    use lattice_core::domain::provenance::Label;

    fn fixture() -> (LearningLessonDto, LessonVerificationReport) {
        let mut lesson = crate::tests::fixture().modules.remove(0).lessons.remove(0);
        lesson.blocks = vec![LearningBlockDto::new(
            LearningBlockKind::Explanation,
            "Title".into(),
            "An exact lesson statement.".into(),
            vec![],
            vec![],
        )];
        let prepared = PreparedLearningLesson {
            blocks: lesson.blocks.clone(),
            questions: vec![],
            keys: vec![],
            verification: None,
        };
        let report = attest(&lesson.id, &prepared);
        (lesson, report)
    }

    #[test]
    fn projection_preserves_the_typed_claim_and_rejects_a_changed_anchor() {
        let (mut lesson, report) = fixture();
        let view = report
            .learner_view(&lesson, &[], &ClaimStates::default())
            .unwrap();
        assert_eq!(view.teaching_claims[0].content_quote, lesson.blocks[0].body);
        assert_eq!(view.teaching_claims[0].verdict, "supported");
        lesson.blocks[0].body = "Different lesson content.".into();
        assert!(report
            .learner_view(&lesson, &[], &ClaimStates::default())
            .is_err());
    }

    #[test]
    fn a_statement_reaches_the_learner_with_its_class_its_label_and_the_parts_it_was_read_as() {
        let (lesson, mut report) = fixture();
        let plain = report
            .learner_view(&lesson, &[], &ClaimStates::default())
            .unwrap();
        // A report from before statements were sorted says none of it.
        assert_eq!(plain.teaching_claims[0].class, None);
        assert_eq!(plain.teaching_claims[0].label, None);
        assert!(plain.teaching_claims[0].parts.is_empty());

        report.findings[0].class = Some(Classed {
            class: Class::Sourced,
            by: ClassifiedBy::Writer,
        });
        report.findings[0].label = Some(Label::Attributed);
        report.findings[0].two_pass.parts = vec![
            CheckedPart {
                part: "Quillet is a lexer".into(),
                kind: PartKind::Stated,
                source_id: Some("version-1".into()),
            },
            CheckedPart {
                part: "a lexer turns characters into tokens".into(),
                kind: PartKind::Common,
                source_id: None,
            },
        ];
        let claim = &report
            .learner_view(&lesson, &[], &ClaimStates::default())
            .unwrap()
            .teaching_claims[0];

        assert_eq!(claim.class, Some(LearningStatementClassDto::Sourced));
        assert_eq!(claim.label.as_deref(), Some("attributed"));
        assert_eq!(claim.parts.len(), 2);
        assert_eq!(claim.parts[0].kind, LearningClaimPartKindDto::Stated);
        assert_eq!(
            claim.parts[0].source_version_id.as_deref(),
            Some("version-1")
        );
        assert_eq!(claim.parts[1].kind, LearningClaimPartKindDto::Common);
        assert_eq!(claim.parts[1].source_version_id, None);

        // The other classes.
        for (class, expected) in [
            (Class::Reasoning, LearningStatementClassDto::Reasoning),
            (Class::Common, LearningStatementClassDto::Common),
        ] {
            report.findings[0].class = Some(Classed {
                class,
                by: ClassifiedBy::Model,
            });
            let view = report
                .learner_view(&lesson, &[], &ClaimStates::default())
                .unwrap();
            assert_eq!(view.teaching_claims[0].class, Some(expected));
        }
    }

    #[test]
    fn a_statement_the_model_called_internal_is_not_shown_or_counted_as_a_checked_claim() {
        let (lesson, mut report) = fixture();
        report.findings.push(report.findings[0].clone());
        report.findings[1].class = Some(Classed {
            class: Class::Internal,
            by: ClassifiedBy::Model,
        });

        let view = report
            .learner_view(&lesson, &[], &ClaimStates::default())
            .unwrap();

        assert_eq!(view.teaching_claims.len(), 1);
        assert_eq!(view.teaching_claims[0].class, None);
        assert_eq!(view.claim_count, 1);
    }

    #[test]
    fn the_report_counts_the_claims_it_lists() {
        let (lesson, mut report) = fixture();
        let mut question = report.findings[0].clone();
        question.unit = lesson.blocks.len();
        report.findings.push(question);

        let view = report
            .learner_view(&lesson, &[], &ClaimStates::default())
            .unwrap();

        assert_eq!(view.claim_count, view.teaching_claims.len());
        assert_eq!(view.claim_count, 1);
    }

    fn not_verified(reason: UnverifiedReason) -> Option<Resolution> {
        Some(Resolution::Unverified { reason })
    }

    #[test]
    fn a_claim_published_not_verified_reads_so_with_its_reason_and_is_never_verified() {
        let (lesson, mut report) = fixture();
        report.findings[0].verdict = ClaimVerdict::Unsupported;
        report.findings[0].resolution = not_verified(UnverifiedReason::Unsupported);

        let view = report
            .learner_view(&lesson, &[], &ClaimStates::default())
            .unwrap();

        let claim = &view.teaching_claims[0];
        assert_eq!(claim.resolution, LearningClaimResolutionDto::NotVerified);
        assert_eq!(
            claim.unverified_reason,
            Some(UnverifiedReasonDto::Unsupported)
        );
        assert_eq!(view.claim_count, 1);
        assert_eq!(view.confirmed_count, 0);
        let wire = serde_json::to_value(claim).unwrap();
        assert_eq!(wire["resolution"], "not_verified");
        assert_eq!(wire["unverifiedReason"], "unsupported");
    }

    #[test]
    fn a_report_counts_the_statements_it_publishes_not_verified_and_says_so() {
        use crate::content_verification::not_verified_line;
        let (_, mut report) = fixture();
        assert_eq!(report.unverified_count(), 0);
        report.findings[0].verdict = ClaimVerdict::Unsupported;
        report.findings[0].resolution = not_verified(UnverifiedReason::Unchecked);
        assert_eq!(report.unverified_count(), 1);

        assert_eq!(not_verified_line(0), None);
        assert_eq!(
            not_verified_line(1).as_deref(),
            Some("1 statement not verified")
        );
        assert_eq!(
            not_verified_line(2).as_deref(),
            Some("2 statements not verified")
        );
    }

    #[test]
    fn a_verified_claim_carries_no_reason_on_the_wire() {
        let (lesson, report) = fixture();

        let view = report
            .learner_view(&lesson, &[], &ClaimStates::default())
            .unwrap();

        let wire = serde_json::to_value(&view.teaching_claims[0]).unwrap();
        assert_eq!(wire["resolution"], "verified");
        assert!(wire["unverifiedReason"].is_null(), "{wire}");
    }

    #[test]
    fn a_sentence_that_carries_an_unverified_claim_reads_not_verified_beside_a_verified_one() {
        let (lesson, mut report) = fixture();
        report.findings.push(report.findings[0].clone());
        report.findings[1].verdict = ClaimVerdict::Contradicted;
        report.findings[1].resolution = not_verified(UnverifiedReason::Contradicted);

        let view = report
            .learner_view(&lesson, &[], &ClaimStates::default())
            .unwrap();

        for claim in &view.teaching_claims {
            assert_eq!(claim.resolution, LearningClaimResolutionDto::NotVerified);
            assert_eq!(
                claim.unverified_reason,
                Some(UnverifiedReasonDto::Contradicted)
            );
        }
    }

    #[test]
    fn a_claim_the_user_confirmed_stays_confirmed_beside_an_unverified_one() {
        let (lesson, mut report) = fixture();
        report.findings.push(report.findings[0].clone());
        report.findings[0].verdict = ClaimVerdict::Unsupported;
        report.findings[0].resolution = Some(Resolution::Confirmed {
            at: 1,
            note: Some("Checked by hand.".into()),
        });
        report.findings[1].verdict = ClaimVerdict::Unsupported;
        report.findings[1].resolution = not_verified(UnverifiedReason::Unsupported);

        let view = report
            .learner_view(&lesson, &[], &ClaimStates::default())
            .unwrap();

        assert_eq!(
            view.teaching_claims[0].resolution,
            LearningClaimResolutionDto::ConfirmedByYou
        );
        assert_eq!(
            view.teaching_claims[1].resolution,
            LearningClaimResolutionDto::NotVerified
        );
    }

    #[test]
    fn a_claim_no_passage_supports_and_nobody_resolved_reads_not_verified_never_verified_or_unavailable(
    ) {
        for (verdict, reason) in [
            (ClaimVerdict::Unsupported, UnverifiedReasonDto::Unsupported),
            (
                ClaimVerdict::Contradicted,
                UnverifiedReasonDto::Contradicted,
            ),
            (ClaimVerdict::Unverified, UnverifiedReasonDto::Unchecked),
        ] {
            let (lesson, mut report) = fixture();
            report.findings[0].verdict = verdict;

            let view = report
                .learner_view(&lesson, &[], &ClaimStates::default())
                .unwrap_or_else(|error| {
                    panic!("{verdict:?} makes the lesson unavailable: {error}")
                });

            let claim = &view.teaching_claims[0];
            assert_eq!(claim.resolution, LearningClaimResolutionDto::NotVerified);
            assert_eq!(claim.unverified_reason, Some(reason), "{verdict:?}");
            assert_eq!(view.claim_count, 1);
            let wire = serde_json::to_value(claim).unwrap();
            assert_eq!(wire["resolution"], "not_verified");
        }
    }

    #[test]
    fn a_missing_reference_is_not_relabelled_as_executed_evidence() {
        let (lesson, mut report) = fixture();
        report.findings[0].evidence.push(EvidencePassage {
            source_id: "missing-version".into(),
            text: "Saved quote".into(),
            start_byte: 0,
            end_byte: 11,
            retrieval_kind: "lexical".into(),
            score: 1.0,
        });
        assert!(report
            .learner_view(&lesson, &[], &ClaimStates::default())
            .is_err());
        report.findings[0].evidence[0].retrieval_kind = PassageKind::Run.label().into();
        let view = report
            .learner_view(&lesson, &[], &ClaimStates::default())
            .unwrap();
        assert_eq!(
            view.teaching_claims[0].passages[0].title,
            "Executed example"
        );
    }

    #[test]
    fn a_lesson_reads_with_numbered_citations_its_statements_stand_on() {
        use lattice_core::application::contracts::replies::{ReplyPieceDto, ReplyReferenceDto};

        let (mut lesson, mut report) = fixture();
        lesson.blocks[0].body = "Quillet reads source text. It then emits tokens.".into();
        let source = |kind: &str, id: &str| EvidencePassage {
            source_id: id.into(),
            text: "Quillet reads source text.".into(),
            start_byte: 0,
            end_byte: 26,
            retrieval_kind: kind.into(),
            score: 1.0,
        };
        let cited = source(PassageKind::Cited.label(), "version-1");
        report.findings[0].quote = "Quillet reads source text.".into();
        report.findings[0].evidence = vec![cited.clone()];
        report.findings[0].class = Some(Classed {
            class: Class::Sourced,
            by: ClassifiedBy::Writer,
        });
        report.findings[0].two_pass.parts = vec![CheckedPart {
            part: "Quillet reads source text".into(),
            kind: PartKind::Stated,
            source_id: Some("version-1".into()),
        }];
        let mut second = report.findings[0].clone();
        second.quote = "It then emits tokens.".into();
        second.class = Some(Classed {
            class: Class::Reasoning,
            by: ClassifiedBy::Model,
        });
        second.two_pass.parts.clear();
        second.evidence = vec![source(PassageKind::Run.label(), "run-1"), cited];
        report.findings.push(second);
        let versions = [SavedEvidenceVersion {
            id: "version-1".into(),
            source_id: "source-1".into(),
            title: "The Quillet Guide".into(),
            resolved_url: None,
            requested_url: Some("https://example.com/quillet".into()),
            content_sha256: "x".into(),
            active_version_id: Some("version-1".into()),
            deleted_at: None,
        }];

        let view = report
            .learner_view(&lesson, &versions, &ClaimStates::default())
            .unwrap();

        assert_eq!(view.citations.len(), 2);
        assert_eq!(view.citations[0].number, 1);
        assert_eq!(view.citations[0].source_id.as_deref(), Some("source-1"));
        assert_eq!(view.citations[0].passage.title, "The Quillet Guide");
        assert_eq!(view.citations[0].passage.source_version_id, "version-1");
        assert!(view.citations[0].passage.trust.is_none());
        assert_eq!(view.citations[1].number, 2);
        assert_eq!(view.citations[1].source_id, None);
        assert_eq!(view.citations[1].passage.title, "Executed example");
        assert_eq!(view.teaching_claims[0].cites, vec![1]);
        assert_eq!(view.teaching_claims[1].cites, vec![1]);
        let mark = ReplyPieceDto::Reference {
            reference: ReplyReferenceDto {
                written: "[1]".into(),
                kind: "source".into(),
                number: 1,
                lines: None,
                name: None,
            },
        };
        let text = |text: &str| ReplyPieceDto::Text { text: text.into() };
        assert_eq!(view.sections.len(), 1);
        assert_eq!(view.sections[0].section_index, 0);
        assert_eq!(
            view.sections[0].pieces,
            vec![
                text("Quillet reads source text."),
                mark.clone(),
                text(" It then emits tokens."),
                mark,
            ]
        );
    }

    #[test]
    fn a_checks_statements_get_no_chip_and_are_numbered_after_every_placed_one() {
        use crate::dto::LearningCheckDto;
        use lattice_core::application::contracts::replies::ReplyPieceDto;

        let (mut lesson, mut report) = fixture();
        lesson.blocks[0].body = "Quillet reads source text.".into();
        lesson.blocks[0].check = Some(LearningCheckDto {
            question: "Quillet reads source text.".into(),
            reveal: "It does.".into(),
        });
        lesson.blocks.push(LearningBlockDto::new(
            LearningBlockKind::Explanation,
            "Second".into(),
            "It then emits tokens.".into(),
            vec![],
            vec![],
        ));
        let passage = |text: &str, start: usize| EvidencePassage {
            source_id: "version-1".into(),
            text: text.into(),
            start_byte: start,
            end_byte: start + text.len(),
            retrieval_kind: PassageKind::Cited.label().into(),
            score: 1.0,
        };
        report.findings[0].unit = 0;
        report.findings[0].quote = "Quillet reads source text.".into();
        report.findings[0].evidence = vec![passage("Quillet reads source text.", 0)];
        report.findings[0].class = Some(Classed {
            class: Class::Sourced,
            by: ClassifiedBy::Writer,
        });
        report.findings[0].two_pass.parts = vec![CheckedPart {
            part: "Quillet reads source text".into(),
            kind: PartKind::Stated,
            source_id: Some("version-1".into()),
        }];
        let mut second = report.findings[0].clone();
        second.unit = 1;
        second.quote = "It then emits tokens.".into();
        second.evidence = vec![passage("It then emits tokens.", 100)];
        report.findings.push(second);
        let versions = [SavedEvidenceVersion {
            id: "version-1".into(),
            source_id: "source-1".into(),
            title: "The Quillet Guide".into(),
            resolved_url: None,
            requested_url: Some("https://example.com/quillet".into()),
            content_sha256: "x".into(),
            active_version_id: Some("version-1".into()),
            deleted_at: None,
        }];

        let view = report
            .learner_view(&lesson, &versions, &ClaimStates::default())
            .unwrap();

        assert_eq!(view.citations.len(), 2);
        assert_eq!(view.citations[0].passage.start_byte, 100);
        assert_eq!(view.citations[1].passage.start_byte, 0);
        assert_eq!(view.teaching_claims[0].cites, vec![2]);
        assert_eq!(view.teaching_claims[1].cites, vec![1]);
        let pieces = |index: usize| {
            &view
                .sections
                .iter()
                .find(|section| section.section_index == index)
                .unwrap()
                .pieces
        };
        assert!(!pieces(0)
            .iter()
            .any(|piece| matches!(piece, ReplyPieceDto::Reference { .. })));
        assert!(pieces(1).iter().any(|piece| matches!(
            piece,
            ReplyPieceDto::Reference { reference } if reference.number == 1
        )));
    }

    #[test]
    fn malformed_findings_cannot_become_section_zero_or_an_unknown_verdict() {
        let (_, report) = fixture();
        let mut raw = serde_json::to_value(&report).unwrap();
        raw["findings"][0].as_object_mut().unwrap().remove("unit");
        assert!(serde_json::from_value::<LessonVerificationReport>(raw).is_err());
        let mut raw = serde_json::to_value(&report).unwrap();
        raw["findings"][0]["verdict"] = serde_json::json!("made_up");
        assert!(serde_json::from_value::<LessonVerificationReport>(raw).is_err());
    }
}

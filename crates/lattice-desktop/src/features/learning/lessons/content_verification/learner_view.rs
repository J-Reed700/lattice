//! The private report owns its public projection. No assessment text is copied
//! into a learner DTO; malformed data is unavailable, never a checked claim.
use super::provenance_gate::FindingTrust;
use super::resolution::Resolution;
use super::{ClaimVerdict, LessonVerificationReport};
use crate::features::learning::{
    dto::{LearningLessonDto, LearningSourceTrustDto, LearningSufficiencyDto},
    lesson_evidence::{
        LearningClaimEvidenceDto, LearningClaimPartDto, LearningClaimPartKindDto,
        LearningClaimResolutionDto, LearningEvidencePassageDto, LearningLessonEvidenceDto,
        LearningStatementClassDto, SavedEvidenceVersion,
    },
    references::trust::dto_map,
};
use lattice_core::application::services::claim_verification::PartKind;
use lattice_core::domain::learning::claim_review::Class;
use lattice_core::shared::error::{AppError, Result};
use std::collections::{HashMap, HashSet};

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
fn statement_class(class: Class) -> Option<LearningStatementClassDto> {
    match class {
        Class::Sourced => Some(LearningStatementClassDto::Sourced),
        Class::Reasoning => Some(LearningStatementClassDto::Reasoning),
        Class::Common => Some(LearningStatementClassDto::Common),
        Class::Internal => None,
    }
}

fn part_kind(kind: PartKind) -> LearningClaimPartKindDto {
    match kind {
        PartKind::Stated => LearningClaimPartKindDto::Stated,
        PartKind::Common => LearningClaimPartKindDto::Common,
        PartKind::SpecificMissing => LearningClaimPartKindDto::SpecificMissing,
        PartKind::Contradicted => LearningClaimPartKindDto::Contradicted,
    }
}

fn unavailable() -> AppError {
    AppError::InvalidState("Saved lesson evidence is unavailable: its report or source bindings do not match this lesson.".into())
}

impl LessonVerificationReport {
    pub(in crate::features::learning) fn learner_view(
        &self,
        lesson: &LearningLessonDto,
        versions: &[SavedEvidenceVersion],
    ) -> Result<LearningLessonEvidenceDto> {
        if self.lesson_id != lesson.id
            || self.policy.is_empty()
            || self.content_sha256.is_empty()
            || self.checker_model.is_empty()
            || self.findings.iter().any(|finding| {
                finding.quote.trim().is_empty()
                    || finding.statement.trim().is_empty()
                    || lesson.blocks.get(finding.unit).is_some_and(|block| {
                        !block.body.contains(&finding.quote)
                            && !block.title.contains(&finding.quote)
                            && !block.rubric.iter().any(|criterion| {
                                criterion.title.contains(&finding.quote)
                                    || criterion.description.contains(&finding.quote)
                            })
                    })
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
        let teaching_claims = self
            .findings
            .iter()
            .filter(|finding| finding.unit < lesson.blocks.len())
            .map(|finding| {
                let passages = finding
                    .evidence
                    .iter()
                    .map(|passage| {
                        let version = by_id.get(passage.source_id.as_str());
                        if version.is_none() && passage.retrieval_kind != "execution" {
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
                            trust: finding
                                .trust
                                .as_deref()
                                .and_then(|trust| passage_trust(trust, &passage.source_id)),
                        })
                    })
                    .collect::<Result<Vec<_>>>()?;
                let source_removed = finding.evidence.iter().any(|passage| {
                    by_id
                        .get(passage.source_id.as_str())
                        .is_some_and(|version| version.deleted_at.is_some())
                });
                let (resolution, note) = match &finding.resolution {
                    Some(Resolution::Confirmed { note, .. }) => {
                        (LearningClaimResolutionDto::ConfirmedByYou, note.clone())
                    }
                    None if source_removed => (LearningClaimResolutionDto::SourceRemoved, None),
                    None => (LearningClaimResolutionDto::Verified, None),
                };
                Ok(LearningClaimEvidenceDto {
                    section_index: finding.unit,
                    content_quote: finding.quote.clone(),
                    claim: finding.statement.clone(),
                    verdict: match finding.verdict {
                        ClaimVerdict::Supported => "supported",
                        ClaimVerdict::Contradicted => "contradicted",
                        ClaimVerdict::Unsupported => "unsupported",
                        ClaimVerdict::Unverified => "unverified",
                    }
                    .into(),
                    reason: finding.reason.clone(),
                    supporting_quote: finding.supporting_quote.clone(),
                    passages,
                    resolution,
                    note,
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
        Ok(LearningLessonEvidenceDto {
            policy: self.policy.clone(),
            checked_at: self.checked_at,
            checker_model: self.checker_model.clone(),
            retrieval_mode: self.retrieval_mode.clone(),
            embedding_model: self.embedding_model.clone(),
            content_sha256: self.content_sha256.clone(),
            claim_count: self.findings.len(),
            confirmed_count: self
                .findings
                .iter()
                .filter(|finding| matches!(finding.resolution, Some(Resolution::Confirmed { .. })))
                .count(),
            executed_examples: self.executions.len(),
            unexecuted_languages: self.unexecuted_languages.clone(),
            sources_current,
            teaching_claims,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::learning::content_verification::{
        tests::attest, CheckedPart, Classed, EvidencePassage,
    };
    use crate::features::learning::dto::{
        LearningBlockDto, LearningBlockKind, PreparedLearningLesson,
    };
    use lattice_core::domain::learning::claim_review::ClassifiedBy;
    use lattice_core::domain::provenance::Label;

    fn fixture() -> (LearningLessonDto, LessonVerificationReport) {
        let mut lesson = crate::features::learning::tests::fixture()
            .modules
            .remove(0)
            .lessons
            .remove(0);
        lesson.blocks = vec![LearningBlockDto {
            kind: LearningBlockKind::Explanation,
            title: "Title".into(),
            body: "An exact lesson statement.".into(),
            rubric: vec![],
            source_ids: vec![],
        }];
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
        let view = report.learner_view(&lesson, &[]).unwrap();
        assert_eq!(view.teaching_claims[0].content_quote, lesson.blocks[0].body);
        assert_eq!(view.teaching_claims[0].verdict, "supported");
        lesson.blocks[0].body = "Different lesson content.".into();
        assert!(report.learner_view(&lesson, &[]).is_err());
    }

    #[test]
    fn a_statement_reaches_the_learner_with_its_class_its_label_and_the_parts_it_was_read_as() {
        let (lesson, mut report) = fixture();
        let plain = report.learner_view(&lesson, &[]).unwrap();
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
        let claim = &report.learner_view(&lesson, &[]).unwrap().teaching_claims[0];

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
            let view = report.learner_view(&lesson, &[]).unwrap();
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

        let view = report.learner_view(&lesson, &[]).unwrap();

        assert_eq!(view.teaching_claims.len(), 1);
        assert_eq!(view.teaching_claims[0].class, None);
        assert_eq!(view.claim_count, 1);
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
        assert!(report.learner_view(&lesson, &[]).is_err());
        report.findings[0].evidence[0].retrieval_kind = "execution".into();
        let view = report.learner_view(&lesson, &[]).unwrap();
        assert_eq!(
            view.teaching_claims[0].passages[0].title,
            "Executed example"
        );
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

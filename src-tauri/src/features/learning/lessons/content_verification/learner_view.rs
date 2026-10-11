//! The private report owns its public projection. No assessment text is copied
//! into a learner DTO; malformed data is unavailable, never a checked claim.
use super::{ClaimVerdict, LessonVerificationReport};
use crate::features::learning::{
    dto::LearningLessonDto,
    lesson_evidence::{
        LearningClaimEvidenceDto, LearningEvidencePassageDto, LearningLessonEvidenceDto,
        SavedEvidenceVersion,
    },
};
use crate::shared::error::{AppError, Result};
use std::collections::{HashMap, HashSet};

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
                finding.unit >= lesson.blocks.len() + lesson.questions.len()
                    || finding.quote.trim().is_empty()
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
                        })
                    })
                    .collect::<Result<Vec<_>>>()?;
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
    use crate::features::learning::content_verification::{tests::attest, EvidencePassage};
    use crate::features::learning::dto::{
        LearningBlockDto, LearningBlockKind, PreparedLearningLesson,
    };

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

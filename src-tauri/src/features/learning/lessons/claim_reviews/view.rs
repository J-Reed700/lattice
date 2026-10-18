//! A lesson's claim review as the panel shows it (§7.3): the ledger, with
//! the unit and source titles each card names, the passages its latest check
//! compared, what research and repair did, the user's verdict, and the
//! actions the claim allows now.

use std::collections::HashMap;

use lattice_core::application::ports::claim_review::ClaimReviewStore;
use lattice_core::domain::learning::claim_review::{
    AttachOutcome, CheckResult, ClaimCounts, ClaimEvent, ClaimLedgerEntry, ClaimState,
    LessonReview, LessonReviewState,
};
use lattice_core::shared::error::{AppError, Result};
use serde_json::Value;

use super::dto::{
    ClaimEventDto, ClaimEvidencePassageDto, ClaimLocationDto, ClaimReviewItemDto, HumanVerdictDto,
    LessonClaimReviewDto, RepairAttemptDto, ResearchAttemptDto, SourceAddedByDto, SourceRefDto,
    StopReasonDto,
};
use super::repository::ShownPassage;
use super::service::ClaimReviewService;
use crate::features::learning::lesson_evidence::LearningEvidencePassageDto;
use crate::features::learning::persistence::db;
use crate::features::learning::repository::LearningRepository;
use crate::features::learning::source_library::LearningSourceLibraryRepository;

/// What a card says about a source version.
struct SourceFacts {
    title: String,
    url: Option<String>,
    added_by: SourceAddedByDto,
}

impl SourceFacts {
    fn reference(&self, source_version_id: &str) -> SourceRefDto {
        SourceRefDto {
            source_version_id: source_version_id.into(),
            title: self.title.clone(),
            url: self.url.clone(),
            added_by: self.added_by,
        }
    }
}

impl ClaimReviewService {
    /// The review of `lesson_id` in `program_id`; not found before the
    /// lesson's first verification pass.
    pub async fn review(&self, program_id: &str, lesson_id: &str) -> Result<LessonClaimReviewDto> {
        let ledger = self.store().load(lesson_id).await?;
        let review = ledger
            .review
            .clone()
            .filter(|review| review.program_id == program_id)
            .ok_or_else(|| AppError::NotFound("This lesson has no claim review yet.".into()))?;
        let sources = self.sources(program_id).await?;
        let titles = self.unit_titles(&review, program_id, lesson_id).await?;
        let shown = self.store().shown_passages(lesson_id).await?;
        let pass = self
            .drafts()
            .job(&review.job_id)
            .await
            .ok()
            .and_then(|job| job.activity)
            .map_or(0, |activity| activity.verification_pass);
        let claims = ledger
            .claims
            .iter()
            .map(|entry| {
                let passages = shown.get(entry.claim_id()).map_or(&[][..], Vec::as_slice);
                item(entry, passages, &sources, &titles)
            })
            .collect();
        Ok(LessonClaimReviewDto {
            lesson_id: ledger.lesson_id.clone(),
            program_id: review.program_id.clone(),
            state: review.state.into(),
            revision: ledger.revision.0,
            counts: ClaimCounts::tally(ledger.claims.iter()).into(),
            pass,
            stop_reason: review.stop_reason.as_ref().map(StopReasonDto::from),
            claims,
        })
    }

    /// Every version of the course's sources, deleted ones included: a card
    /// still names a source the user removed.
    async fn sources(&self, program_id: &str) -> Result<HashMap<String, SourceFacts>> {
        let mut connection = self.pool().acquire().await.map_err(db)?;
        let workspace =
            LearningSourceLibraryRepository::workspace_on(&mut connection, program_id, true)
                .await?;
        Ok(workspace
            .sources
            .into_iter()
            .flat_map(|source| {
                let added_by = source.added_by;
                source.versions.into_iter().map(move |version| {
                    (
                        version.id,
                        SourceFacts {
                            title: version.title,
                            url: version.resolved_url,
                            added_by,
                        },
                    )
                })
            })
            .collect())
    }

    /// The title of each unit, blocks then questions: from the draft that
    /// waits for review, or from the published lesson.
    async fn unit_titles(
        &self,
        review: &LessonReview,
        program_id: &str,
        lesson_id: &str,
    ) -> Result<Vec<String>> {
        if review.state != LessonReviewState::Ready {
            if let Some(draft) = self
                .drafts()
                .lesson_draft(&review.job_id, lesson_id)
                .await?
            {
                let candidate: Value = serde_json::from_str(&draft.candidate)
                    .map_err(|error| AppError::Serialization(error.to_string()))?;
                let titles = |field: &str, key: &str| -> Vec<String> {
                    candidate[field]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .map(|unit| unit[key].as_str().unwrap_or_default().to_owned())
                        .collect()
                };
                let mut units = titles("blocks", "title");
                units.extend(titles("questions", "prompt"));
                return Ok(units);
            }
        }
        let program = LearningRepository::new(self.pool().clone())
            .get(program_id)
            .await?;
        Ok(program
            .modules
            .iter()
            .flat_map(|module| &module.lessons)
            .find(|lesson| lesson.id == lesson_id)
            .map(|lesson| {
                let blocks = lesson.blocks.iter().map(|block| block.title.clone());
                let questions = lesson.questions.iter().map(|q| q.prompt.clone());
                blocks.chain(questions).collect()
            })
            .unwrap_or_default())
    }
}

/// One claim card.
fn item(
    entry: &ClaimLedgerEntry,
    shown: &[ShownPassage],
    sources: &HashMap<String, SourceFacts>,
    titles: &[String],
) -> ClaimReviewItemDto {
    let location = entry.location();
    let reference = |id: &String| sources.get(id).map(|facts| facts.reference(id));
    let mut research = Vec::new();
    let mut repairs = Vec::new();
    for record in entry.events() {
        match &record.event {
            ClaimEvent::Researched {
                queries,
                sources_added,
                passages_found,
            } => research.push(ResearchAttemptDto {
                queries: queries.clone(),
                sources_added: sources_added.iter().filter_map(reference).collect(),
                passages_found: *passages_found,
                at: record.provenance.at,
            }),
            ClaimEvent::RepairAttempted {
                attempt,
                escalated,
                before,
                after,
                outcome,
            } => repairs.push(RepairAttemptDto {
                attempt: *attempt,
                escalated: *escalated,
                before: before.clone(),
                after: after.clone(),
                outcome: outcome.into(),
                at: record.provenance.at,
            }),
            _ => {}
        }
    }
    let (reason, supporting_quote, human) = match entry.state() {
        ClaimState::Open { reason, .. } | ClaimState::Queued { reason, .. } => {
            (Some(reason.clone()), None, None)
        }
        ClaimState::ResolvedByEvidence {
            supporting_quote, ..
        } => (None, Some(supporting_quote.clone()), None),
        ClaimState::Confirmed { at, note, bound } => (
            failed_reason(entry),
            None,
            Some(HumanVerdictDto {
                at: *at,
                note: note.clone(),
                bound_sources: bound
                    .source_version_ids
                    .iter()
                    .filter_map(reference)
                    .collect(),
            }),
        ),
        _ => (failed_reason(entry), None, None),
    };
    ClaimReviewItemDto {
        claim_id: entry.claim_id().0.clone(),
        location: ClaimLocationDto {
            unit_index: location.unit_index,
            unit_kind: location.unit_kind.into(),
            unit_title: titles.get(location.unit_index).cloned().unwrap_or_default(),
            field: location.field.clone(),
            quote: location.quote.clone(),
            span: location.span,
        },
        statement: entry.statement().to_owned(),
        state: entry.state().into(),
        verdict_kind: entry.latest_failure().map(Into::into),
        reason,
        evidence: shown
            .iter()
            .map(|passage| {
                let facts = sources.get(&passage.source_version_id);
                ClaimEvidencePassageDto {
                    passage: LearningEvidencePassageDto {
                        source_version_id: passage.source_version_id.clone(),
                        title: facts.map(|facts| facts.title.clone()).unwrap_or_default(),
                        url: facts.and_then(|facts| facts.url.clone()),
                        text: passage.text.clone(),
                        start_byte: passage.start_byte,
                        end_byte: passage.end_byte,
                        retrieval_kind: passage.retrieval_kind.clone(),
                    },
                    added_by: facts.map(|facts| facts.added_by),
                }
            })
            .collect(),
        supporting_quote,
        research,
        repairs,
        // The ledger keeps whether an example ran, not its run; the run is
        // in the lesson's report, which a waiting lesson does not have yet.
        executions: Vec::new(),
        human,
        available_actions: entry
            .available_actions()
            .into_iter()
            .map(Into::into)
            .collect(),
        history: entry.events().iter().map(ClaimEventDto::from).collect(),
    }
}

/// The reason of the claim's latest failing check, kept after it closes.
fn failed_reason(entry: &ClaimLedgerEntry) -> Option<String> {
    entry.events().iter().rev().find_map(|record| {
        let check = match &record.event {
            ClaimEvent::Checked(check)
            | ClaimEvent::EvidenceAttached {
                result: AttachOutcome::Checked(check),
                ..
            } => check,
            _ => return None,
        };
        matches!(check.result, CheckResult::Failed { .. }).then(|| check.reason.clone())
    })
}

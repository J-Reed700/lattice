use std::collections::HashMap;

use lattice_core::application::ports::claim_review::ClaimReviewStore;
use lattice_core::domain::learning::claim_review::{
    ClaimId, Ledger, LessonReview, LessonReviewState, Revision, UserAction,
};
use lattice_core::domain::provenance::ClaimKind;
use lattice_core::shared::edits::{ContentEdits, Uncommitted};
use lattice_core::shared::error::{AppError, Result};

use super::super::checker_examples::LabelSource;
use super::super::published_undo;
use super::super::repository::{blocks, check_revision, revision_conflict};
use super::super::teaching_claims::name_claims;
use super::super::teaching_findings::{TeachingFindingDto, TeachingFindingsRepository};
use super::{add_kinds, batch_refusal, settle, user_resolution, ClaimReviewService, Planned};
use crate::edits::keys;

const NO_NOTES: &str = "Choose the teaching notes first.";
const ONLY_PUBLISHED: &str = "Only a published lesson's sentences can be taken out this way.";
const NOT_A_SENTENCE: &str =
    "This note isn't about one whole sentence of a section, so there's no sentence to remove.";

/// What `remove_note_sentences` did.
pub struct NoteSentencesRemoved {
    /// The review's revision after the removal.
    pub revision: i64,
    /// The claims the removal ended, in the order it ended them.
    pub claim_ids: Vec<String>,
}

impl ClaimReviewService {
    /// Takes the sentence of every teaching note in `finding_keys` out of its
    /// published section, all or nothing, under one revision. Returns the
    /// revision after and the claims the removals ended, in the order they
    /// ended: an Undo of them through `resolve_many` puts everything back.
    pub async fn remove_note_sentences(
        &self,
        lesson_id: &str,
        finding_keys: &[String],
        expected_revision: i64,
    ) -> Result<NoteSentencesRemoved> {
        let ledger = self.store.load(lesson_id).await?;
        let review = ledger
            .review
            .clone()
            .ok_or_else(|| AppError::NotFound("This lesson has no statement review.".into()))?;
        if ledger.revision != Revision(expected_revision) {
            return Err(revision_conflict());
        }
        if review.state != LessonReviewState::Ready {
            return Err(AppError::InvalidState(ONLY_PUBLISHED.into()));
        }
        let mut findings = TeachingFindingsRepository::new(self.database.clone())
            .list(lesson_id)
            .await?;
        let edits = crate::edits::content_edits(&self.database)?;
        let (bodies, newest) = {
            let mut read = self.database.begin_read().await?;
            let bodies = blocks::section_bodies_in(&mut read, lesson_id).await?;
            let refs: Vec<_> = bodies
                .keys()
                .map(|&unit| keys::section_ref(lesson_id, unit))
                .collect();
            let map: HashMap<usize, _> = edits
                .newest(&mut read.reading(), &refs, keys::CLAIMS_REACTOR)
                .await?
                .into_iter()
                .filter_map(|item| {
                    let unit = keys::section_key(&item.content.key)?.1;
                    Some((unit, item))
                })
                .collect();
            read.commit().await?;
            (bodies, map)
        };
        name_claims(&ledger, &bodies, &newest, &mut findings);
        let mut chosen: Vec<&TeachingFindingDto> = Vec::new();
        for key in finding_keys {
            let finding = findings
                .iter()
                .find(|finding| finding.finding_key == *key)
                .ok_or_else(|| {
                    AppError::NotFound("No teaching note with that key in this lesson.".into())
                })?;
            if !chosen
                .iter()
                .any(|seen| seen.finding_key == finding.finding_key)
            {
                chosen.push(finding);
            }
        }
        if chosen.is_empty() {
            return Err(AppError::InvalidInput(NO_NOTES.into()));
        }
        let refused = chosen.iter().filter(|finding| !finding.removable).count();
        if refused > 0 {
            return Err(batch_refusal(
                "removed",
                refused,
                chosen.len(),
                NOT_A_SENTENCE,
            ));
        }

        let (ended, moved, mut revision, uncommitted) = self
            .remove_sentences(&edits, &review, &ledger, &chosen)
            .await?;
        for pending in uncommitted {
            let _ = edits.committed(pending).await;
        }
        if self.redecide(lesson_id, moved).await? {
            revision = self.store.load(lesson_id).await?.revision;
        }
        Ok(NoteSentencesRemoved {
            revision: revision.0,
            claim_ids: ended.into_iter().map(|id| id.0).collect(),
        })
    }

    /// Takes the chosen notes' sentences out of the published text, ends the
    /// claims they stood on and settles what the removals give the checker
    /// examples and calibration, in one unit of work under the ledger's
    /// revision. Returns the claims it ended, the kinds whose bar moved,
    /// the revision it left, and pending edit follow-ups.
    async fn remove_sentences(
        &self,
        edits: &ContentEdits,
        review: &LessonReview,
        ledger: &Ledger,
        chosen: &[&TeachingFindingDto],
    ) -> Result<(Vec<ClaimId>, Vec<ClaimKind>, Revision, Vec<Uncommitted>)> {
        let lesson_id = ledger.lesson_id.as_str();
        let mut tx = self.database.begin_write().await?;
        check_revision(&mut tx, lesson_id, ledger.revision).await?;
        let (ended, through_claims, written, uncommitted) =
            published_undo::remove_note_sentences_in(&mut tx, edits, ledger, chosen).await?;
        let mut moved = Vec::new();
        for id in &through_claims {
            if let Some(entry) = ledger.claims.iter().find(|entry| entry.claim_id() == id) {
                let resolution =
                    user_resolution(UserAction::Remove, None, Some(LabelSource::UserRemove));
                let item = Planned { entry, resolution };
                let kinds = settle::settle_in(&mut tx, review, lesson_id, &item).await?;
                add_kinds(&mut moved, kinds);
            }
        }
        tx.commit().await?;
        Ok((ended, moved, written, uncommitted))
    }
}

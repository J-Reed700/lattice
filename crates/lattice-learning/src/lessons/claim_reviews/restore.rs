//! Putting back a sentence the machine took out, and restoring the original
//! of one it rewrote: the service's side of both. A draft takes the user's
//! event and the next pass writes the text; a published lesson changes its
//! text, ledger and report at once (`published_restore`).

use lattice_core::application::ports::claim_review::ClaimReviewStore;
use lattice_core::domain::learning::claim_review::{
    ClaimActionKind, ClaimEvent, ClaimId, ClaimLedgerEntry, LessonReview, LessonReviewState,
    Revision, UserAction,
};
use lattice_core::shared::error::{AppError, Result};
use serde_json::Value;

use super::actions::{in_section_body, ONLY_SECTION};
use super::checker_examples::LabelSource;
use super::published_restore::{self, Restore, Restoring};
use super::put_back::{self, Lesson};
use super::service::{provenance, ClaimReviewService, Resolution};
use super::trust_changed::forget_verdict;
use lattice_core::domain::learning::claim_review::TrustVerdict;

const BLANK: &str = "Write the sentence you want to put back.";
const NOT_TAKEN_OUT: &str = "Only a sentence the machine took out can be put back.";
const NOT_REWRITTEN: &str = "Only a sentence the machine rewrote has an original to restore.";

fn resolution(action: UserAction) -> Resolution {
    Resolution {
        event: ClaimEvent::UserAction { action, note: None },
        label: None,
        trust: None,
        forget: None,
    }
}

impl ClaimReviewService {
    /// The put back of `entry`'s sentence as the user asked: where it was or
    /// at the end of `section`, as it was or as `text` writes it.
    pub(super) async fn putting_back(
        &self,
        review: &LessonReview,
        lesson_id: &str,
        entry: &ClaimLedgerEntry,
        text: Option<String>,
        section: Option<u32>,
    ) -> Result<Resolution> {
        if !entry.allows(ClaimActionKind::PutBack) {
            return Err(AppError::InvalidState(NOT_TAKEN_OUT.into()));
        }
        let written = text.map(|text| text.trim().to_owned());
        if written.as_deref() == Some("") {
            return Err(AppError::InvalidInput(BLANK.into()));
        }
        let original = put_back::sentence_of(entry);
        let edited = written.as_deref().is_some_and(|text| text != original);
        let sentence = written.unwrap_or(original);
        let ledger = self.store().load(lesson_id).await?;
        let units = self.units(review, &review.program_id, lesson_id).await?;
        let lesson = Lesson {
            units: &units,
            published: review.state == LessonReviewState::Ready,
        };
        let place = put_back::resolve(&lesson, &ledger.claims, entry, section)?;
        Ok(resolution(UserAction::PutBack {
            sentence,
            edited,
            place,
        }))
    }

    /// The restore of the original of `entry`'s rewrite.
    pub(super) fn restoring_original(&self, entry: &ClaimLedgerEntry) -> Result<Resolution> {
        let (original, rewrite) = entry
            .rewrite()
            .filter(|_| entry.allows(ClaimActionKind::RestoreOriginal))
            .ok_or_else(|| AppError::InvalidState(NOT_REWRITTEN.into()))?;
        Ok(resolution(UserAction::RestoreOriginal {
            original: original.to_owned(),
            rewrite: rewrite.to_owned(),
        }))
    }

    /// Writes a put back or a restore into a published lesson's text,
    /// ledger and report; `None` for any other event.
    pub(super) async fn restored_in_published(
        &self,
        review: &LessonReview,
        lesson_id: &str,
        entry: &ClaimLedgerEntry,
        event: &ClaimEvent,
        expected_revision: Revision,
    ) -> Result<Option<Revision>> {
        let restoring = match event {
            ClaimEvent::UserAction {
                action: UserAction::PutBack { .. },
                ..
            } => Restoring::PutBack,
            ClaimEvent::UserAction {
                action: UserAction::RestoreOriginal { original, rewrite },
                ..
            } => Restoring::Original { original, rewrite },
            _ => return Ok(None),
        };
        if !in_section_body(entry) {
            return Err(AppError::InvalidInput(ONLY_SECTION.into()));
        }
        let revision = published_restore::apply(
            self.pool(),
            Restore {
                lesson_id,
                claim_id: entry.claim_id(),
                restoring,
                event: event.clone(),
                expected_revision,
                user: provenance(review),
            },
        )
        .await?;
        Ok(Some(revision))
    }

    /// Puts `claim_id`'s sentence back where it was, for a source that
    /// supports it. `None` when the place is not known: the user chooses it.
    pub(crate) async fn put_back_for_source(
        &self,
        lesson_id: &str,
        claim_id: &ClaimId,
    ) -> Result<Option<Revision>> {
        let ledger = self.store().load(lesson_id).await?;
        let Some(review) = ledger.review.clone() else {
            return Ok(None);
        };
        let published = review.state == LessonReviewState::Ready;
        let Some(entry) = ledger
            .claims
            .iter()
            .find(|entry| entry.claim_id() == claim_id)
            .filter(|entry| entry.allows(ClaimActionKind::PutBack))
            .filter(|entry| !published || in_section_body(entry))
        else {
            return Ok(None);
        };
        let resolution = match self
            .putting_back(&review, lesson_id, entry, None, None)
            .await
        {
            Ok(resolution) => resolution,
            Err(AppError::InvalidInput(message)) if message == put_back::CHOOSE => return Ok(None),
            Err(error) => return Err(error),
        };
        if published {
            return self
                .restored_in_published(
                    &review,
                    lesson_id,
                    entry,
                    &resolution.event,
                    ledger.revision,
                )
                .await;
        }
        let revision = self
            .store()
            .append(
                lesson_id,
                entry.claim_id(),
                provenance(&review),
                resolution.event,
                ledger.revision,
            )
            .await?;
        Ok(Some(revision))
    }

    /// Takes back the claim's last user event on a published lesson.
    pub(super) async fn undone_in_published(
        &self,
        review: &LessonReview,
        lesson_id: &str,
        entry: &ClaimLedgerEntry,
        expected_revision: Revision,
    ) -> Result<i64> {
        let edited = entry.undo_target().is_some_and(|ordinal| {
            entry.events().iter().any(|record| {
                record.ordinal == ordinal
                    && matches!(
                        record.event,
                        ClaimEvent::UserAction {
                            action: UserAction::Edit { .. },
                            ..
                        }
                    )
            })
        });
        let revision = published_restore::undo(
            self.pool(),
            lesson_id,
            entry.claim_id(),
            expected_revision,
            provenance(review),
        )
        .await?;
        if edited {
            self.examples
                .forget(&entry.claim_id().0, LabelSource::UserEdit)
                .await?;
            let redecided =
                forget_verdict(self.pool(), &entry.claim_id().0, TrustVerdict::Edit).await?;
            if redecided.lessons.iter().any(|lesson| lesson == lesson_id) {
                return Ok(self.store().load(lesson_id).await?.revision.0);
            }
        }
        Ok(revision.0)
    }

    /// Whether a pass already wrote the user's put back or restore into the
    /// draft.
    pub(super) async fn restore_applied(
        &self,
        review: &LessonReview,
        lesson_id: &str,
        entry: &ClaimLedgerEntry,
        event: &ClaimEvent,
    ) -> Result<bool> {
        let units: Value = self.units(review, &review.program_id, lesson_id).await?;
        let holds = |unit: usize, field: &str, text: &str| {
            crate::content_verification::field_text(&units, unit, field)
                .is_some_and(|body| body.contains(text))
        };
        Ok(match event {
            ClaimEvent::UserAction {
                action:
                    UserAction::PutBack {
                        sentence, place, ..
                    },
                ..
            } => holds(place.unit_index, &place.field, sentence),
            ClaimEvent::UserAction {
                action: UserAction::RestoreOriginal { original, .. },
                ..
            } => {
                let location = entry.location();
                holds(location.unit_index, &location.field, original)
            }
            _ => false,
        })
    }
}

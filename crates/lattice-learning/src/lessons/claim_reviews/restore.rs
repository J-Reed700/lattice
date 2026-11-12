//! Putting back a sentence the machine took out, and restoring the original
//! of one it rewrote: the service's side of both. A draft takes the user's
//! event and the next pass writes the text; a published lesson changes its
//! text, ledger and report at once (`published_restore`).

use lattice_core::application::ports::claim_review::ClaimReviewStore;
use lattice_core::domain::learning::claim_review::{
    Anchor, ClaimActionKind, ClaimEvent, ClaimId, ClaimLedgerEntry, LessonReview,
    LessonReviewState, PutPlace, Revision, UserAction,
};
use lattice_core::shared::error::{AppError, Result};
use serde_json::Value;

use super::actions::in_section_body;
use super::dto::ClaimActionRequestDto;
use super::put_back::{self, Lesson};
use super::service::{ClaimReviewService, Resolution};
use crate::content_verification::with_pending_put_backs;

const BLANK: &str = "Write the sentence you want to put back.";
pub(super) const NOT_TAKEN_OUT: &str = "Only a sentence the machine took out can be put back.";
const NOT_REWRITTEN: &str = "Only a sentence the machine rewrote has an original to restore.";

fn resolution(action: UserAction) -> Resolution {
    Resolution {
        event: ClaimEvent::UserAction { action, note: None },
        label: None,
        unlabel: None,
        trust: None,
        forget: None,
    }
}

impl ClaimReviewService {
    /// The lesson's text a put back is placed in: a published lesson's as it
    /// is, a draft's with the put backs the next pass has yet to write.
    pub(super) async fn put_back_units(
        &self,
        review: &LessonReview,
        lesson_id: &str,
        claims: &[ClaimLedgerEntry],
    ) -> Result<Value> {
        let units = self.units(review, &review.program_id, lesson_id).await?;
        Ok(if review.state == LessonReviewState::Ready {
            units
        } else {
            with_pending_put_backs(&units, claims)
        })
    }

    /// The put back of a claim whose sentence an earlier claim of the same
    /// batch brings back: nothing of its own to write.
    pub(super) fn put_back_with_sibling(entry: &ClaimLedgerEntry) -> Resolution {
        let location = entry.location();
        resolution(UserAction::PutBack {
            sentence: put_back::sentence_of(entry),
            edited: false,
            place: PutPlace {
                unit_index: location.unit_index,
                field: location.field.clone(),
                anchor: Anchor::End,
            },
        })
    }

    /// The put back of `entry`'s sentence as the user asked: where it was or
    /// at the end of `section`, as it was or as `text` writes it. In a
    /// batch `staged` is the text the put backs before it left.
    pub(super) async fn putting_back(
        &self,
        review: &LessonReview,
        lesson_id: &str,
        entry: &ClaimLedgerEntry,
        text: Option<String>,
        section: Option<u32>,
        staged: Option<&Value>,
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
        let loaded;
        let units = match staged {
            Some(units) => units,
            None => {
                loaded = self
                    .put_back_units(review, lesson_id, &ledger.claims)
                    .await?;
                &loaded
            }
        };
        let lesson = Lesson {
            units,
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
        let units = self
            .put_back_units(&review, lesson_id, &ledger.claims)
            .await?;
        let lesson = Lesson {
            units: &units,
            published,
        };
        if put_back::place(&lesson, &ledger.claims, entry).is_none() {
            return Ok(None);
        }
        let action = ClaimActionRequestDto::PutBack {
            text: None,
            section: None,
        };
        self.resolve(lesson_id, &claim_id.0, action, ledger.revision.0)
            .await
            .map(|revision| Some(Revision(revision)))
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

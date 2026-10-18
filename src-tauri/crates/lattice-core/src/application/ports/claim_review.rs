//! Where a lesson's claim ledger is kept.
//!
//! The learning feature implements this over the `learning_lesson_reviews`,
//! `learning_lesson_claims` and `learning_lesson_claim_events` tables; the
//! job and the claim commands both write through it. The domain validates
//! every event (`ClaimLedgerEntry::append`); the store adds the lesson-wide
//! revision that keeps a user action and the running job from overwriting
//! each other (§9).
//! Design: docs/design/2026-10-16-lesson-claim-review.md §3, §7, §9.

use async_trait::async_trait;

use crate::domain::learning::claim_review::{
    ClaimEvent, ClaimId, EventProvenance, Ledger, LessonReview, Revision,
};
use crate::shared::error::Result;

#[async_trait]
pub trait ClaimReviewStore: Send + Sync {
    /// The lesson's review row, its revision and every claim with its log,
    /// replayed. A lesson never verified under the ledger has no review and
    /// no claims.
    async fn load(&self, lesson_id: &str) -> Result<Ledger>;

    /// Appends one event to one claim in one transaction: the event row, the
    /// claim's replayed state and the review's revision. An `Extracted` event
    /// for an unknown claim id starts that claim. Refused with
    /// `AppError::InvalidState`, writing nothing, when `expected_revision` is
    /// not the review's current revision or the claim's state does not allow
    /// the event. Returns the new revision.
    async fn append(
        &self,
        lesson_id: &str,
        claim_id: &ClaimId,
        provenance: EventProvenance,
        event: ClaimEvent,
        expected_revision: Revision,
    ) -> Result<Revision>;

    /// Creates or replaces the lesson's review row and returns the new
    /// revision. Claims and their events are untouched.
    async fn set_lesson_review(&self, lesson_id: &str, review: LessonReview) -> Result<Revision>;
}

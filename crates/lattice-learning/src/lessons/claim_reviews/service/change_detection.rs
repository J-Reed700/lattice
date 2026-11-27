use lattice_core::application::ports::claim_review::ClaimReviewStore;
use lattice_core::domain::learning::claim_review::{Actor, ClaimEvent, Ledger, LessonReviewState};
use lattice_core::shared::error::Result;

use super::ClaimReviewService;

impl ClaimReviewService {
    /// Whether claim activity or a different checker gives verification new
    /// work since the lesson stopped.
    pub async fn changed_since_stop(&self, lesson_id: &str) -> Result<bool> {
        let ledger = self.store.load(lesson_id).await?;
        self.changed_in(&ledger).await
    }

    /// `changed_since_stop` over a loaded ledger. A different checker counts
    /// even when the user has not acted on a claim card.
    pub(in crate::lessons::claim_reviews) async fn changed_in(
        &self,
        ledger: &Ledger,
    ) -> Result<bool> {
        let Some(review) = &ledger.review else {
            return Ok(false);
        };
        if !matches!(
            review.state,
            LessonReviewState::NeedsReview | LessonReviewState::Stopped
        ) {
            return Ok(false);
        }
        if self
            .checker_model
            .as_ref()
            .is_some_and(|model| model != &review.checker_model)
        {
            return Ok(true);
        }
        let stopped_at = self
            .store
            .review_written_at(&ledger.lesson_id)
            .await?
            .unwrap_or(i64::MAX);
        Ok(changed_since(ledger, stopped_at))
    }
}

/// Whether the user acted on any claim at or after `at` (§6.2), a trust
/// change re-decided one (source provenance §18.7: an override or a moved
/// bar is the user's doing too), or a claim was checked outside the
/// preparation job (against a source the user added or attached, or by an
/// audit the user ran).
pub fn changed_since(ledger: &Ledger, at: i64) -> bool {
    ledger.claims.iter().any(|entry| {
        entry.events().iter().any(|record| {
            let changed = record.provenance.actor == Actor::User
                || matches!(
                    record.event,
                    ClaimEvent::StandingAssessed { cause: Some(_), .. }
                )
                || (matches!(record.event, ClaimEvent::Checked(_))
                    && record.provenance.job_id.is_none());
            changed && record.provenance.at >= at
        })
    })
}

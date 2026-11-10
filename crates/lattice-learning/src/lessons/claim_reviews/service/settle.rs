//! What a written claim gives the checker examples and trust calibration.

use super::{add_kinds, ClaimReviewService, Planned};
use crate::lessons::claim_reviews::checker_examples::CheckerExamples;
use crate::lessons::claim_reviews::repository::shown_passages_in;
use crate::lessons::claim_reviews::trust_changed::redecide_kinds;
use crate::references::trust::outcomes::{self, VerdictScope};
use lattice_core::domain::learning::claim_review::LessonReview;
use lattice_core::domain::provenance::ClaimKind;
use lattice_core::shared::error::Result;
use lattice_core::shared::persistence::WriteTx;

impl ClaimReviewService {
    /// What a written claim gives the checker examples and trust
    /// calibration, on the unit of work that wrote it. Returns the kinds
    /// whose bar the outcomes moved: [`Self::redecide`] re-decides them
    /// after the commit.
    pub(super) async fn settle_in(
        &self,
        tx: &mut WriteTx,
        review: &LessonReview,
        lesson_id: &str,
        item: &Planned<'_>,
    ) -> Result<Vec<ClaimKind>> {
        let Planned { entry, resolution } = item;
        if let Some(source) = resolution.unlabel {
            CheckerExamples::forget_in(tx, &entry.claim_id().0, source).await?;
        }
        if let Some(label) = resolution.label {
            let shown = shown_passages_in(tx, lesson_id).await?;
            let passages = shown.get(entry.claim_id()).cloned().unwrap_or_default();
            CheckerExamples::record_human_verdict_in(
                tx,
                &review.program_id,
                lesson_id,
                entry,
                &passages,
                label,
            )
            .await?;
        }
        let scope = VerdictScope {
            program_id: &review.program_id,
            lesson_id,
        };
        let mut moved = Vec::new();
        if let Some(verdict) = resolution.trust {
            add_kinds(
                &mut moved,
                outcomes::record_in(tx, scope, entry, verdict).await?,
            );
        }
        if let Some(verdict) = resolution.forget {
            add_kinds(
                &mut moved,
                outcomes::forget_in(tx, &entry.claim_id().0, verdict).await?,
            );
        }
        Ok(moved)
    }

    /// Re-decides the claims of every kind whose bar a settlement moved;
    /// whether that re-decided one of this lesson's claims.
    pub(super) async fn redecide(&self, lesson_id: &str, moved: Vec<ClaimKind>) -> Result<bool> {
        let redecided = redecide_kinds(&self.database, moved).await?;
        Ok(redecided.lessons.iter().any(|lesson| lesson == lesson_id))
    }
}

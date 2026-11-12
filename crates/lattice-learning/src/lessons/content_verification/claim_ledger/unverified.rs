//! Statements the machine did all it could for and could not settle: the
//! lesson publishes with them, marked not verified, instead of waiting for a
//! claim-by-claim review (design 2026-11-06, a lesson publishes what
//! verification could not settle).

use super::super::round::Unsettled;
use super::*;
use lattice_core::domain::learning::acceptance::{End, Then};
use lattice_core::domain::learning::claim_review::UnverifiedReason;

/// What the claim's last judged reading found; a claim no reading judged was
/// not checked.
fn last_reading(entry: &ClaimLedgerEntry) -> UnverifiedReason {
    entry
        .latest_failure()
        .map_or(UnverifiedReason::Unchecked, UnverifiedReason::of)
}

impl ClaimLedger {
    /// The statements nobody has settled: not verified, not confirmed and not
    /// gone. A code example's claim is none of them: its run is no sentence,
    /// and one that did not complete stays an issue until the user confirms it.
    fn unsettled(&self) -> Vec<ClaimId> {
        self.entries
            .iter()
            .filter(|entry| !is_example(entry) && !entry.state().is_resolved())
            .map(|entry| entry.claim_id().clone())
            .collect()
    }

    /// Whether the Verifier disputes a claim the user has not resolved: a
    /// lesson held for that is the course's own stricter policy.
    fn disputed(&self) -> bool {
        self.entries
            .iter()
            .any(|entry| entry.state().verifier_disputed())
    }

    /// The claims the machine ended whose sentences `candidate` still holds.
    fn ended_standing(&self, candidate: &Value) -> Vec<ClaimId> {
        self.standing_ended(candidate)
            .into_iter()
            .map(|entry| entry.claim_id().clone())
            .collect()
    }

    /// Publishes `claims` not verified, each for `reason`, or for what its
    /// last reading found when none is given; a claim whose state refuses the
    /// event (the user acted first) is left as it is. How many were marked.
    async fn publish_unverified(
        &mut self,
        claims: &[ClaimId],
        reason: Option<UnverifiedReason>,
    ) -> Result<usize> {
        let mut marked = 0;
        for id in claims {
            let Some(entry) = self.entry(id).filter(|entry| !is_example(entry)) else {
                continue;
            };
            let reason = reason.unwrap_or_else(|| last_reading(entry));
            if self
                .append(id, ClaimEvent::PublishedUnverified { reason })
                .await?
            {
                marked += 1;
            }
        }
        Ok(marked)
    }

    /// A step ended `id` at `End::Unverified`: it is published not verified,
    /// for what its last reading found.
    pub(in crate::lessons::content_verification) async fn publish_if_unverified(
        &mut self,
        id: &ClaimId,
        then: &Then,
    ) -> Result<()> {
        if *then == Then::End(End::Unverified) {
            self.publish_unverified(std::slice::from_ref(id), None)
                .await?;
        }
        Ok(())
    }

    /// Publishes not verified what a round left unsettled (§6.1): the claims
    /// it waited on or could not read; those waiting for the user, unless the
    /// Verifier's dispute holds the lesson; the machine's ended claims whose
    /// sentences `candidate` still holds. How many it published.
    pub(in crate::lessons::content_verification) async fn publish_unsettled(
        &mut self,
        unsettled: Unsettled,
        candidate: &Value,
    ) -> Result<usize> {
        let (claims, reason) = match unsettled {
            Unsettled::Waiting(claims) | Unsettled::Unreadable(claims) => {
                (claims, Some(UnverifiedReason::Unchecked))
            }
            Unsettled::AwaitingUser if self.disputed() => return Ok(0),
            Unsettled::AwaitingUser => (self.unsettled(), Some(UnverifiedReason::Unchecked)),
            Unsettled::Ended => (self.ended_standing(candidate), None),
        };
        self.publish_unverified(&claims, reason).await
    }
}

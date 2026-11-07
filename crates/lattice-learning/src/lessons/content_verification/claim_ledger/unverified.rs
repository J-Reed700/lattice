//! Statements the machine did all it could for and could not settle: the
//! lesson publishes with them, marked not verified, instead of waiting for a
//! claim-by-claim review (design 2026-11-06, a lesson publishes what
//! verification could not settle).

use super::stops::standing_ended;
use super::*;
use lattice_core::domain::learning::claim_review::{ReopenCause, UnverifiedReason, VerdictKind};

/// What the claim's last reading found: a contradiction, or else that the
/// sources do not state it.
fn last_reading(entry: &ClaimLedgerEntry) -> UnverifiedReason {
    match entry.latest_failure() {
        Some(kind) if kind.is_contradiction() => UnverifiedReason::Contradicted,
        _ => UnverifiedReason::Unsupported,
    }
}

impl ClaimLedger {
    /// The statements nobody has settled: not verified, not confirmed and not
    /// gone. A code example's claim is none of them: its run is no sentence,
    /// and one that did not complete stays an issue until the user confirms it.
    pub(in crate::lessons::content_verification) fn unsettled(
        &self,
    ) -> Vec<ClaimId> {
        self.entries
            .iter()
            .filter(|entry| !is_example(entry) && !entry.state().is_resolved())
            .map(|entry| entry.claim_id().clone())
            .collect()
    }

    /// Whether the Verifier disputes a claim the user has not resolved: a
    /// lesson held for that is the course's own stricter policy.
    pub(in crate::lessons::content_verification) fn disputed(&self) -> bool {
        self.entries.iter().any(|entry| {
            matches!(
                entry.state(),
                ClaimState::Open {
                    kind: VerdictKind::Disputed { .. },
                    ..
                } | ClaimState::Queued {
                    kind: VerdictKind::Disputed { .. },
                    ..
                } | ClaimState::Reopened {
                    cause: ReopenCause::VerifierDispute,
                    ..
                }
            )
        })
    }

    /// The claims the machine ended whose sentences `candidate` still holds.
    pub(in crate::lessons::content_verification) fn ended_standing(
        &self,
        candidate: &Value,
    ) -> Vec<ClaimId> {
        standing_ended(&self.entries, &self.cuts, candidate)
            .into_iter()
            .map(|entry| entry.claim_id().clone())
            .collect()
    }

    /// Publishes `claims` not verified, each for `reason`, or for what its
    /// last reading found when none is given; a claim whose state refuses the
    /// event (the user acted first) is left as it is. How many were marked.
    pub(in crate::lessons::content_verification) async fn publish_unverified(
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

    /// Drops the repairs waiting for these claims: they are published not
    /// verified instead, and nothing is charged.
    pub(in crate::lessons::content_verification) async fn drop_repairs(
        &mut self,
        claims: &[ClaimId],
    ) -> Result<()> {
        self.pending
            .repairs
            .retain(|record| !claims.contains(&record.claim_id));
        self.pending.repairs.retain(|record| {
            !record
                .rides_on
                .as_ref()
                .is_some_and(|target| claims.contains(target))
        });
        self.save_pending().await
    }
}

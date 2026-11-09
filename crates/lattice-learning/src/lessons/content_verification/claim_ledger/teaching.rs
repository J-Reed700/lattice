//! The teaching review after a claim repair or a user edit may repair the
//! sentence of a machine's claim that has attempts left (claim sources design
//! §10.3).

use super::*;
use lattice_core::domain::learning::acceptance::AcceptancePolicy;
#[cfg(test)]
use lattice_core::domain::learning::claim_review::{Actor, ClaimEvent};

impl ClaimLedger {
    /// Records `event` as the user's, which the machine's `append` cannot.
    #[cfg(test)]
    pub(in crate::lessons::content_verification) fn record_by_user(
        &mut self,
        claim_id: &ClaimId,
        event: ClaimEvent,
    ) -> Result<bool> {
        let provenance = EventProvenance {
            actor: Actor::User,
            ..self.provenance()
        };
        let Some(entry) = self.probe(claim_id, &provenance, &event)? else {
            return Ok(false);
        };
        if let Some(existing) = self
            .entries
            .iter_mut()
            .find(|existing| existing.claim_id() == claim_id)
        {
            *existing = entry;
        }
        Ok(true)
    }

    /// The claims, not among `targets`, whose sentences the teaching review
    /// may repair.
    #[allow(dead_code)]
    pub(in crate::lessons::content_verification) fn releasable(
        &self,
        _policy: &dyn AcceptancePolicy,
        _targets: &[ClaimId],
    ) -> Vec<ClaimId> {
        Vec::new()
    }

    /// Records that the teaching review may rewrite the sentences of `claims`.
    #[allow(dead_code)]
    pub(in crate::lessons::content_verification) async fn begin_teaching(
        &mut self,
        _candidate: &Value,
        _claims: &[ClaimId],
    ) -> Result<()> {
        Ok(())
    }
}

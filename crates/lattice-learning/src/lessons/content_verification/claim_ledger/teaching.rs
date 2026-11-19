//! The teaching review after a claim repair or a user edit may repair the
//! sentence of a machine's claim that has rewrites left (claim sources design
//! §10.3). The claim whose sentence it changed continues as a machine successor
//! that is read again after a rewrite, so each such change is one of the
//! rewrites its class graph gives it, and spends one of the claim's position.

use super::*;
use lattice_core::domain::learning::acceptance::{
    next_rewrite, repair_attempt, AcceptancePolicy, Position,
};
use lattice_core::domain::learning::claim_review::{Actor, ClaimEvent, ClaimState};

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

    /// The claims, not among `targets`, whose sentences the teaching review may
    /// repair: statements the machine holds, verified, passed as common
    /// knowledge or published not verified (all the machine's verdicts), with
    /// no evidence the user attached, no edit of the user's in their lineage, no
    /// repair waiting and a rewrite left (`teaching_successor`).
    pub(in crate::lessons::content_verification) fn releasable(
        &self,
        policy: &dyn AcceptancePolicy,
        targets: &[ClaimId],
    ) -> Vec<ClaimId> {
        self.entries
            .iter()
            .filter(|entry| self.is_statement(entry.claim_id()))
            .filter(|entry| !targets.contains(entry.claim_id()))
            .filter(|entry| !self.edited_now.contains(entry.claim_id()))
            .filter(|entry| {
                matches!(
                    entry.state(),
                    ClaimState::Unchecked
                        | ClaimState::Open { .. }
                        | ClaimState::Consistent { .. }
                        | ClaimState::ResolvedByEvidence { .. }
                        | ClaimState::GeneralKnowledge { .. }
                        | ClaimState::Unverified { .. }
                )
            })
            .filter(|entry| !entry.location().quote.trim().is_empty())
            .filter(|entry| {
                !entry
                    .events()
                    .iter()
                    .any(|record| matches!(record.event, ClaimEvent::EvidenceAttached { .. }))
            })
            .filter(|entry| {
                !self
                    .pending
                    .repairs
                    .iter()
                    .any(|record| record.claim_id == *entry.claim_id())
            })
            .filter(|entry| self.teaching_successor(policy, entry.claim_id()).is_some())
            .map(|entry| entry.claim_id().clone())
            .collect()
    }

    /// Records that the teaching review may rewrite the sentences of `claims`:
    /// a claim whose sentence it changes continues as a successor read again
    /// after a rewrite, and one whose sentence it leaves costs nothing.
    pub(in crate::lessons::content_verification) async fn begin_teaching(
        &mut self,
        policy: &dyn AcceptancePolicy,
        candidate: &Value,
        claims: &[ClaimId],
    ) -> Result<()> {
        if claims.is_empty() {
            return Ok(());
        }
        let records: Vec<RepairRecord> = claims
            .iter()
            .filter_map(|id| {
                let entry = self.entry(id)?;
                let last = self.stood_last(policy, id)?;
                let attempt = repair_attempt(policy, last);
                Some(RepairRecord {
                    claim_id: id.clone(),
                    unit: entry.location().unit_index,
                    attempt,
                    escalated: false,
                    before: entry.location().quote.clone(),
                    rides_on: Some(id.clone()),
                    step: None,
                    narrowing: false,
                })
            })
            .collect();
        self.pending.repairs.extend(records);
        self.pending
            .previous
            .get_or_insert_with(|| candidate.clone());
        self.save_pending().await
    }

    /// Where a successor of `id` would begin if the teaching review rewrote
    /// its sentence: none when the lineage holds the user's edit or no rewrite
    /// is left. Nothing it holds turns back from false.
    pub(super) fn teaching_successor(
        &self,
        policy: &dyn AcceptancePolicy,
        id: &ClaimId,
    ) -> Option<Position> {
        if self.holds_user_edit(id) {
            return None;
        }
        next_rewrite(policy, self.stood_last(policy, id)?)
    }

    /// Whether the lineage of `id` holds the user's edit, whose text the
    /// machine does not rewrite again.
    fn holds_user_edit(&self, id: &ClaimId) -> bool {
        let mut here = self.entry(id);
        for _ in 0..=self.entries.len() {
            let Some(entry) = here else {
                return false;
            };
            let edited = self.entries.iter().any(|other| {
                other.events().iter().any(|record| {
                    matches!(
                        &record.event,
                        ClaimEvent::Superseded { successor: Some(next), by: Actor::User }
                            if next == entry.claim_id()
                    )
                })
            });
            if edited {
                return true;
            }
            here = self.predecessor(entry.claim_id());
        }
        true
    }
}

//! The teaching review after a claim repair or a user edit may repair the
//! sentence of a machine's claim that has rewrites left (claim sources design
//! §10.3). The claim whose sentence it changed continues as a machine successor
//! that is read again after a rewrite, so each such change is one of the
//! rewrites its class graph gives it.

use super::*;
use lattice_core::domain::learning::acceptance::{AcceptancePolicy, Outcome, Step};
use lattice_core::domain::learning::claim_review::{Actor, ClaimEvent, ClaimState};

/// The rewrites a class graph gives a statement before its cut: a rewrite and
/// its retry. A teaching repair of a sentence is one of them.
const REWRITES_A_CLAIM_GETS: usize = 2;

impl ClaimLedger {
    /// The claims, not among `targets`, whose sentences the teaching review may
    /// repair: statements the machine holds or verified, with no evidence the
    /// user attached, no edit of the user's in their lineage, no repair waiting
    /// and a rewrite left.
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
            .filter(|entry| self.position(policy, entry.claim_id()).is_some())
            .filter(|entry| {
                self.rewrites_used(entry.claim_id())
                    .is_some_and(|used| used < REWRITES_A_CLAIM_GETS)
            })
            .map(|entry| entry.claim_id().clone())
            .collect()
    }

    /// Records that the teaching review may rewrite the sentences of `claims`:
    /// a claim whose sentence it changes continues as a successor read again
    /// after a rewrite, and one whose sentence it leaves costs nothing.
    pub(in crate::lessons::content_verification) async fn begin_teaching(
        &mut self,
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
                let used = self.rewrites_used(id)?;
                Some(RepairRecord {
                    claim_id: id.clone(),
                    unit: entry.location().unit_index,
                    attempt: u8::try_from(used + 1).unwrap_or(u8::MAX),
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

    /// The rewrites the sentence of `id` has had: those its claim took by the
    /// policy and each machine successor on the way. None when the lineage
    /// holds the user's edit, whose text the machine does not rewrite again.
    fn rewrites_used(&self, id: &ClaimId) -> Option<usize> {
        let mut used = 0;
        let mut here = self.entry(id);
        for _ in 0..=self.entries.len() {
            let Some(entry) = here else {
                return Some(used);
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
                return None;
            }
            used += entry
                .events()
                .iter()
                .filter(|record| {
                    matches!(
                        record.event,
                        ClaimEvent::Stepped {
                            step: Step::Rewrite(_),
                            outcome: Outcome::Rewritten,
                            ..
                        }
                    )
                })
                .count();
            here = self.predecessor(entry.claim_id());
            if here.is_some() {
                used += 1;
            }
        }
        None
    }
}

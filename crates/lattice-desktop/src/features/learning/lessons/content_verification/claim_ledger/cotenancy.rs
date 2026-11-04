//! Which statements stand on one sentence: a rewrite of the sentence changes
//! them all (claim classes design §3.2).

use super::{is_gone, stand_together, ClaimLedger, Place};
use lattice_core::domain::learning::claim_review::ClaimId;

/// The group each of `places` stands in with the others on its sentence: an
/// id the statements of one group share, none for one alone. Two stand
/// together directly or through a third.
pub(in crate::features::learning::lessons::content_verification) fn sentences(
    places: &[Place<'_>],
) -> Vec<Option<u32>> {
    let mut group: Vec<usize> = (0..places.len()).collect();
    for (at, here) in places.iter().enumerate() {
        for (other, there) in places.iter().enumerate().skip(at + 1) {
            if !stand_together(here, there) {
                continue;
            }
            if let (Some(&to), Some(&from)) = (group.get(at), group.get(other)) {
                group
                    .iter_mut()
                    .filter(|label| **label == from)
                    .for_each(|label| *label = to);
            }
        }
    }
    group
        .iter()
        .map(|&label| {
            let shared = group.iter().filter(|&&other| other == label).count() > 1;
            u32::try_from(label).ok().filter(|_| shared)
        })
        .collect()
}

impl ClaimLedger {
    /// The live claims standing on the same sentence as `claim_id`: those in
    /// its unit and field whose quote holds its quote or is held by it. A
    /// rewrite of the sentence changes them all (claim classes design §3.2).
    pub(in crate::features::learning::lessons::content_verification) fn cotenants(
        &self,
        claim_id: &ClaimId,
    ) -> Vec<ClaimId> {
        let Some(entry) = self.entry(claim_id) else {
            return Vec::new();
        };
        let here = Place::of(entry.location());
        self.entries
            .iter()
            .filter(|other| other.claim_id() != claim_id && !is_gone(other.state()))
            .filter(|other| stand_together(&here, &Place::of(other.location())))
            .map(|other| other.claim_id().clone())
            .collect()
    }

    /// The statements of the claims still standing, each with its section,
    /// but `claims`: what the lesson keeps of its sentences when these claims
    /// lose theirs.
    pub(in crate::features::learning::lessons::content_verification) fn live_statements_but(
        &self,
        claims: &[ClaimId],
    ) -> Vec<(usize, String)> {
        self.entries
            .iter()
            .filter(|entry| !is_gone(entry.state()) && !claims.contains(entry.claim_id()))
            .map(|entry| (entry.location().unit_index, entry.statement().to_owned()))
            .collect()
    }

    /// `claims` and the claims standing on their sentences.
    pub(in crate::features::learning::lessons::content_verification) fn with_cotenants(
        &self,
        claims: &[ClaimId],
    ) -> Vec<ClaimId> {
        let mut all = claims.to_vec();
        for id in claims {
            for mate in self.cotenants(id) {
                if !all.contains(&mate) {
                    all.push(mate);
                }
            }
        }
        all
    }
}

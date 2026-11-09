//! The teaching review after a claim repair or a user edit may repair the
//! sentence of a machine's claim that has attempts left (claim sources design
//! §10.3).

use super::*;
use lattice_core::domain::learning::acceptance::AcceptancePolicy;

impl ClaimLedger {
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

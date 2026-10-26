//! Old held lessons must be able to reach the current verification rules.
//! Reading a review only detects the work; Resume performs it in the job.

use super::service::ClaimReviewService;
use crate::features::learning::references::trust::{
    source_view::latest_standing, SqliteSourceTrustStore,
};
use lattice_core::application::ports::source_trust::SourceTrustStore;
use lattice_core::domain::learning::claim_review::{Ledger, VerdictKind};
use lattice_core::domain::provenance::TRUST_POLICY_VERSION;
use lattice_core::shared::error::Result;
use std::collections::BTreeSet;

impl ClaimReviewService {
    pub(super) async fn stale_standing(&self, ledger: &Ledger) -> Result<bool> {
        let Some(review) = &ledger.review else {
            return Ok(false);
        };
        let mut versions = BTreeSet::new();
        for entry in &ledger.claims {
            if !matches!(
                entry.state().verdict(),
                Some(VerdictKind::InsufficientStanding { .. })
            ) {
                continue;
            }
            let Some(trust) = entry.trust() else {
                continue;
            };
            if trust.available.is_none() || trust.policy != TRUST_POLICY_VERSION {
                return Ok(true);
            }
            versions.extend(
                trust
                    .supports
                    .iter()
                    .map(|support| &support.source_version_id),
            );
        }
        let store = SqliteSourceTrustStore::new(self.pool().clone());
        for version in versions {
            // A source with no captured provenance cannot acquire a standing
            // judgement by rerunning. Do not enable an endless retry for it.
            if let Some(record) = store.provenance(version).await? {
                if latest_standing(self.pool(), &review.program_id, &record)
                    .await?
                    .is_none()
                {
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }
}

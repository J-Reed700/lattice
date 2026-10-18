//! The user's site overrides as Settings lists them (§13, §19), each with
//! what the claims under it did since. Setting and clearing an override
//! re-decides claims, so that work is the claim ledger's
//! (`claim_reviews::trust_overrides`).

use super::dto_map;
use super::repository::SqliteSourceTrustStore;
use crate::features::learning::dto::{LearningSiteTrustOverrideDto, LearningTrackRecordDto};
use crate::features::learning::persistence::db;
use lattice_core::application::ports::SourceTrustStore;
use lattice_core::domain::provenance::{OriginKey, TrustOverride};
use lattice_core::shared::error::Result;

/// Every site and site-for-subject override, each with the track record of
/// the site's origins: the user's verdicts on claims its pages supported.
pub async fn list(store: &SqliteSourceTrustStore) -> Result<Vec<LearningSiteTrustOverrideDto>> {
    let mut listed = Vec::new();
    for entry in store.site_overrides().await? {
        let site = match &entry {
            TrustOverride::Site {
                registrable_domain, ..
            }
            | TrustOverride::SiteSubject {
                registrable_domain, ..
            } => registrable_domain.clone(),
            TrustOverride::Source { .. } => continue,
        };
        let origins: Vec<String> =
            sqlx::query_scalar("SELECT DISTINCT origin_key FROM source_provenance WHERE site=?")
                .bind(&site)
                .fetch_all(store.pool())
                .await
                .map_err(db)?;
        let mut record = LearningTrackRecordDto { right: 0, wrong: 0 };
        for origin in origins {
            let track = store.track_record(&OriginKey(origin)).await?;
            record.right = record.right.saturating_add(track.right);
            record.wrong = record.wrong.saturating_add(track.wrong);
        }
        listed.push(LearningSiteTrustOverrideDto {
            trust_override: dto_map::trust_override(&entry),
            track_record: record,
        });
    }
    Ok(listed)
}

//! The user's site overrides as Settings lists them (§13, §19), each with
//! what the claims under it did since. Setting and clearing an override
//! re-decides claims, so that work is the claim ledger's
//! (`claim_reviews::trust_overrides`).

use super::dto_map;
use super::repository::SqliteSourceTrustStore;
use crate::features::learning::dto::{LearningSiteTrustOverrideDto, LearningTrackRecordDto};
use crate::features::learning::persistence::db;
use lattice_core::application::ports::SourceTrustStore;
use lattice_core::domain::provenance::TrustOverride;
use lattice_core::shared::error::{AppError, Result};
use std::collections::HashMap;

fn site_of(entry: &TrustOverride) -> Option<&str> {
    match entry {
        TrustOverride::Site {
            registrable_domain, ..
        }
        | TrustOverride::SiteSubject {
            registrable_domain, ..
        } => Some(registrable_domain),
        TrustOverride::Source { .. } => None,
    }
}

/// Every site and site-for-subject override, each with the track record of
/// the site's origins: the user's verdicts on claims its pages supported.
pub async fn list(store: &SqliteSourceTrustStore) -> Result<Vec<LearningSiteTrustOverrideDto>> {
    let entries = store.site_overrides().await?;
    let sites: Vec<_> = entries.iter().filter_map(site_of).collect();
    if sites.is_empty() {
        return Ok(Vec::new());
    }
    // A site can contain many snapshots from the same origin. Deduplicate
    // origins before joining outcomes so their history is counted only once.
    let rows: Vec<(String, i64, i64)> = sqlx::query_as(
        "SELECT p.site, SUM(o.decision_sufficient=o.decision_right),
                        SUM(o.decision_sufficient<>o.decision_right)
         FROM (SELECT DISTINCT site,origin_key FROM source_provenance
               WHERE site IN (SELECT value FROM json_each(?))) p
         JOIN source_trust_outcomes o ON o.origin_key=p.origin_key
         GROUP BY p.site,p.origin_key",
    )
    .bind(serde_json::to_string(&sites)?)
    .fetch_all(store.pool())
    .await
    .map_err(db)?;
    let mut records: HashMap<String, (u32, u32)> = HashMap::new();
    for (site, held, failed) in rows {
        let count = |value| {
            u32::try_from(value)
                .map_err(|_| AppError::Database("Track record count is out of range".into()))
        };
        let record = records.entry(site).or_default();
        record.0 = record.0.saturating_add(count(held)?);
        record.1 = record.1.saturating_add(count(failed)?);
    }
    let mut listed = Vec::new();
    for entry in entries {
        let Some(site) = site_of(&entry) else {
            continue;
        };
        let (right, wrong) = records.get(site).copied().unwrap_or_default();
        listed.push(LearningSiteTrustOverrideDto {
            trust_override: dto_map::trust_override(&entry),
            track_record: LearningTrackRecordDto { right, wrong },
        });
    }
    Ok(listed)
}

//! The override commands' work (§16, §19, decision 4): set or clear one
//! override and report what it sends back to be re-decided, and list the
//! user's site overrides with what the claims under them did since.
//!
//! INTEGRATION (P5/P7): after a change, the ledger reopens the waiting
//! claims that cite `change.versions` (`ReopenCause::TrustChanged`) and the
//! gate re-decides them without a model call. P5 owns that reopen; call it
//! from `set` and `clear` with the `TrustChange` once it lands.

use super::dto_map;
use super::overrides::{self, TrustChange};
use super::repository::{unix_seconds, SqliteSourceTrustStore};
use super::waiting_claims::impact;
use crate::features::learning::dto::{
    LearningSiteTrustOverrideDto, LearningTrackRecordDto, LearningTrustChangeDto,
    LearningTrustChangeResultDto, LearningTrustOverrideInputDto, LearningTrustOverrideScopeDto,
};
use crate::features::learning::persistence::db;
use lattice_core::application::ports::SourceTrustStore;
use lattice_core::domain::provenance::{OriginKey, TrustOverride};
use lattice_core::shared::error::Result;

async fn result(
    store: &SqliteSourceTrustStore,
    change: TrustChange,
) -> Result<LearningTrustChangeResultDto> {
    let impact = impact(store.pool(), &change.versions).await?;
    Ok(LearningTrustChangeResultDto {
        change: LearningTrustChangeDto {
            scope: dto_map::scope(&change.scope),
            source_version_ids: change
                .versions
                .into_iter()
                .map(|version| version.source_version_id)
                .collect(),
        },
        impact,
    })
}

/// Creates or replaces the override at its scope.
pub async fn set(
    store: &SqliteSourceTrustStore,
    input: &LearningTrustOverrideInputDto,
) -> Result<LearningTrustChangeResultDto> {
    let entry = dto_map::override_input(input, unix_seconds())?;
    let change = overrides::set(store, &entry).await?;
    result(store, change).await
}

/// Removes the override at `scope`; `None` when there was none.
pub async fn clear(
    store: &SqliteSourceTrustStore,
    scope: &LearningTrustOverrideScopeDto,
) -> Result<Option<LearningTrustChangeResultDto>> {
    match overrides::clear(store, &dto_map::core_scope(scope)).await? {
        Some(change) => Ok(Some(result(store, change).await?)),
        None => Ok(None),
    }
}

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

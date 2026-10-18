//! The claims an override change sends back to be re-decided (§18.7, §19):
//! claims waiting in a lesson review (open or queued) whose shown evidence
//! cites a version the override covers. The override menu states the count
//! before the user commits ("3 claims in 2 lessons cite this source; they
//! will be re-decided"), and the change reports it after.
//!
//! Read-only over the claim ledger's tables; the reopen itself is the
//! ledger's (`ReopenCause::TrustChanged`).

use super::overrides::CoveredVersion;
use crate::features::learning::dto::LearningTrustImpactDto;
use crate::features::learning::persistence::db;
use lattice_core::shared::error::Result;
use serde_json::json;
use sqlx::{Row, SqlitePool};

/// Ledger states in which a claim is still waiting for a decision.
const WAITING_STATES: [&str; 2] = ["open", "queued"];

/// How many waiting claims, in how many lessons, cite any of `versions`.
pub async fn impact(
    pool: &SqlitePool,
    versions: &[CoveredVersion],
) -> Result<LearningTrustImpactDto> {
    let count = |value: usize| u32::try_from(value).unwrap_or(u32::MAX);
    if versions.is_empty() {
        return Ok(LearningTrustImpactDto {
            versions: 0,
            claims: 0,
            lessons: 0,
        });
    }
    let covered = json!(versions
        .iter()
        .map(
            |version| json!({ "program": version.program_id, "version": version.source_version_id })
        )
        .collect::<Vec<_>>())
    .to_string();
    let states = json!(WAITING_STATES).to_string();
    let row = sqlx::query(
        "SELECT COUNT(DISTINCT c.claim_id) AS claims, COUNT(DISTINCT c.lesson_id) AS lessons \
         FROM learning_lesson_claims c JOIN learning_lesson_reviews r ON r.lesson_id = c.lesson_id \
         WHERE json_extract(c.state_json, '$.state') IN (SELECT value FROM json_each(?)) \
           AND EXISTS (SELECT 1 FROM json_each(c.evidence_json) shown, json_each(?) covered \
                       WHERE json_extract(covered.value, '$.program') = r.program_id \
                         AND json_extract(covered.value, '$.version') = json_extract(shown.value, '$.sourceVersionId'))",
    )
    .bind(states)
    .bind(covered)
    .fetch_one(pool)
    .await
    .map_err(db)?;
    let claims: i64 = row.try_get("claims").map_err(db)?;
    let lessons: i64 = row.try_get("lessons").map_err(db)?;
    Ok(LearningTrustImpactDto {
        versions: count(versions.len()),
        claims: u32::try_from(claims).unwrap_or(u32::MAX),
        lessons: u32::try_from(lessons).unwrap_or(u32::MAX),
    })
}

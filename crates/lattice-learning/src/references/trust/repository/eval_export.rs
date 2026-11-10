use super::rows::{override_from_row, OVERRIDE_COLUMNS};
use crate::persistence::db;
use lattice_core::domain::provenance::TrustOverride;
use lattice_core::shared::error::Result;
use sqlx::{Row, SqlitePool};

/// A version an override covers, with who added its source.
pub(in crate::references::trust) struct CoveredRow {
    pub version_id: String,
    pub added_by: String,
}

fn covered_rows(rows: &[sqlx::sqlite::SqliteRow]) -> Result<Vec<CoveredRow>> {
    rows.iter()
        .map(|row| {
            Ok(CoveredRow {
                version_id: row.try_get("version_id").map_err(db)?,
                added_by: row.try_get("added_by").map_err(db)?,
            })
        })
        .collect()
}

pub(in crate::references::trust) async fn source_versions(
    pool: &SqlitePool,
    program_id: &str,
    source_id: &str,
) -> Result<Vec<CoveredRow>> {
    let rows = sqlx::query(
        "SELECT v.id AS version_id, l.added_by FROM learning_source_library l          JOIN learning_source_versions v ON v.program_id = l.program_id AND v.id = l.active_version_id          WHERE l.program_id=? AND l.id=? AND l.deleted_at IS NULL",
    )
    .bind(program_id)
    .bind(source_id)
    .fetch_all(pool)
    .await
    .map_err(db)?;
    covered_rows(&rows)
}

pub(in crate::references::trust) async fn site_versions(
    pool: &SqlitePool,
    registrable_domain: &str,
) -> Result<Vec<CoveredRow>> {
    let rows = sqlx::query(
        "SELECT p.source_version_id AS version_id, l.added_by FROM source_provenance p          JOIN learning_source_versions v ON v.program_id = p.program_id AND v.id = p.source_version_id          JOIN learning_source_library l ON l.program_id = v.program_id AND l.id = v.source_id          WHERE p.site=? ORDER BY p.program_id, p.source_version_id",
    )
    .bind(registrable_domain)
    .fetch_all(pool)
    .await
    .map_err(db)?;
    covered_rows(&rows)
}

pub(in crate::references::trust) async fn all_overrides(
    pool: &SqlitePool,
) -> Result<Vec<TrustOverride>> {
    sqlx::query(&format!(
        "SELECT {OVERRIDE_COLUMNS} FROM source_trust_overrides ORDER BY scope, registrable_domain, class_key, program_id, source_id"
    ))
    .fetch_all(pool)
    .await
    .map_err(db)?
    .iter()
    .map(override_from_row)
    .collect()
}

use super::rows::{override_from_row, OVERRIDE_COLUMNS};
use lattice_core::domain::provenance::TrustOverride;
use lattice_core::shared::error::Result;
use lattice_core::shared::persistence::Database;
use sqlx::Row;

/// A version an override covers, with who added its source.
pub(in crate::references::trust) struct CoveredRow {
    pub version_id: String,
    pub added_by: String,
}

fn covered_rows(rows: &[sqlx::sqlite::SqliteRow]) -> Result<Vec<CoveredRow>> {
    rows.iter()
        .map(|row| {
            Ok(CoveredRow {
                version_id: row.try_get("version_id")?,
                added_by: row.try_get("added_by")?,
            })
        })
        .collect()
}

pub(in crate::references::trust) async fn source_versions(
    database: &Database,
    program_id: &str,
    source_id: &str,
) -> Result<Vec<CoveredRow>> {
    let mut read = database.begin_read().await?;
    let rows = sqlx::query(
        "SELECT v.id AS version_id, l.added_by FROM learning_source_library l          JOIN learning_source_versions v ON v.program_id = l.program_id AND v.id = l.active_version_id          WHERE l.program_id=? AND l.id=? AND l.deleted_at IS NULL",
    )
    .bind(program_id)
    .bind(source_id)
    .fetch_all(&mut *read)
    .await?;
    read.commit().await?;
    covered_rows(&rows)
}

pub(in crate::references::trust) async fn site_versions(
    database: &Database,
    registrable_domain: &str,
) -> Result<Vec<CoveredRow>> {
    let mut read = database.begin_read().await?;
    let rows = sqlx::query(
        "SELECT p.source_version_id AS version_id, l.added_by FROM source_provenance p          JOIN learning_source_versions v ON v.program_id = p.program_id AND v.id = p.source_version_id          JOIN learning_source_library l ON l.program_id = v.program_id AND l.id = v.source_id          WHERE p.site=? ORDER BY p.program_id, p.source_version_id",
    )
    .bind(registrable_domain)
    .fetch_all(&mut *read)
    .await?;
    read.commit().await?;
    covered_rows(&rows)
}

pub(in crate::references::trust) async fn all_overrides(
    database: &Database,
) -> Result<Vec<TrustOverride>> {
    let mut read = database.begin_read().await?;
    let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT {OVERRIDE_COLUMNS} FROM source_trust_overrides ORDER BY scope, registrable_domain, class_key, program_id, source_id"
    )))
    .fetch_all(&mut *read)
    .await?;
    read.commit().await?;
    rows.iter().map(override_from_row).collect()
}

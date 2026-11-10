//! The library rows the provenance gate reads.

use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::persistence::DatabaseHandle;
use sqlx::Row;

fn db(error: sqlx::Error) -> AppError {
    AppError::Database(error.to_string())
}

/// A source version and the library facts about its source.
pub(super) struct VersionRow {
    pub source_id: String,
    pub full_text: String,
    pub acquired_at: i64,
    pub added_by: String,
    pub selection_note: Option<String>,
}

pub(super) async fn version_row(
    pool: &impl DatabaseHandle,
    program_id: &str,
    version_id: &str,
) -> Result<VersionRow> {
    let pool = pool.sqlx_pool();
    let row = sqlx::query(
        "SELECT v.source_id, v.full_text, v.acquired_at, l.added_by, l.selection_note FROM learning_source_versions v JOIN learning_source_library l ON l.id = v.source_id AND l.program_id = v.program_id WHERE v.program_id = ? AND v.id = ?",
    )
    .bind(program_id)
    .bind(version_id)
    .fetch_one(pool)
    .await
    .map_err(db)?;
    Ok(VersionRow {
        source_id: row.try_get("source_id").map_err(db)?,
        full_text: row.try_get("full_text").map_err(db)?,
        acquired_at: row.try_get("acquired_at").map_err(db)?,
        added_by: row.try_get("added_by").map_err(db)?,
        selection_note: row.try_get("selection_note").map_err(db)?,
    })
}

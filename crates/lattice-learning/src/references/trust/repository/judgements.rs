use crate::persistence::db;
use lattice_core::shared::error::Result;
use lattice_core::shared::persistence::Database;
use sqlx::Row;

/// What a claim's re-profile reads: its statement and where it sits.
pub(in crate::references::trust) struct ClaimContext {
    pub statement: String,
    pub unit_index: i64,
    pub lesson_title: String,
    pub goal: String,
}

pub(in crate::references::trust) async fn active_version_id(
    database: &Database,
    program_id: &str,
    source_id: &str,
) -> Result<Option<String>> {
    let mut read = database.begin_read().await.map_err(db)?;
    let version = sqlx::query_scalar::<_, Option<String>>(
        "SELECT active_version_id FROM learning_source_library WHERE program_id=? AND id=? AND deleted_at IS NULL",
    )
    .bind(program_id)
    .bind(source_id)
    .fetch_optional(&mut *read)
    .await
    .map_err(db)?;
    read.commit().await.map_err(db)?;
    Ok(version.flatten())
}

pub(in crate::references::trust) async fn claim_context(
    database: &Database,
    program_id: &str,
    claim_id: &str,
) -> Result<Option<ClaimContext>> {
    let mut read = database.begin_read().await.map_err(db)?;
    let row = sqlx::query(
        "SELECT c.statement, c.unit_index, l.title AS lesson_title, p.goal          FROM learning_lesson_claims c          JOIN learning_lesson_reviews r ON r.lesson_id = c.lesson_id          JOIN learning_lessons l ON l.id = c.lesson_id AND l.program_id = r.program_id          JOIN learning_programs p ON p.id = r.program_id          WHERE c.claim_id=? AND r.program_id=?",
    )
    .bind(claim_id)
    .bind(program_id)
    .fetch_optional(&mut *read)
    .await
    .map_err(db)?;
    read.commit().await.map_err(db)?;
    let Some(row) = row else {
        return Ok(None);
    };
    Ok(Some(ClaimContext {
        statement: row.try_get("statement").map_err(db)?,
        unit_index: row.try_get("unit_index").map_err(db)?,
        lesson_title: row.try_get("lesson_title").map_err(db)?,
        goal: row.try_get("goal").map_err(db)?,
    }))
}

use lattice_core::shared::persistence::Database;
use lattice_core::shared::{AppError, Result};
use sqlx::Row;
use std::collections::HashMap;

/// The titles of `programs` by id; a course deleted meanwhile is left out.
pub(crate) async fn course_titles(
    db: &Database,
    programs: &[String],
) -> Result<HashMap<String, String>> {
    titles(
        db,
        "SELECT id, title FROM learning_programs WHERE id IN (SELECT value FROM json_each(?))",
        programs,
    )
    .await
}

pub(super) async fn lesson_titles(
    db: &Database,
    lessons: &[String],
) -> Result<HashMap<String, String>> {
    titles(
        db,
        "SELECT id, title FROM learning_lessons WHERE id IN (SELECT value FROM json_each(?))",
        lessons,
    )
    .await
}

async fn titles(db: &Database, sql: &str, ids: &[String]) -> Result<HashMap<String, String>> {
    if ids.is_empty() {
        return Ok(HashMap::new());
    }
    let ids = serde_json::to_string(ids).map_err(|e| AppError::Serialization(e.to_string()))?;
    let mut read = db.begin_read().await?;
    let rows = sqlx::query(sql)
        .bind(ids)
        .fetch_all(&mut *read)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;
    read.commit().await?;
    Ok(rows
        .iter()
        .map(|row| (row.get::<String, _>("id"), row.get::<String, _>("title")))
        .collect())
}

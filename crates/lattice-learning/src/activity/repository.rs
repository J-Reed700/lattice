use lattice_core::shared::persistence::driver::DriverPool;
use lattice_core::shared::{AppError, Result};
use sqlx::{Row, SqlitePool};
use std::collections::HashMap;

/// The titles of `programs` by id; a course deleted meanwhile is left out.
pub(crate) async fn course_titles(
    pool: &impl DriverPool,
    programs: &[String],
) -> Result<HashMap<String, String>> {
    let pool = pool.sqlx_pool();
    titles(
        pool,
        "SELECT id, title FROM learning_programs WHERE id IN (SELECT value FROM json_each(?))",
        programs,
    )
    .await
}

pub(super) async fn lesson_titles(
    pool: &impl DriverPool,
    lessons: &[String],
) -> Result<HashMap<String, String>> {
    let pool = pool.sqlx_pool();
    titles(
        pool,
        "SELECT id, title FROM learning_lessons WHERE id IN (SELECT value FROM json_each(?))",
        lessons,
    )
    .await
}

async fn titles(pool: &SqlitePool, sql: &str, ids: &[String]) -> Result<HashMap<String, String>> {
    if ids.is_empty() {
        return Ok(HashMap::new());
    }
    let ids = serde_json::to_string(ids).map_err(|e| AppError::Serialization(e.to_string()))?;
    let rows = sqlx::query(sql)
        .bind(ids)
        .fetch_all(pool)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;
    Ok(rows
        .iter()
        .map(|row| (row.get::<String, _>("id"), row.get::<String, _>("title")))
        .collect())
}

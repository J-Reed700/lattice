use futures::FutureExt;
use lattice_core::shared::{AppError, Result};
use sqlx::SqlitePool;

pub(super) async fn program_ids(pool: &SqlitePool) -> Result<Vec<String>> {
    sqlx::query_scalar("SELECT id FROM learning_programs")
        .fetch_all(pool)
        .boxed()
        .await
        .map_err(|error| AppError::Database(error.to_string()))
}

pub(super) async fn program_exists(pool: &SqlitePool, program: &str) -> Result<bool> {
    sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM learning_programs WHERE id=?")
        .bind(program)
        .fetch_one(pool)
        .boxed()
        .await
        .map(|count| count > 0)
        .map_err(|error| AppError::Database(error.to_string()))
}

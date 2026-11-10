use lattice_core::shared::persistence::driver::DriverPool;
use lattice_core::shared::{AppError, Result};

pub(super) async fn program_ids(pool: &impl DriverPool) -> Result<Vec<String>> {
    let pool = pool.sqlx_pool();
    sqlx::query_scalar("SELECT id FROM learning_programs")
        .fetch_all(pool)
        .await
        .map_err(|error| AppError::Database(error.to_string()))
}

pub(super) async fn program_exists(pool: &impl DriverPool, program: &str) -> Result<bool> {
    let pool = pool.sqlx_pool();
    sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM learning_programs WHERE id=?")
        .bind(program)
        .fetch_one(pool)
        .await
        .map(|count| count > 0)
        .map_err(|error| AppError::Database(error.to_string()))
}

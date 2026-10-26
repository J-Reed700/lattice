use lattice_core::shared::error::Result;
use sqlx::SqlitePool;

pub async fn initialize_database(pool: &SqlitePool) -> Result<()> {
    tracing::info!("Initializing database...");

    crate::infrastructure::persistence::database::migrate::run_migrations(pool).await?;

    Ok(())
}

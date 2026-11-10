use lattice_core::shared::error::Result;
use lattice_core::shared::persistence::DatabaseHandle;

pub async fn initialize_database(pool: &impl DatabaseHandle) -> Result<()> {
    tracing::info!("Initializing database...");

    crate::infrastructure::persistence::database::migrate::run_migrations(pool).await?;

    Ok(())
}

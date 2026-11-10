use lattice_core::shared::error::Result;
use lattice_core::shared::persistence::driver::DriverPool;

pub async fn initialize_database(pool: &impl DriverPool) -> Result<()> {
    tracing::info!("Initializing database...");

    crate::infrastructure::persistence::database::migrate::run_migrations(pool).await?;

    Ok(())
}

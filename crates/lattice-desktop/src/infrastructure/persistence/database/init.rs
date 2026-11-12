use lattice_core::shared::error::Result;
use lattice_core::shared::persistence::Database;

pub async fn initialize_database(db: &Database) -> Result<()> {
    tracing::info!("Initializing database...");

    crate::infrastructure::persistence::database::migrate::run_migrations(db).await?;

    Ok(())
}

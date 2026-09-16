use crate::shared::error::Result;
use sqlx::SqlitePool;

pub async fn initialize_database(pool: &SqlitePool) -> Result<()> {
    tracing::info!("Initializing database...");

    apply_pragmas(pool).await?;

    crate::infrastructure::persistence::database::migrate::run_migrations(pool).await?;

    Ok(())
}

async fn apply_pragmas(pool: &SqlitePool) -> Result<()> {
    let pragmas = [
        "PRAGMA foreign_keys = ON",
        "PRAGMA journal_mode = WAL",
        "PRAGMA synchronous = NORMAL",
        "PRAGMA cache_size = -20000",
        "PRAGMA temp_store = MEMORY",
        "PRAGMA mmap_size = 268435456",
    ];

    for pragma in &pragmas {
        sqlx::query(pragma).execute(pool).await?;
    }

    Ok(())
}

//! Database Schema Constants and Utilities
//! All table creation is handled by migrations

use lattice_core::shared::error::Result;
use sqlx::SqlitePool;

pub const SCHEMA_VERSION: i32 = 18;

/// Initializes the database schema by running migrations.
pub async fn initialize_schema(pool: &SqlitePool) -> Result<()> {
    sqlx::migrate!("./migrations")
        .run(pool)
        .await
        .map_err(|e| lattice_core::shared::error::AppError::Database(e.to_string()))?;

    Ok(())
}

/// Rebuild FTS5 index from scratch
pub async fn rebuild_fts5_index(pool: &SqlitePool) -> Result<()> {
    tracing::info!("Rebuilding FTS5 index...");

    sqlx::query("DELETE FROM documents_fts")
        .execute(pool)
        .await?;

    sqlx::query(
        r#"
        INSERT INTO documents_fts(document_id, content)
        SELECT document_id, GROUP_CONCAT(content, ' ')
        FROM text_chunks
        GROUP BY document_id
        "#,
    )
    .execute(pool)
    .await?;

    tracing::info!("FTS5 index rebuilt");
    Ok(())
}

//! Database Schema Constants and Utilities
//! All table creation is handled by migrations

use lattice_core::shared::error::Result;
use lattice_core::shared::persistence::Database;

pub const SCHEMA_VERSION: i32 = 18;

/// Rebuild FTS5 index from scratch
pub async fn rebuild_fts5_index(db: &Database) -> Result<()> {
    tracing::info!("Rebuilding FTS5 index...");

    let mut tx = db.begin_write().await?;

    sqlx::query("DELETE FROM documents_fts")
        .execute(&mut *tx)
        .await?;

    sqlx::query(
        r#"
        INSERT INTO documents_fts(document_id, content)
        SELECT document_id, GROUP_CONCAT(content, ' ')
        FROM text_chunks
        GROUP BY document_id
        "#,
    )
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    tracing::info!("FTS5 index rebuilt");
    Ok(())
}

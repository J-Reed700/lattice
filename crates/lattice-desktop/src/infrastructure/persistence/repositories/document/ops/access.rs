//! Access metadata changes must never rewrite document children.

use lattice_core::shared::error::Result;
use sqlx::SqliteConnection;

pub async fn record_accesses(
    connection: &mut SqliteConnection,
    document_ids: &[String],
) -> Result<()> {
    if document_ids.is_empty() {
        return Ok(());
    }
    sqlx::query(
        "UPDATE documents
         SET access_count=MIN(access_count+1,2147483647),last_accessed_at=?,
             updated_at=CURRENT_TIMESTAMP
         WHERE id IN (SELECT value FROM json_each(?))",
    )
    .bind(chrono::Utc::now().to_rfc3339())
    .bind(serde_json::to_string(document_ids)?)
    .execute(connection)
    .await?;
    Ok(())
}

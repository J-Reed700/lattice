//! Document CRUD operations.

use super::checksum::calculate_checksum;
use super::types::DocumentRecord;
use crate::features::indexing::engine::error::Result;
use lattice_core::shared::error::ResultExt;
use lattice_core::shared::persistence::Database;
use sqlx::Row;
use std::path::Path;

/// Check if a document exists in the database by path.
pub async fn document_exists(db: &Database, path: &Path) -> Result<bool> {
    let file_path = path
        .to_str()
        .with_context(|| format!("Invalid UTF-8 in path: {}", path.display()))?;

    let mut read = db.begin_read().await?;
    let count: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*) FROM documents WHERE file_path = ?
        "#,
    )
    .bind(file_path)
    .fetch_one(&mut *read)
    .await?;
    read.commit().await?;

    Ok(count > 0)
}

/// Get document record by file path.
pub async fn get_document_by_path(db: &Database, path: &Path) -> Result<Option<DocumentRecord>> {
    let file_path = path
        .to_str()
        .with_context(|| format!("Invalid UTF-8 in path: {}", path.display()))?;

    let mut read = db.begin_read().await?;
    let result = sqlx::query(
        r#"
        SELECT
            id, file_path, file_name, file_type, mime_type,
            size_bytes, checksum, status
        FROM documents
        WHERE file_path = ?
        "#,
    )
    .bind(file_path)
    .fetch_optional(&mut *read)
    .await?;
    read.commit().await?;

    Ok(result.map(|row| {
        let id: String = row.get("id");
        let file_path: String = row.get("file_path");
        let file_name: String = row.get("file_name");
        let file_type: Option<String> = row.get("file_type");
        let mime_type: Option<String> = row.get("mime_type");
        let size_bytes: i64 = row.get("size_bytes");
        let checksum: String = row.get("checksum");
        let status: String = row.get("status");

        DocumentRecord {
            id,
            file_path: file_path.clone(),
            file_name,
            file_type,
            mime_type: mime_type.unwrap_or_default(),
            size_bytes,
            checksum,
            status,
            path: file_path,
            indexed_at: chrono::Utc::now().to_rfc3339(),
        }
    }))
}

/// Check if a document needs reindexing (checksum changed).
pub async fn needs_reindex(db: &Database, path: &Path) -> Result<bool> {
    let current_checksum = calculate_checksum(path).await?;

    if let Some(doc) = get_document_by_path(db, path).await? {
        Ok(doc.checksum != current_checksum)
    } else {
        Ok(true)
    }
}

/// Update document status.
pub async fn mark_document_status(db: &Database, path: &Path, status: &str) -> Result<()> {
    let file_path = path
        .to_str()
        .with_context(|| format!("Invalid UTF-8 in path: {}", path.display()))?;

    let mut tx = db.begin_write().await?;
    sqlx::query!(
        r#"
        UPDATE documents SET status = ?, updated_at = CURRENT_TIMESTAMP
        WHERE file_path = ?
        "#,
        status,
        file_path
    )
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;

    Ok(())
}

/// Remove a document from the database.
pub async fn remove_document(db: &Database, path: &Path) -> Result<()> {
    let file_path = path
        .to_str()
        .with_context(|| format!("Invalid UTF-8 in path: {}", path.display()))?;

    let mut tx = db.begin_write().await?;
    sqlx::query!(
        r#"
        DELETE FROM documents WHERE file_path = ?
        "#,
        file_path
    )
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;

    Ok(())
}

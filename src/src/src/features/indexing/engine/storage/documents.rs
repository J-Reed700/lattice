//! Document CRUD operations.

use super::checksum::calculate_checksum;
use super::types::DocumentRecord;
use crate::infrastructure::indexing::error::{IndexingError, Result};
use crate::shared::error::ResultExt;
use crate::shared::utils::path::path_to_string;
use chrono::Utc;
use serde::Deserialize;
use sqlx::{Row, SqlitePool};
use std::path::Path;
use uuid::Uuid;

/// Metadata structure from web article metadata.json
#[derive(Debug, Deserialize)]
struct WebArticleMetadata {
    title: String,
}

/// Check if path is a web article (has sibling metadata.json)
fn is_web_article(path: &Path) -> bool {
    if let Some(parent) = path.parent() {
        parent.join("metadata.json").exists()
    } else {
        false
    }
}

/// Extract title from web article metadata.json
async fn get_web_article_title(path: &Path) -> Result<String> {
    let parent = path.parent().ok_or_else(|| IndexingError::FileRead {
        path: path.display().to_string(),
        reason: "Path has no parent directory".to_string(),
    })?;

    let metadata_path = parent.join("metadata.json");
    let metadata_content = tokio::fs::read_to_string(&metadata_path)
        .await
        .map_err(|e| IndexingError::FileRead {
            path: metadata_path.display().to_string(),
            reason: format!("Failed to read metadata.json: {}", e),
        })?;

    let metadata: WebArticleMetadata =
        serde_json::from_str(&metadata_content).map_err(|e| IndexingError::FileRead {
            path: metadata_path.display().to_string(),
            reason: format!("Failed to parse metadata.json: {}", e),
        })?;

    Ok(metadata.title)
}

/// Check if a document exists in the database by path.
pub async fn document_exists(pool: &SqlitePool, path: &Path) -> Result<bool> {
    let file_path = path
        .to_str()
        .with_context(|| format!("Invalid UTF-8 in path: {}", path.display()))?;

    let count: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*) FROM documents WHERE file_path = ?
        "#,
    )
    .bind(file_path)
    .fetch_one(pool)
    .await?;

    Ok(count > 0)
}

/// Get document record by file path.
pub async fn get_document_by_path(
    pool: &SqlitePool,
    path: &Path,
) -> Result<Option<DocumentRecord>> {
    let file_path = path
        .to_str()
        .with_context(|| format!("Invalid UTF-8 in path: {}", path.display()))?;

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
    .fetch_optional(pool)
    .await?;

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
pub async fn needs_reindex(pool: &SqlitePool, path: &Path) -> Result<bool> {
    let current_checksum = calculate_checksum(path).await?;

    if let Some(doc) = get_document_by_path(pool, path).await? {
        Ok(doc.checksum != current_checksum)
    } else {
        Ok(true)
    }
}

/// Update document status.
pub async fn mark_document_status(pool: &SqlitePool, path: &Path, status: &str) -> Result<()> {
    let file_path = path
        .to_str()
        .with_context(|| format!("Invalid UTF-8 in path: {}", path.display()))?;

    sqlx::query!(
        r#"
        UPDATE documents SET status = ?, updated_at = CURRENT_TIMESTAMP
        WHERE file_path = ?
        "#,
        status,
        file_path
    )
    .execute(pool)
    .await?;

    Ok(())
}

/// Remove a document from the database.
pub async fn remove_document(pool: &SqlitePool, path: &Path) -> Result<()> {
    let file_path = path
        .to_str()
        .with_context(|| format!("Invalid UTF-8 in path: {}", path.display()))?;

    sqlx::query!(
        r#"
        DELETE FROM documents WHERE file_path = ?
        "#,
        file_path
    )
    .execute(pool)
    .await?;

    Ok(())
}

/// Store only file metadata without indexing content.
pub async fn store_file_metadata_only(
    pool: &SqlitePool,
    path: &Path,
    _file_id: &str,
    mime_type: &str,
) -> Result<String> {
    let path_str = path_to_string(path).map_err(|e| IndexingError::FileRead {
        path: path.display().to_string(),
        reason: format!("Invalid path encoding: {}", e),
    })?;

    // Extract file_name - use title for web articles
    let filename = if is_web_article(path) {
        // Web article: extract title from metadata.json
        match get_web_article_title(path).await {
            Ok(title) => title,
            Err(e) => {
                tracing::warn!(
                    "Failed to extract title from web article metadata.json, using filename: {}",
                    e
                );
                path.file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("unknown")
                    .to_string()
            }
        }
    } else {
        // Local file: use filename
        path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string()
    };

    let checksum = calculate_checksum(path).await?;

    let metadata = tokio::fs::metadata(path)
        .await
        .map_err(|e| IndexingError::FileRead {
            path: path.display().to_string(),
            reason: e.to_string(),
        })?;

    let size_bytes = metadata.len() as i64;

    let modified_at = metadata
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let indexed_at = Utc::now().to_rfc3339();
    let modified_at_str = chrono::DateTime::from_timestamp(modified_at as i64, 0)
        .map(|dt| dt.to_rfc3339())
        .unwrap_or_else(|| {
            tracing::warn!("Invalid timestamp {}, using current time", modified_at);
            indexed_at.clone()
        });

    let doc_id = Uuid::new_v4().to_string();

    // Return the id the row actually has, not the one we generated. On a
    // re-store the DO UPDATE branch leaves `id` alone, so handing the caller
    // the generated UUID pointed them at a document that does not exist.
    // Non-macro form to avoid regenerating the offline sqlx cache.
    let actual_id: String = sqlx::query_scalar(
        r#"
        INSERT INTO documents (
            id, file_path, file_name, mime_type,
            size_bytes, modified_at, indexed_at,
            checksum, status
        )
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, 'stored')
        ON CONFLICT(file_path) DO UPDATE SET
            indexed_at = excluded.indexed_at,
            checksum = excluded.checksum,
            status = 'stored',
            updated_at = CURRENT_TIMESTAMP
        RETURNING id
        "#,
    )
    .bind(&doc_id)
    .bind(&path_str)
    .bind(&filename)
    .bind(mime_type)
    .bind(size_bytes)
    .bind(&modified_at_str)
    .bind(&indexed_at)
    .bind(&checksum)
    .fetch_one(pool)
    .await?;

    Ok(actual_id)
}

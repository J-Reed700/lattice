//! Single-value queries: counts, existence checks, checksums, paths and ids.

use lattice_core::shared::error::{AppError, Result};
use sqlx::SqliteConnection;

pub async fn exists_by_path(conn: &mut SqliteConnection, file_path: &str) -> Result<bool> {
    sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM documents WHERE file_path = ?)")
        .bind(file_path)
        .fetch_one(conn)
        .await
        .map_err(|e| AppError::Database(format!("Failed to check document existence: {}", e)))
}

pub async fn count_documents(conn: &mut SqliteConnection) -> Result<i64> {
    sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM documents")
        .fetch_one(conn)
        .await
        .map_err(|e| AppError::Database(format!("Failed to count documents: {}", e)))
}

pub async fn count_chunks(conn: &mut SqliteConnection) -> Result<i64> {
    sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM text_chunks")
        .fetch_one(conn)
        .await
        .map_err(|e| AppError::Database(format!("Failed to count chunks: {}", e)))
}

/// How many documents reference this content checksum.
pub async fn count_by_checksum(conn: &mut SqliteConnection, checksum: &str) -> Result<u64> {
    sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM documents WHERE checksum = ?")
        .bind(checksum)
        .fetch_one(conn)
        .await
        .map(|count| count.max(0) as u64)
        .map_err(|e| AppError::Database(format!("Failed to count documents by checksum: {}", e)))
}

/// Every distinct content checksum referenced by a document.
pub async fn list_checksums(conn: &mut SqliteConnection) -> Result<Vec<String>> {
    sqlx::query_scalar::<_, String>(
        "SELECT DISTINCT checksum FROM documents WHERE checksum IS NOT NULL AND checksum <> ''",
    )
    .fetch_all(conn)
    .await
    .map_err(|e| AppError::Database(format!("Failed to list document checksums: {}", e)))
}

/// The content checksum of one document, without loading its aggregate.
pub async fn find_checksum_by_id(conn: &mut SqliteConnection, id: &str) -> Result<String> {
    sqlx::query_scalar::<_, String>("SELECT checksum FROM documents WHERE id = ?")
        .bind(id)
        .fetch_optional(conn)
        .await
        .map_err(|e| AppError::Database(format!("Failed to find document checksum: {}", e)))?
        .ok_or_else(|| AppError::NotFound(format!("Document not found: {id}")))
}

pub async fn count(conn: &mut SqliteConnection) -> Result<usize> {
    sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM documents")
        .fetch_one(conn)
        .await
        .map(|count| count as usize)
        .map_err(|e| AppError::Database(format!("Failed to count documents: {}", e)))
}

pub async fn exists(conn: &mut SqliteConnection, id: &str) -> Result<bool> {
    sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM documents WHERE id = ?)")
        .bind(id)
        .fetch_one(conn)
        .await
        .map_err(|e| AppError::Database(format!("Failed to check document existence: {}", e)))
}

pub async fn find_file_path_by_id(
    conn: &mut SqliteConnection,
    document_id: &str,
) -> Result<String> {
    let file_path = sqlx::query!(
        r#"
        SELECT file_path as "path!"
        FROM documents
        WHERE id = ?
        "#,
        document_id
    )
    .fetch_optional(conn)
    .await
    .map_err(|e| {
        AppError::Database(format!(
            "Failed to find file path for document '{}': {}",
            document_id, e
        ))
    })?;

    match file_path {
        Some(record) => Ok(record.path),
        None => Err(AppError::NotFound(format!(
            "Document not found: {}",
            document_id
        ))),
    }
}

pub async fn find_id_by_path(
    conn: &mut SqliteConnection,
    file_path: &str,
) -> Result<Option<String>> {
    let record = sqlx::query!("SELECT id FROM documents WHERE file_path = ?", file_path)
        .fetch_optional(conn)
        .await
        .map_err(|e| {
            AppError::Database(format!(
                "Failed to find document ID for path '{}': {}",
                file_path, e
            ))
        })?;

    Ok(record.and_then(|r| r.id))
}

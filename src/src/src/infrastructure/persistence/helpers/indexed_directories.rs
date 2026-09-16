//! Database query helpers for FileAccessConfig initialization
//!
//! Queries indexed directories from the documents table to populate
//! FileAccessConfig allowed_roots at application startup.

use crate::shared::error::{AppError, Result};
use sqlx::SqlitePool;
use std::path::PathBuf;

/// Query unique top-level indexed directories from documents table
///
/// Extracts the first path component (directory name) from all indexed
/// file paths to determine which directories have been indexed.
///
/// # Returns
/// - `Ok(Vec<PathBuf>)` - List of unique directory names
/// - `Err(AppError)` - Database query error
///
/// # Example
/// ```
/// // If documents table has:
/// // - "notes/doc1.txt"
/// // - "notes/doc2.txt"
/// // - "research/paper.pdf"
/// //
/// // Returns: ["notes", "research"]
/// ```
pub async fn query_indexed_directories(pool: &SqlitePool) -> Result<Vec<PathBuf>> {
    let rows = sqlx::query!(
        r#"
        SELECT DISTINCT
            CASE
                WHEN instr(file_path, '/') > 0
                THEN substr(file_path, 1, instr(file_path, '/') - 1)
                ELSE file_path
            END as root_dir
        FROM documents
        WHERE file_path IS NOT NULL
          AND file_path != ''
        ORDER BY root_dir
        "#
    )
    .fetch_all(pool)
    .await
    .map_err(|e| AppError::Database(format!("Failed to query indexed directories: {}", e)))?;

    Ok(rows
        .into_iter()
        .map(|row| PathBuf::from(row.root_dir))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    #[tokio::test]
    async fn test_query_indexed_directories() {
        let pool = SqlitePoolOptions::new().connect(":memory:").await.unwrap();

        // Create schema
        sqlx::query(
            "CREATE TABLE documents (
                id TEXT PRIMARY KEY,
                file_path TEXT NOT NULL,
                title TEXT,
                content TEXT
            )",
        )
        .execute(&pool)
        .await
        .unwrap();

        // Insert test data
        sqlx::query("INSERT INTO documents (id, file_path, title, content) VALUES (?, ?, ?, ?)")
            .bind("doc-1")
            .bind("notes/doc1.txt")
            .bind("Doc 1")
            .bind("Content 1")
            .execute(&pool)
            .await
            .unwrap();

        sqlx::query("INSERT INTO documents (id, file_path, title, content) VALUES (?, ?, ?, ?)")
            .bind("doc-2")
            .bind("notes/doc2.txt")
            .bind("Doc 2")
            .bind("Content 2")
            .execute(&pool)
            .await
            .unwrap();

        sqlx::query("INSERT INTO documents (id, file_path, title, content) VALUES (?, ?, ?, ?)")
            .bind("doc-3")
            .bind("research/paper.pdf")
            .bind("Paper")
            .bind("Content 3")
            .execute(&pool)
            .await
            .unwrap();

        // Query directories
        let dirs = query_indexed_directories(&pool).await.unwrap();

        assert_eq!(dirs.len(), 2);
        assert!(dirs.contains(&PathBuf::from("notes")));
        assert!(dirs.contains(&PathBuf::from("research")));
    }
}

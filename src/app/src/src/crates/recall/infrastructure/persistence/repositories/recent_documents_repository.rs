//! Recent Documents Repository Implementation
//!
//! Infrastructure implementation for recent documents persistence using SQLite.
//!
//! # Architecture
//!
//! This repository implements the RecentDocumentsRepositoryPort trait,
//! providing operations for tracking and retrieving recently accessed documents.

use crate::application::dtos::RecentDocumentDto;
use crate::application::ports::RecentDocumentsRepositoryPort;
use crate::infrastructure::persistence::database::query_with_timeout;
use crate::shared::error::Result;
use async_trait::async_trait;
use chrono::Utc;
use sqlx::{Row, SqlitePool};

/// SQLite implementation of recent documents repository.
///
/// Handles persistence of document access tracking in SQLite database.
/// Uses upsert logic to update access count and timestamp for existing entries.
///
/// # Thread Safety
///
/// This repository is thread-safe through SQLitePool's internal connection management.
#[derive(Debug, Clone)]
pub struct RecentDocumentsRepository {
    pool: SqlitePool,
}

impl RecentDocumentsRepository {
    /// Create a new recent documents repository.
    ///
    /// # Arguments
    ///
    /// * `pool` - SQLite connection pool
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl RecentDocumentsRepositoryPort for RecentDocumentsRepository {
    async fn track_access(&self, document_id: &str) -> Result<()> {
        let pool = self.pool.clone();
        let document_id = document_id.to_string();
        let now = Utc::now().to_rfc3339();

        query_with_timeout(|| async {
            // Upsert: Insert new or update existing
            // If document_id exists, increment access_count and update last_accessed_at
            sqlx::query!(
                r#"
                INSERT INTO recent_documents (document_id, last_accessed_at, access_count)
                VALUES (?, ?, 1)
                ON CONFLICT(document_id) DO UPDATE SET
                    last_accessed_at = excluded.last_accessed_at,
                    access_count = access_count + 1
                "#,
                document_id,
                now
            )
            .execute(&pool)
            .await?;

            Ok(())
        })
        .await
    }

    async fn get_recent_documents(&self, limit: usize) -> Result<Vec<RecentDocumentDto>> {
        let pool = self.pool.clone();
        let limit = limit as i64;

        query_with_timeout(|| async {
            let results = sqlx::query(
                r#"
                SELECT
                    rd.document_id,
                    rd.last_accessed_at,
                    rd.access_count,
                    d.id,
                    COALESCE(files.name, d.title) as file_name,
                    COALESCE(files.path, d.source_url, 'unknown') as file_path,
                    COALESCE(files.mime_type, d.content_type) as file_type
                FROM recent_documents rd
                INNER JOIN documents d ON rd.document_id = d.id
                LEFT JOIN files ON d.file_id = files.id
                ORDER BY rd.last_accessed_at DESC
                LIMIT ?
                "#,
            )
            .bind(limit)
            .fetch_all(&pool)
            .await?;

            Ok(results
                .into_iter()
                .map(|row| RecentDocumentDto {
                    id: row.get("id"),
                    document_id: row.get("document_id"),
                    document_name: row.get("file_name"),
                    document_path: row.get("file_path"),
                    file_type: row.get("file_type"),
                    last_accessed_at: row.get("last_accessed_at"),
                    access_count: row.get("access_count"),
                })
                .collect())
        })
        .await
    }

    async fn clear_recent_history(&self, before_date: Option<&str>) -> Result<usize> {
        let pool = self.pool.clone();

        query_with_timeout(|| async {
            let result = if let Some(date) = before_date {
                let date = date.to_string();
                sqlx::query!(
                    r#"
                    DELETE FROM recent_documents
                    WHERE last_accessed_at < ?
                    "#,
                    date
                )
                .execute(&pool)
                .await?
            } else {
                // Clear all history if no date specified
                sqlx::query!(
                    r#"
                    DELETE FROM recent_documents
                    "#
                )
                .execute(&pool)
                .await?
            };

            Ok(result.rows_affected() as usize)
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    async fn create_test_pool() -> SqlitePool {
        SqlitePoolOptions::new().connect(":memory:").await.unwrap()
    }

    async fn setup_schema(pool: &SqlitePool) {
        // Create files table
        sqlx::query(
            r#"
            CREATE TABLE files (
                id TEXT PRIMARY KEY NOT NULL,
                path TEXT NOT NULL UNIQUE,
                name TEXT NOT NULL,
                mime_type TEXT,
                size_bytes INTEGER NOT NULL DEFAULT 0,
                checksum_sha256 TEXT,
                created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
            )
            "#,
        )
        .execute(pool)
        .await
        .unwrap();

        // Create documents table
        sqlx::query(
            r#"
            CREATE TABLE documents (
                id TEXT PRIMARY KEY NOT NULL,
                file_id TEXT,
                title TEXT NOT NULL,
                content TEXT NOT NULL,
                source_url TEXT,
                source_type TEXT NOT NULL CHECK (source_type IN ('web', 'file', 'manual', 'api')),
                content_type TEXT NOT NULL,
                status TEXT NOT NULL CHECK (status IN ('pending', 'processing', 'indexed', 'failed')) DEFAULT 'pending',
                created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                indexed_at TEXT,
                FOREIGN KEY (file_id) REFERENCES files(id) ON DELETE CASCADE
            )
            "#,
        )
        .execute(pool)
        .await
        .unwrap();

        // Create recent_documents table
        sqlx::query(
            r#"
            CREATE TABLE recent_documents (
                document_id TEXT PRIMARY KEY NOT NULL,
                last_accessed_at TEXT NOT NULL,
                access_count INTEGER NOT NULL DEFAULT 1,
                FOREIGN KEY (document_id) REFERENCES documents(id) ON DELETE CASCADE
            )
            "#,
        )
        .execute(pool)
        .await
        .unwrap();

        // Insert test files
        sqlx::query(
            r#"
            INSERT INTO files (id, path, name, mime_type, size_bytes, checksum_sha256) VALUES
            ('file1', '/test/doc1.pdf', 'doc1.pdf', 'application/pdf', 1024, 'abc123'),
            ('file2', '/test/doc2.txt', 'doc2.txt', 'text/plain', 2048, 'def456')
            "#,
        )
        .execute(pool)
        .await
        .unwrap();

        // Insert test documents (file-based)
        sqlx::query(
            r#"
            INSERT INTO documents (id, file_id, title, content, source_type, content_type, status) VALUES
            ('doc1', 'file1', 'Test Document 1', 'Content here', 'file', 'application/pdf', 'indexed'),
            ('doc2', 'file2', 'Test Document 2', 'Content here', 'file', 'text/plain', 'indexed')
            "#,
        )
        .execute(pool)
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn test_track_access_new_document() {
        let pool = create_test_pool().await;
        setup_schema(&pool).await;
        let repo = RecentDocumentsRepository::new(pool.clone());

        // Track access to a new document
        repo.track_access("doc1").await.unwrap();

        // Verify it was added
        let recent = repo.get_recent_documents(10).await.unwrap();
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].document_id, "doc1");
        assert_eq!(recent[0].access_count, 1);
    }

    #[tokio::test]
    async fn test_track_access_existing_document_increments_count() {
        let pool = create_test_pool().await;
        setup_schema(&pool).await;
        let repo = RecentDocumentsRepository::new(pool.clone());

        // Track access multiple times
        repo.track_access("doc1").await.unwrap();
        repo.track_access("doc1").await.unwrap();
        repo.track_access("doc1").await.unwrap();

        // Verify count was incremented
        let recent = repo.get_recent_documents(10).await.unwrap();
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].document_id, "doc1");
        assert_eq!(recent[0].access_count, 3);
    }

    #[tokio::test]
    async fn test_track_access_updates_timestamp() {
        let pool = create_test_pool().await;
        setup_schema(&pool).await;
        let repo = RecentDocumentsRepository::new(pool.clone());

        // First access
        repo.track_access("doc1").await.unwrap();
        let first = repo.get_recent_documents(1).await.unwrap();
        let first_timestamp = first[0].last_accessed_at.clone();

        // Wait a bit
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        // Second access
        repo.track_access("doc1").await.unwrap();
        let second = repo.get_recent_documents(1).await.unwrap();
        let second_timestamp = second[0].last_accessed_at.clone();

        // Timestamp should be updated
        assert_ne!(first_timestamp, second_timestamp);
        assert!(second_timestamp > first_timestamp);
    }

    #[tokio::test]
    async fn test_get_recent_documents_ordered_by_access_time() {
        let pool = create_test_pool().await;
        setup_schema(&pool).await;
        let repo = RecentDocumentsRepository::new(pool.clone());

        // Access documents in sequence
        repo.track_access("doc1").await.unwrap();
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        repo.track_access("doc2").await.unwrap();

        // Should be ordered by most recent first
        let recent = repo.get_recent_documents(10).await.unwrap();
        assert_eq!(recent.len(), 2);
        assert_eq!(recent[0].document_id, "doc2"); // Most recent
        assert_eq!(recent[1].document_id, "doc1");
    }

    #[tokio::test]
    async fn test_get_recent_documents_respects_limit() {
        let pool = create_test_pool().await;
        setup_schema(&pool).await;
        let repo = RecentDocumentsRepository::new(pool.clone());

        // Track multiple documents
        repo.track_access("doc1").await.unwrap();
        repo.track_access("doc2").await.unwrap();

        // Request only 1
        let recent = repo.get_recent_documents(1).await.unwrap();
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].document_id, "doc2"); // Most recent
    }

    #[tokio::test]
    async fn test_clear_recent_history_all() {
        let pool = create_test_pool().await;
        setup_schema(&pool).await;
        let repo = RecentDocumentsRepository::new(pool.clone());

        // Track some documents
        repo.track_access("doc1").await.unwrap();
        repo.track_access("doc2").await.unwrap();

        // Verify they exist
        let recent = repo.get_recent_documents(10).await.unwrap();
        assert_eq!(recent.len(), 2);

        // Clear all history
        let cleared = repo.clear_recent_history(None).await.unwrap();
        assert_eq!(cleared, 2);

        // Verify all cleared
        let recent = repo.get_recent_documents(10).await.unwrap();
        assert_eq!(recent.len(), 0);
    }

    #[tokio::test]
    async fn test_clear_recent_history_before_date() {
        let pool = create_test_pool().await;
        setup_schema(&pool).await;
        let repo = RecentDocumentsRepository::new(pool.clone());

        // Track documents at different times
        repo.track_access("doc1").await.unwrap();
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        // Get the current time to use as cutoff
        let cutoff_time = Utc::now().to_rfc3339();

        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
        repo.track_access("doc2").await.unwrap();

        // Clear history before cutoff (should only clear doc1)
        let cleared = repo.clear_recent_history(Some(&cutoff_time)).await.unwrap();
        assert_eq!(cleared, 1);

        // Verify only doc2 remains
        let recent = repo.get_recent_documents(10).await.unwrap();
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].document_id, "doc2");
    }

    #[tokio::test]
    async fn test_get_recent_documents_includes_document_details() {
        let pool = create_test_pool().await;
        setup_schema(&pool).await;
        let repo = RecentDocumentsRepository::new(pool.clone());

        // Track access
        repo.track_access("doc1").await.unwrap();

        // Get recent and verify details
        let recent = repo.get_recent_documents(1).await.unwrap();
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].document_id, "doc1");
        assert_eq!(recent[0].document_name, "doc1.pdf");
        assert_eq!(recent[0].document_path, "/test/doc1.pdf");
        assert_eq!(recent[0].file_type, Some("application/pdf".to_string()));
        assert_eq!(recent[0].access_count, 1);
    }

    #[tokio::test]
    async fn test_track_access_multiple_documents() {
        let pool = create_test_pool().await;
        setup_schema(&pool).await;
        let repo = RecentDocumentsRepository::new(pool.clone());

        // Track different access patterns
        repo.track_access("doc1").await.unwrap();
        repo.track_access("doc2").await.unwrap();
        repo.track_access("doc1").await.unwrap(); // Access doc1 again

        let recent = repo.get_recent_documents(10).await.unwrap();
        assert_eq!(recent.len(), 2);

        // doc1 should be most recent (accessed last)
        assert_eq!(recent[0].document_id, "doc1");
        assert_eq!(recent[0].access_count, 2);

        // doc2 should be second
        assert_eq!(recent[1].document_id, "doc2");
        assert_eq!(recent[1].access_count, 1);
    }
}

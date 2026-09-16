//! Recent Documents Repository Implementation
//!
//! Infrastructure implementation for recent documents persistence using SQLite.
//!
//! # Architecture
//!
//! This repository implements the RecentDocumentsRepositoryPort trait,
//! providing operations for tracking and retrieving recently accessed documents.

use crate::application::contracts::recent_documents::RecentDocumentRecord as RecentDocumentDto;
use crate::application::ports::RecentDocumentsRepositoryPort;
use crate::infrastructure::persistence::database::query_with_timeout;
use crate::shared::error::{AppError, Result};
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

        let tracked = query_with_timeout(|| async {
            let mut tx = pool.begin().await?;
            let exists: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM documents WHERE id = ?")
                .bind(&document_id).fetch_one(&mut *tx).await?;
            if exists == 0 { return Ok(false); }
            let id = uuid::Uuid::new_v4().to_string();
            sqlx::query(
                r#"
                INSERT INTO recent_documents (id, document_id, last_accessed_at, access_count)
                VALUES (?, ?, ?, 1)
                ON CONFLICT(document_id) DO UPDATE SET
                    last_accessed_at = excluded.last_accessed_at,
                    access_count = access_count + 1
                "#
            )
            .bind(id).bind(&document_id).bind(&now)
            .execute(&mut *tx)
            .await?;

            // Tracking and bounded-history eviction succeed or roll back together.
            sqlx::query("DELETE FROM recent_documents WHERE document_id NOT IN (SELECT document_id FROM recent_documents ORDER BY last_accessed_at DESC, rowid DESC LIMIT 50)")
                .execute(&mut *tx).await?;
            tx.commit().await?;

            Ok(true)
        })
        .await?;
        if !tracked {
            return Err(AppError::NotFound("Document not found".into()));
        }
        Ok(())
    }

    async fn get_recent_documents(&self, limit: usize) -> Result<Vec<RecentDocumentDto>> {
        let pool = self.pool.clone();
        let limit = limit.min(100) as i64;

        query_with_timeout(|| async {
            let results = sqlx::query(
                r#"
                SELECT
                    rd.document_id,
                    rd.last_accessed_at,
                    rd.access_count,
                    COALESCE(rd.id, rd.document_id) as id,
                    d.file_name,
                    d.file_path,
                    d.mime_type as file_type
                FROM recent_documents rd
                INNER JOIN documents d ON rd.document_id = d.id
                ORDER BY rd.last_accessed_at DESC, rd.rowid DESC
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
        // Exercise the same schema as the application, including future migrations.
        sqlx::migrate!("./migrations").run(pool).await.unwrap();
        sqlx::query(
            r#"
            WITH seed(id, path, name, mime, size) AS (
                VALUES ('doc1', '/test/doc1.pdf', 'doc1.pdf', 'application/pdf', 1024),
                    ('doc2', '/test/doc2.txt', 'doc2.txt', 'text/plain', 2048)
            )
            INSERT INTO documents (id, file_path, file_name, mime_type, size_bytes,
                modified_at, indexed_at, checksum, status)
            SELECT id, path, name, mime, size, '2026-09-15T00:00:00Z',
                '2026-09-15T00:00:00Z', 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa', 'indexed'
            FROM seed
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

        let recent = repo.get_recent_documents(10).await.unwrap();
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].document_id, "doc1");
        assert_eq!(recent[0].access_count, 1);
    }

    #[tokio::test]
    async fn tracking_and_eviction_roll_back_together() {
        let pool = create_test_pool().await;
        setup_schema(&pool).await;
        sqlx::raw_sql("WITH RECURSIVE n(x) AS (SELECT 1 UNION ALL SELECT x+1 FROM n WHERE x<51)
            INSERT INTO documents (id, file_path, file_name, size_bytes, modified_at, checksum) SELECT 'old-'||x, '/test/old-'||x, 'old', 0, '2020-01-01T00:00:00Z', 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa' FROM n;
            INSERT INTO recent_documents (id, document_id, last_accessed_at, access_count)
                SELECT id, id, '2020-01-01', 1 FROM documents WHERE id LIKE 'old-%';
            CREATE TRIGGER reject_eviction BEFORE DELETE ON recent_documents BEGIN SELECT RAISE(ABORT, 'eviction failure'); END;")
            .execute(&pool).await.unwrap();
        let repo = RecentDocumentsRepository::new(pool.clone());
        assert!(repo.track_access("doc1").await.is_err());
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM recent_documents")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 51);
        let tracked: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM recent_documents WHERE document_id='doc1'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(tracked, 0);
        sqlx::query("DROP TRIGGER reject_eviction")
            .execute(&pool)
            .await
            .unwrap();
        repo.track_access("doc1").await.unwrap();
        let records = repo.get_recent_documents(usize::MAX).await.unwrap();
        assert_eq!(records.len(), 50);
        assert_eq!(records[0].document_id, "doc1");
        assert!(!records[0].id.is_empty());
        repo.track_access("doc1").await.unwrap();
        let tracked = repo.get_recent_documents(1).await.unwrap();
        assert_eq!(tracked[0].id, records[0].id);
        assert_eq!(tracked[0].access_count, 2);
    }

    #[tokio::test]
    async fn unknown_document_is_not_tracked() {
        let pool = create_test_pool().await;
        setup_schema(&pool).await;
        let repo = RecentDocumentsRepository::new(pool);
        assert!(matches!(
            repo.track_access("missing").await,
            Err(AppError::NotFound(_))
        ));
        assert!(repo.get_recent_documents(10).await.unwrap().is_empty());
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

        let recent = repo.get_recent_documents(10).await.unwrap();
        assert_eq!(recent.len(), 2);

        // Clear all history
        let cleared = repo.clear_recent_history(None).await.unwrap();
        assert_eq!(cleared, 2);

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

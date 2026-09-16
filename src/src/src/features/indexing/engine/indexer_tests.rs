//! Comprehensive tests for document indexing
//!
//! Tests include:
//! - Transaction rollback on error
//! - Concurrent indexing
//! - Duplicate file handling
//! - Metadata extraction
//! - Progress tracking
//!
//! These run against the **real** schema, produced by the migrations in
//! `migrations/`. An earlier version of this file hand-rolled its own tables
//! (`files`, `chunks`) that exist nowhere in the application. Those tests
//! passed while verifying nothing about production behaviour — and two of
//! them referenced `text_chunks`, a table their own fixture never created,
//! so they failed outright. Fixtures that invent a schema are worse than no
//! fixtures: they report success for code paths that cannot work.

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]
mod tests {
    use sqlx::SqlitePool;
    use std::fs;
    use std::path::PathBuf;
    use tempfile::TempDir;

    // ============================================================================
    // Helper Functions
    // ============================================================================

    async fn setup_test_db() -> (SqlitePool, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");

        let pool = SqlitePool::connect(&format!("sqlite:{}?mode=rwc", db_path.display()))
            .await
            .unwrap();

        // Production enables foreign keys (database/connection.rs); without
        // it these tests would not observe cascade or constraint behaviour.
        sqlx::query("PRAGMA foreign_keys = ON")
            .execute(&pool)
            .await
            .unwrap();

        sqlx::migrate!("./migrations").run(&pool).await.unwrap();

        (pool, temp_dir)
    }

    fn create_test_file(dir: &std::path::Path, name: &str, content: &str) -> PathBuf {
        let file_path = dir.join(name);
        fs::write(&file_path, content).unwrap();
        file_path
    }

    /// Insert a document row. Mirrors the column set the indexing engine
    /// actually writes, so a schema change breaks these tests rather than
    /// letting them drift into fiction again.
    async fn insert_document<'e, E>(
        executor: E,
        id: &str,
        path: &str,
        name: &str,
        status: &str,
    ) -> sqlx::Result<()>
    where
        E: sqlx::Executor<'e, Database = sqlx::Sqlite>,
    {
        sqlx::query(
            "INSERT INTO documents (id, file_path, file_name, mime_type, size_bytes, \
             modified_at, indexed_at, checksum, status) \
             VALUES (?, ?, ?, 'text/plain', 100, '2026-01-01T00:00:00Z', \
             '2026-01-01T00:00:00Z', 'deadbeef', ?)",
        )
        .bind(id)
        .bind(path)
        .bind(name)
        .bind(status)
        .execute(executor)
        .await
        .map(|_| ())
    }

    async fn count(pool: &SqlitePool, sql: &str) -> i64 {
        sqlx::query_scalar::<_, i64>(sql)
            .fetch_one(pool)
            .await
            .unwrap()
    }

    // ============================================================================
    // Transaction Rollback Tests
    // ============================================================================

    #[tokio::test]
    async fn test_indexing_transaction_rollback_on_error() {
        let (pool, temp_dir) = setup_test_db().await;
        let file_path = create_test_file(temp_dir.path(), "test.txt", "Test content");

        let mut tx = pool.begin().await.unwrap();

        insert_document(
            &mut *tx,
            "doc_1",
            file_path.to_str().unwrap(),
            "test.txt",
            "pending",
        )
        .await
        .unwrap();

        sqlx::query(
            "INSERT INTO text_chunks (id, document_id, content, chunk_index, start_char, end_char) \
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind("chunk_1")
        .bind("doc_1")
        .bind("Chunk 1")
        .bind(0)
        .bind(0)
        .bind(7)
        .execute(&mut *tx)
        .await
        .unwrap();

        tx.rollback().await.unwrap();

        assert_eq!(
            count(&pool, "SELECT COUNT(*) FROM documents").await,
            0,
            "Document should not exist after rollback"
        );
        assert_eq!(
            count(&pool, "SELECT COUNT(*) FROM text_chunks").await,
            0,
            "Chunks should not exist after rollback"
        );
    }

    #[tokio::test]
    async fn test_indexing_transaction_commit() {
        let (pool, temp_dir) = setup_test_db().await;
        let file_path = create_test_file(temp_dir.path(), "test.txt", "Test content");

        let mut tx = pool.begin().await.unwrap();

        insert_document(
            &mut *tx,
            "doc_1",
            file_path.to_str().unwrap(),
            "test.txt",
            "indexed",
        )
        .await
        .unwrap();

        for i in 0..5 {
            sqlx::query(
                "INSERT INTO text_chunks (id, document_id, content, chunk_index, start_char, end_char) \
                 VALUES (?, ?, ?, ?, ?, ?)",
            )
            .bind(format!("chunk_{}", i))
            .bind("doc_1")
            .bind(format!("Chunk {}", i))
            .bind(i)
            .bind(0)
            .bind(10)
            .execute(&mut *tx)
            .await
            .unwrap();
        }

        tx.commit().await.unwrap();

        assert_eq!(
            count(
                &pool,
                "SELECT COUNT(*) FROM documents WHERE status = 'indexed'"
            )
            .await,
            1
        );
        assert_eq!(count(&pool, "SELECT COUNT(*) FROM text_chunks").await, 5);
    }

    /// Chunks must not outlive their document. Production relies on this
    /// cascade when a document is removed.
    #[tokio::test]
    async fn test_deleting_document_cascades_to_chunks() {
        let (pool, _temp_dir) = setup_test_db().await;

        insert_document(&pool, "doc_1", "/tmp/a.txt", "a.txt", "indexed")
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO text_chunks (id, document_id, content, chunk_index, start_char, end_char) \
             VALUES ('chunk_1', 'doc_1', 'body', 0, 0, 4)",
        )
        .execute(&pool)
        .await
        .unwrap();

        sqlx::query("DELETE FROM documents WHERE id = 'doc_1'")
            .execute(&pool)
            .await
            .unwrap();

        assert_eq!(
            count(&pool, "SELECT COUNT(*) FROM text_chunks").await,
            0,
            "chunks must be removed with their document"
        );
    }

    /// A chunk referencing a nonexistent document must be refused. This is
    /// the constraint that the re-index id bug used to trip on every edit.
    #[tokio::test]
    async fn test_chunk_with_unknown_document_is_rejected() {
        let (pool, _temp_dir) = setup_test_db().await;

        let result = sqlx::query(
            "INSERT INTO text_chunks (id, document_id, content, chunk_index, start_char, end_char) \
             VALUES ('chunk_1', 'no-such-doc', 'body', 0, 0, 4)",
        )
        .execute(&pool)
        .await;

        assert!(
            result.is_err(),
            "foreign keys must reject a chunk with no parent document"
        );
    }

    // ============================================================================
    // Concurrent Indexing Tests
    // ============================================================================

    #[tokio::test]
    async fn test_concurrent_file_indexing() {
        let (pool, temp_dir) = setup_test_db().await;

        let file_paths: Vec<_> = (0..10)
            .map(|i| {
                create_test_file(
                    temp_dir.path(),
                    &format!("file_{}.txt", i),
                    &format!("Content {}", i),
                )
            })
            .collect();

        let mut handles = Vec::new();
        for (i, path) in file_paths.into_iter().enumerate() {
            let pool = pool.clone();
            handles.push(tokio::spawn(async move {
                insert_document(
                    &pool,
                    &format!("doc_{}", i),
                    path.to_str().unwrap(),
                    &format!("file_{}.txt", i),
                    "indexed",
                )
                .await
            }));
        }

        for handle in handles {
            handle.await.unwrap().unwrap();
        }

        assert_eq!(
            count(
                &pool,
                "SELECT COUNT(*) FROM documents WHERE status = 'indexed'"
            )
            .await,
            10
        );
    }

    #[tokio::test]
    async fn test_duplicate_file_path_rejected() {
        let (pool, _temp_dir) = setup_test_db().await;

        insert_document(&pool, "doc_1", "/tmp/same.txt", "same.txt", "indexed")
            .await
            .unwrap();

        let result = insert_document(&pool, "doc_2", "/tmp/same.txt", "same.txt", "indexed").await;

        assert!(
            result.is_err(),
            "file_path is UNIQUE; a second row for the same path must be refused"
        );
    }

    // ============================================================================
    // Status and Metadata Tests
    // ============================================================================

    #[tokio::test]
    async fn test_update_existing_file_index_status() {
        let (pool, _temp_dir) = setup_test_db().await;

        insert_document(&pool, "doc_1", "/tmp/a.txt", "a.txt", "pending")
            .await
            .unwrap();

        sqlx::query("UPDATE documents SET status = 'indexed' WHERE id = 'doc_1'")
            .execute(&pool)
            .await
            .unwrap();

        let status: String = sqlx::query_scalar("SELECT status FROM documents WHERE id = 'doc_1'")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(status, "indexed");
    }

    #[tokio::test]
    async fn test_file_metadata_storage() {
        let (pool, _temp_dir) = setup_test_db().await;

        insert_document(&pool, "doc_1", "/tmp/report.pdf", "report.pdf", "indexed")
            .await
            .unwrap();

        let (name, mime, size): (String, String, i64) = sqlx::query_as(
            "SELECT file_name, mime_type, size_bytes FROM documents WHERE id = 'doc_1'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();

        assert_eq!(name, "report.pdf");
        assert_eq!(mime, "text/plain");
        assert_eq!(size, 100);
    }

    #[tokio::test]
    async fn test_failed_indexing_tracking() {
        let (pool, _temp_dir) = setup_test_db().await;

        insert_document(&pool, "doc_1", "/tmp/broken.bin", "broken.bin", "failed")
            .await
            .unwrap();
        insert_document(&pool, "doc_2", "/tmp/ok.txt", "ok.txt", "indexed")
            .await
            .unwrap();

        assert_eq!(
            count(
                &pool,
                "SELECT COUNT(*) FROM documents WHERE status = 'failed'"
            )
            .await,
            1
        );
    }
}

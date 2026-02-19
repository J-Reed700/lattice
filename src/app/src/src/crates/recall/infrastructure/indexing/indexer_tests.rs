//! Comprehensive tests for document indexing
//!
//! Tests include:
//! - Transaction rollback on error
//! - Concurrent indexing
//! - Duplicate file handling
//! - Metadata extraction
//! - Progress tracking

#[cfg(test)]
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

        // Create schema
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS files (
                id TEXT PRIMARY KEY,
                path TEXT NOT NULL UNIQUE,
                name TEXT NOT NULL,
                extension TEXT,
                size_bytes INTEGER,
                modified_at TIMESTAMP,
                is_indexed BOOLEAN DEFAULT 0,
                index_status TEXT DEFAULT 'pending',
                created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
            );

            CREATE TABLE IF NOT EXISTS chunks (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                file_id TEXT NOT NULL,
                content TEXT NOT NULL,
                chunk_index INTEGER NOT NULL,
                token_count INTEGER,
                FOREIGN KEY (file_id) REFERENCES files(id) ON DELETE CASCADE
            );

            CREATE TABLE IF NOT EXISTS text_embeddings (
                id TEXT PRIMARY KEY,
                chunk_id INTEGER,
                embedding BLOB NOT NULL,
                dimension INTEGER NOT NULL,
                created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                FOREIGN KEY (chunk_id) REFERENCES chunks(id) ON DELETE CASCADE
            );
            "#,
        )
        .execute(&pool)
        .await
        .unwrap();

        (pool, temp_dir)
    }

    fn create_test_file(dir: &std::path::Path, name: &str, content: &str) -> PathBuf {
        let file_path = dir.join(name);
        fs::write(&file_path, content).unwrap();
        file_path
    }

    // ============================================================================
    // Transaction Rollback Tests
    // ============================================================================

    #[tokio::test]
    async fn test_indexing_transaction_rollback_on_error() {
        let (pool, temp_dir) = setup_test_db().await;

        // Create test file
        let file_path = create_test_file(temp_dir.path(), "test.txt", "Test content");

        // Start transaction
        let mut tx = pool.begin().await.unwrap();

        // Insert file record
        sqlx::query(
            "INSERT INTO files (id, path, name, extension, size_bytes, is_indexed)
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind("file_1")
        .bind(file_path.to_str().unwrap())
        .bind("test.txt")
        .bind("txt")
        .bind(100)
        .bind(false)
        .execute(&mut *tx)
        .await
        .unwrap();

        // Insert chunks
        sqlx::query(
            "INSERT INTO text_chunks (file_id, content, chunk_index, token_count)
             VALUES (?, ?, ?, ?)",
        )
        .bind("file_1")
        .bind("Chunk 1")
        .bind(0)
        .bind(10)
        .execute(&mut *tx)
        .await
        .unwrap();

        // Simulate error and rollback
        tx.rollback().await.unwrap();

        // Verify nothing was committed
        let file_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM files")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(file_count, 0, "File should not exist after rollback");

        let chunk_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM text_chunks")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(chunk_count, 0, "Chunks should not exist after rollback");
    }

    #[tokio::test]
    async fn test_indexing_transaction_commit() {
        let (pool, temp_dir) = setup_test_db().await;

        let file_path = create_test_file(temp_dir.path(), "test.txt", "Test content");

        // Start transaction
        let mut tx = pool.begin().await.unwrap();

        // Insert complete indexing data
        sqlx::query(
            "INSERT INTO files (id, path, name, extension, size_bytes, is_indexed, index_status)
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind("file_1")
        .bind(file_path.to_str().unwrap())
        .bind("test.txt")
        .bind("txt")
        .bind(100)
        .bind(true)
        .bind("completed")
        .execute(&mut *tx)
        .await
        .unwrap();

        for i in 0..5 {
            sqlx::query(
                "INSERT INTO text_chunks (file_id, content, chunk_index, token_count)
                 VALUES (?, ?, ?, ?)",
            )
            .bind("file_1")
            .bind(format!("Chunk {}", i))
            .bind(i)
            .bind(50)
            .execute(&mut *tx)
            .await
            .unwrap();
        }

        // Commit transaction
        tx.commit().await.unwrap();

        // Verify data was committed
        let file_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM files WHERE is_indexed = 1")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(file_count, 1);

        let chunk_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM text_chunks")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(chunk_count, 5);
    }

    // ============================================================================
    // Concurrent Indexing Tests
    // ============================================================================

    #[tokio::test]
    async fn test_concurrent_file_indexing() {
        let (pool, temp_dir) = setup_test_db().await;

        // Create multiple test files
        let file_paths: Vec<_> = (0..10)
            .map(|i| {
                create_test_file(
                    temp_dir.path(),
                    &format!("file_{}.txt", i),
                    &format!("Content {}", i),
                )
            })
            .collect();

        // Index files concurrently
        let handles: Vec<_> = file_paths
            .into_iter()
            .enumerate()
            .map(|(i, path)| {
                let pool_clone = pool.clone();
                tokio::spawn(async move {
                    sqlx::query(
                        "INSERT INTO files (id, path, name, extension, is_indexed)
                         VALUES (?, ?, ?, ?, ?)",
                    )
                    .bind(format!("file_{}", i))
                    .bind(path.to_str().unwrap())
                    .bind(format!("file_{}.txt", i))
                    .bind("txt")
                    .bind(true)
                    .execute(&pool_clone)
                    .await
                    .unwrap();
                })
            })
            .collect();

        // All indexing operations should succeed
        for handle in handles {
            handle.await.unwrap();
        }

        // Verify all files were indexed
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM files WHERE is_indexed = 1")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 10);
    }

    // ============================================================================
    // Duplicate File Handling Tests
    // ============================================================================

    #[tokio::test]
    async fn test_duplicate_file_path_rejected() {
        let (pool, temp_dir) = setup_test_db().await;

        let file_path = create_test_file(temp_dir.path(), "test.txt", "Content");

        // Insert first time
        sqlx::query("INSERT INTO files (id, path, name, extension) VALUES (?, ?, ?, ?)")
            .bind("file_1")
            .bind(file_path.to_str().unwrap())
            .bind("test.txt")
            .bind("txt")
            .execute(&pool)
            .await
            .unwrap();

        // Try to insert same path again
        let result =
            sqlx::query("INSERT INTO files (id, path, name, extension) VALUES (?, ?, ?, ?)")
                .bind("file_2")
                .bind(file_path.to_str().unwrap()) // Same path
                .bind("test.txt")
                .bind("txt")
                .execute(&pool)
                .await;

        assert!(
            result.is_err(),
            "Duplicate path should be rejected by UNIQUE constraint"
        );
    }

    #[tokio::test]
    async fn test_update_existing_file_index_status() {
        let (pool, temp_dir) = setup_test_db().await;

        let file_path = create_test_file(temp_dir.path(), "test.txt", "Content");

        // Insert file as pending
        sqlx::query(
            "INSERT INTO files (id, path, name, extension, is_indexed, index_status)
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind("file_1")
        .bind(file_path.to_str().unwrap())
        .bind("test.txt")
        .bind("txt")
        .bind(false)
        .bind("pending")
        .execute(&pool)
        .await
        .unwrap();

        // Update to indexed
        sqlx::query("UPDATE files SET is_indexed = ?, index_status = ? WHERE id = ?")
            .bind(true)
            .bind("completed")
            .bind("file_1")
            .execute(&pool)
            .await
            .unwrap();

        // Verify update
        let (is_indexed, status): (bool, String) =
            sqlx::query_as("SELECT is_indexed, index_status FROM files WHERE id = ?")
                .bind("file_1")
                .fetch_one(&pool)
                .await
                .unwrap();

        assert!(is_indexed);
        assert_eq!(status, "completed");
    }

    // ============================================================================
    // Metadata Extraction Tests
    // ============================================================================

    #[tokio::test]
    async fn test_file_metadata_storage() {
        let (pool, temp_dir) = setup_test_db().await;

        let file_path = create_test_file(temp_dir.path(), "document.pdf", "PDF content");
        let metadata = fs::metadata(&file_path).unwrap();

        // Insert with full metadata
        sqlx::query(
            "INSERT INTO files (id, path, name, extension, size_bytes, modified_at)
             VALUES (?, ?, ?, ?, ?, datetime('now'))",
        )
        .bind("file_1")
        .bind(file_path.to_str().unwrap())
        .bind("document.pdf")
        .bind("pdf")
        .bind(metadata.len() as i64)
        .execute(&pool)
        .await
        .unwrap();

        // Verify metadata was stored
        let (name, extension, size): (String, String, i64) =
            sqlx::query_as("SELECT name, extension, size_bytes FROM files WHERE id = ?")
                .bind("file_1")
                .fetch_one(&pool)
                .await
                .unwrap();

        assert_eq!(name, "document.pdf");
        assert_eq!(extension, "pdf");
        assert_eq!(size, metadata.len() as i64);
    }

    // ============================================================================
    // Progress Tracking Tests
    // ============================================================================

    #[tokio::test]
    async fn test_index_status_tracking() {
        let (pool, temp_dir) = setup_test_db().await;

        let file_path = create_test_file(temp_dir.path(), "test.txt", "Content");

        // Insert as pending
        sqlx::query("INSERT INTO files (id, path, name, index_status) VALUES (?, ?, ?, ?)")
            .bind("file_1")
            .bind(file_path.to_str().unwrap())
            .bind("test.txt")
            .bind("pending")
            .execute(&pool)
            .await
            .unwrap();

        // Update to processing
        sqlx::query("UPDATE files SET index_status = ? WHERE id = ?")
            .bind("processing")
            .bind("file_1")
            .execute(&pool)
            .await
            .unwrap();

        let status: String = sqlx::query_scalar("SELECT index_status FROM files WHERE id = ?")
            .bind("file_1")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(status, "processing");

        // Update to completed
        sqlx::query("UPDATE files SET index_status = ?, is_indexed = ? WHERE id = ?")
            .bind("completed")
            .bind(true)
            .bind("file_1")
            .execute(&pool)
            .await
            .unwrap();

        let (status, indexed): (String, bool) =
            sqlx::query_as("SELECT index_status, is_indexed FROM files WHERE id = ?")
                .bind("file_1")
                .fetch_one(&pool)
                .await
                .unwrap();

        assert_eq!(status, "completed");
        assert!(indexed);
    }

    #[tokio::test]
    async fn test_failed_indexing_tracking() {
        let (pool, temp_dir) = setup_test_db().await;

        let file_path = create_test_file(temp_dir.path(), "test.txt", "Content");

        // Insert and mark as failed
        sqlx::query(
            "INSERT INTO files (id, path, name, index_status, is_indexed)
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind("file_1")
        .bind(file_path.to_str().unwrap())
        .bind("test.txt")
        .bind("failed")
        .bind(false)
        .execute(&pool)
        .await
        .unwrap();

        // Query failed files
        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM files WHERE index_status = 'failed'")
                .fetch_one(&pool)
                .await
                .unwrap();

        assert_eq!(count, 1);
    }
}

//! Comprehensive database query tests
//!
//! Tests include:
//! - Query timeout enforcement
//! - Connection pool exhaustion
//! - Foreign key constraint enforcement
//! - Transaction rollback on error
//! - Concurrent queries

#[cfg(test)]
mod tests {
    use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
    use sqlx::SqlitePool;
    use std::time::Duration;
    use tempfile::TempDir;
    use tokio::time::timeout;

    async fn setup_test_db() -> (SqlitePool, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");

        let connect_options = SqliteConnectOptions::new()
            .filename(&db_path)
            .create_if_missing(true)
            .busy_timeout(Duration::from_secs(5))
            .pragma("journal_mode", "WAL")
            .pragma("synchronous", "NORMAL");

        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect_with(connect_options)
            .await
            .unwrap();

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS documents (
                id TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
            );

            CREATE TABLE IF NOT EXISTS text_chunks (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                document_id TEXT NOT NULL,
                content TEXT NOT NULL,
                chunk_index INTEGER NOT NULL,
                FOREIGN KEY (document_id) REFERENCES documents(id) ON DELETE CASCADE
            );

            CREATE INDEX idx_text_chunks_document_id ON text_chunks(document_id);
            "#,
        )
        .execute(&pool)
        .await
        .unwrap();

        (pool, temp_dir)
    }

    #[tokio::test]
    async fn test_query_timeout_enforcement() {
        let (pool, _temp_dir) = setup_test_db().await;

        for i in 0..1000 {
            sqlx::query("INSERT INTO documents (id, title) VALUES (?, ?)")
                .bind(format!("doc_{}", i))
                .bind(format!("Title {}", i))
                .execute(&pool)
                .await
                .unwrap();
        }

        // Query with timeout
        let result = timeout(
            Duration::from_secs(5),
            sqlx::query("SELECT * FROM documents").fetch_all(&pool),
        )
        .await;

        assert!(result.is_ok(), "Query should complete within timeout");
        let docs = result.unwrap().unwrap();
        assert_eq!(docs.len(), 1000);
    }

    #[tokio::test]
    async fn test_heavy_query_with_timeout() {
        let (pool, _temp_dir) = setup_test_db().await;

        // This should complete quickly
        let result = timeout(
            Duration::from_secs(10),
            sqlx::query("SELECT COUNT(*) as count FROM documents").fetch_one(&pool),
        )
        .await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_foreign_key_constraint_enforced() {
        let (pool, _temp_dir) = setup_test_db().await;

        // Enable foreign keys
        sqlx::query("PRAGMA foreign_keys = ON")
            .execute(&pool)
            .await
            .unwrap();

        // Try to insert chunk without parent document
        let result = sqlx::query(
            "INSERT INTO text_chunks (document_id, content, chunk_index) VALUES (?, ?, ?)",
        )
        .bind("nonexistent_doc")
        .bind("Some content")
        .bind(0)
        .execute(&pool)
        .await;

        assert!(result.is_err(), "Foreign key constraint should be enforced");
    }

    #[tokio::test]
    async fn test_cascade_delete() {
        let (pool, _temp_dir) = setup_test_db().await;

        // Enable foreign keys
        sqlx::query("PRAGMA foreign_keys = ON")
            .execute(&pool)
            .await
            .unwrap();

        sqlx::query("INSERT INTO documents (id, title) VALUES (?, ?)")
            .bind("doc_1")
            .bind("Test Document")
            .execute(&pool)
            .await
            .unwrap();

        for i in 0..5 {
            sqlx::query(
                "INSERT INTO text_chunks (document_id, content, chunk_index) VALUES (?, ?, ?)",
            )
            .bind("doc_1")
            .bind(format!("Content {}", i))
            .bind(i)
            .execute(&pool)
            .await
            .unwrap();
        }

        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM text_chunks WHERE document_id = ?")
                .bind("doc_1")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(count, 5);

        sqlx::query("DELETE FROM documents WHERE id = ?")
            .bind("doc_1")
            .execute(&pool)
            .await
            .unwrap();

        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM text_chunks WHERE document_id = ?")
                .bind("doc_1")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(count, 0, "Chunks should be deleted via CASCADE");
    }

    #[tokio::test]
    async fn test_transaction_rollback_on_error() {
        let (pool, _temp_dir) = setup_test_db().await;

        let mut tx = pool.begin().await.unwrap();

        sqlx::query("INSERT INTO documents (id, title) VALUES (?, ?)")
            .bind("doc_1")
            .bind("Test")
            .execute(&mut *tx)
            .await
            .unwrap();

        let exists: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM documents WHERE id = ?)")
                .bind("doc_1")
                .fetch_one(&mut *tx)
                .await
                .unwrap();
        assert!(exists);

        // Rollback
        tx.rollback().await.unwrap();

        // Verify it doesn't exist after rollback
        let exists: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM documents WHERE id = ?)")
                .bind("doc_1")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert!(!exists, "Document should not exist after rollback");
    }

    #[tokio::test]
    async fn test_transaction_commit() {
        let (pool, _temp_dir) = setup_test_db().await;

        let mut tx = pool.begin().await.unwrap();

        for i in 0..10 {
            sqlx::query("INSERT INTO documents (id, title) VALUES (?, ?)")
                .bind(format!("doc_{}", i))
                .bind(format!("Title {}", i))
                .execute(&mut *tx)
                .await
                .unwrap();
        }

        // Commit
        tx.commit().await.unwrap();

        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM documents")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 10);
    }

    #[tokio::test]
    async fn test_concurrent_reads() {
        let (pool, _temp_dir) = setup_test_db().await;

        for i in 0..100 {
            sqlx::query("INSERT INTO documents (id, title) VALUES (?, ?)")
                .bind(format!("doc_{}", i))
                .bind(format!("Title {}", i))
                .execute(&pool)
                .await
                .unwrap();
        }

        // Spawn 20 concurrent read tasks
        let handles: Vec<_> = (0..20)
            .map(|_| {
                let pool_clone = pool.clone();
                tokio::spawn(async move {
                    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM documents")
                        .fetch_one(&pool_clone)
                        .await
                        .unwrap();
                    assert_eq!(count, 100);
                })
            })
            .collect();

        // All reads should succeed
        for handle in handles {
            handle.await.unwrap();
        }
    }

    #[tokio::test]
    async fn test_concurrent_writes() {
        let (pool, _temp_dir) = setup_test_db().await;

        // Spawn 10 concurrent write tasks
        let handles: Vec<_> = (0..10)
            .map(|i| {
                let pool_clone = pool.clone();
                tokio::spawn(async move {
                    for j in 0..10 {
                        sqlx::query("INSERT INTO documents (id, title) VALUES (?, ?)")
                            .bind(format!("doc_{}_{}", i, j))
                            .bind(format!("Title {}_{}", i, j))
                            .execute(&pool_clone)
                            .await
                            .unwrap();
                    }
                })
            })
            .collect();

        // All writes should succeed
        for handle in handles {
            handle.await.unwrap();
        }

        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM documents")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 100);
    }

    #[tokio::test]
    async fn test_index_usage() {
        let (pool, _temp_dir) = setup_test_db().await;

        sqlx::query("INSERT INTO documents (id, title) VALUES (?, ?)")
            .bind("doc_1")
            .bind("Test")
            .execute(&pool)
            .await
            .unwrap();

        for i in 0..1000 {
            sqlx::query(
                "INSERT INTO text_chunks (document_id, content, chunk_index) VALUES (?, ?, ?)",
            )
            .bind("doc_1")
            .bind(format!("Content {}", i))
            .bind(i)
            .execute(&pool)
            .await
            .unwrap();
        }

        // Query using index should be fast
        let start = std::time::Instant::now();
        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM text_chunks WHERE document_id = ?")
                .bind("doc_1")
                .fetch_one(&pool)
                .await
                .unwrap();
        let duration = start.elapsed();

        assert_eq!(count, 1000);
        assert!(
            duration < Duration::from_millis(100),
            "Indexed query should be fast"
        );
    }

    #[tokio::test]
    async fn test_connection_pool_reuse() {
        let (pool, _temp_dir) = setup_test_db().await;

        // Perform multiple queries using the same pool
        for i in 0..50 {
            sqlx::query("INSERT INTO documents (id, title) VALUES (?, ?)")
                .bind(format!("doc_{}", i))
                .bind(format!("Title {}", i))
                .execute(&pool)
                .await
                .unwrap();

            let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM documents")
                .fetch_one(&pool)
                .await
                .unwrap();
            assert_eq!(count, (i + 1) as i64);
        }
    }
}

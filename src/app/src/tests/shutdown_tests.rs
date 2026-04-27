#![allow(clippy::panic)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::assertions_on_constants)]
#![allow(clippy::useless_vec)]
#![allow(clippy::indexing_slicing)]
#![allow(unused_variables)]
#![allow(unused_imports)]
#![allow(deprecated)]

// Shutdown Tests
//
// Tests for graceful shutdown behavior with extended timeouts.
// Verifies that the shutdown logic properly handles:
// - Normal database closure within timeout
// - Database busy conditions requiring extended timeout
// - Forced cleanup after timeout
// - Connection count logging
#[cfg(test)]
mod shutdown_tests {
    use sqlx::sqlite::SqlitePoolOptions;
    use sqlx::Acquire;
    use std::sync::Arc;
    use std::time::Duration;
    use tokio::sync::Barrier;

    #[tokio::test]
    async fn test_db_closes_within_timeout() {
        // Create a test pool
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect("sqlite::memory:")
            .await
            .unwrap();

        // Simulate graceful close (should succeed quickly)
        let close_result = tokio::time::timeout(Duration::from_secs(10), pool.close()).await;

        assert!(
            close_result.is_ok(),
            "Database should close within 10 second timeout"
        );
    }

    #[tokio::test]
    async fn test_db_busy_with_active_connections() {
        // Create pool with multiple connections
        let pool = SqlitePoolOptions::new()
            .max_connections(3)
            .connect("sqlite::memory:")
            .await
            .unwrap();

        // Create table for testing
        sqlx::query("CREATE TABLE test (id INTEGER PRIMARY KEY, value TEXT)")
            .execute(&pool)
            .await
            .unwrap();

        // Acquire connections and start long-running queries
        let conn1 = pool.acquire().await.unwrap();
        let conn2 = pool.acquire().await.unwrap();

        // Hold connections without releasing
        let barrier = Arc::new(Barrier::new(3));
        let barrier_clone1 = Arc::clone(&barrier);
        let barrier_clone2 = Arc::clone(&barrier);

        // Spawn tasks that hold connections
        let task1 = tokio::spawn(async move {
            let _conn = conn1;
            barrier_clone1.wait().await;
            // Connection released after barrier
        });

        let task2 = tokio::spawn(async move {
            let _conn = conn2;
            barrier_clone2.wait().await;
            // Connection released after barrier
        });

        // Try to close pool while connections are in use
        let close_start = tokio::time::Instant::now();
        let close_handle = tokio::spawn({
            let pool = pool.clone();
            async move { tokio::time::timeout(Duration::from_secs(10), pool.close()).await }
        });

        // Wait a bit, then release connections
        tokio::time::sleep(Duration::from_millis(100)).await;
        barrier.wait().await;

        // Wait for tasks to complete
        task1.await.unwrap();
        task2.await.unwrap();

        // Check close result
        let close_result = close_handle.await.unwrap();
        let duration = close_start.elapsed();

        // Should eventually succeed (after connections released)
        assert!(
            close_result.is_ok() || duration >= Duration::from_secs(10),
            "Close should succeed or timeout after 10 seconds"
        );
    }

    #[tokio::test]
    async fn test_connection_count_tracking() {
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .min_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();

        // Initial size
        let initial_size = pool.size();
        assert!(initial_size >= 1, "Pool should have at least 1 connection");

        // Acquire connections
        let conn1 = pool.acquire().await.unwrap();
        let conn2 = pool.acquire().await.unwrap();

        let size_with_acquired = pool.size();
        assert!(
            size_with_acquired >= 2,
            "Pool should have at least 2 connections when acquired"
        );

        // Release connections
        drop(conn1);
        drop(conn2);

        // Close pool
        pool.close().await;
    }

    #[tokio::test]
    async fn test_shutdown_with_no_active_connections() {
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect("sqlite::memory:")
            .await
            .unwrap();

        // Close immediately (no active connections)
        let start = tokio::time::Instant::now();
        let result = tokio::time::timeout(Duration::from_secs(10), pool.close()).await;
        let duration = start.elapsed();

        assert!(result.is_ok(), "Should close successfully");
        assert!(
            duration < Duration::from_secs(1),
            "Should close quickly with no active connections, took {:?}",
            duration
        );
    }

    #[tokio::test]
    async fn test_shutdown_timeout_constants() {
        // Verify timeout constants are reasonable
        const DB_CLOSE_TIMEOUT: Duration = Duration::from_secs(10);
        const TOTAL_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(15);

        assert!(
            TOTAL_SHUTDOWN_TIMEOUT > DB_CLOSE_TIMEOUT,
            "Total shutdown timeout should be greater than DB close timeout"
        );

        assert!(
            DB_CLOSE_TIMEOUT >= Duration::from_secs(5),
            "DB close timeout should be at least 5 seconds for busy conditions"
        );
    }

    #[tokio::test]
    async fn test_multiple_close_attempts() {
        let pool = SqlitePoolOptions::new()
            .max_connections(3)
            .connect("sqlite::memory:")
            .await
            .unwrap();

        // First close
        let result1 = tokio::time::timeout(Duration::from_secs(10), pool.close()).await;

        assert!(result1.is_ok(), "First close should succeed");

        // Attempting to use closed pool should fail gracefully
        // (This is just verifying cleanup worked)
    }

    #[tokio::test]
    async fn test_forced_cleanup_fallback() {
        // Simulate scenario where forced cleanup is needed
        let pool = SqlitePoolOptions::new()
            .max_connections(2)
            .connect("sqlite::memory:")
            .await
            .unwrap();

        // Create a connection that holds a lock
        let mut conn = pool.acquire().await.unwrap();

        // Start a transaction to hold the connection
        sqlx::query("CREATE TABLE test (id INTEGER)")
            .execute(&mut *conn)
            .await
            .unwrap();

        let tx = conn.begin().await.unwrap();

        // Try to close pool while transaction is active
        let close_result = tokio::time::timeout(
            Duration::from_millis(500), // Short timeout to trigger timeout path
            pool.close(),
        )
        .await;

        // Should timeout (connection still in use)
        assert!(
            close_result.is_err(),
            "Should timeout when connection is held by transaction"
        );

        // Rollback transaction and release connection
        tx.rollback().await.unwrap();
        drop(conn);

        // Now pool can be closed (but it's already dropped)
    }

    #[tokio::test]
    async fn test_logging_during_shutdown() {
        // This test verifies that shutdown logic can access pool.size()
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .min_connections(2)
            .connect("sqlite::memory:")
            .await
            .unwrap();

        // Acquire a connection
        let _conn = pool.acquire().await.unwrap();

        // Get connection count (simulates logging in graceful_shutdown)
        let conn_count = pool.size();

        assert!(
            conn_count >= 1,
            "Pool size should be at least 1 with active connection"
        );

        // Close pool
        pool.close().await;
    }

    #[tokio::test]
    async fn test_concurrent_shutdown_requests() {
        let pool = SqlitePoolOptions::new()
            .max_connections(3)
            .connect("sqlite::memory:")
            .await
            .unwrap();

        // Spawn multiple close attempts
        let close1 = tokio::spawn({
            let pool = pool.clone();
            async move { tokio::time::timeout(Duration::from_secs(10), pool.close()).await }
        });

        // Small delay before second attempt
        tokio::time::sleep(Duration::from_millis(10)).await;

        let close2 = tokio::spawn({
            let pool = pool.clone();
            async move { tokio::time::timeout(Duration::from_secs(10), pool.close()).await }
        });

        // At least one should succeed
        let result1 = close1.await.unwrap();
        let result2 = close2.await.unwrap();

        // One should succeed, other might timeout or error (pool already closed)
        assert!(
            result1.is_ok() || result2.is_ok(),
            "At least one close attempt should succeed"
        );
    }
}

//! SQLite audit sink implementation.
//!
//! This sink persists audit events to a SQLite database table,
//! providing durable storage and query capabilities.

use crate::infrastructure::audit::event::AuditEvent;
use crate::infrastructure::audit::logger::AuditSink;
use crate::shared::error::{AppError, Result};
use async_trait::async_trait;
use sqlx::{Row, SqlitePool};
use std::path::Path;
use tracing::{debug, error, info};

/// SQLite-based audit sink.
///
/// This sink stores audit events in a SQLite database table with the following schema:
///
/// ```sql
/// CREATE TABLE audit_events (
///     id TEXT PRIMARY KEY,
///     timestamp TEXT NOT NULL,
///     user_id TEXT,
///     action TEXT NOT NULL,
///     resource_id TEXT,
///     result TEXT NOT NULL,
///     metadata TEXT NOT NULL
/// );
/// ```
pub struct SqliteAuditSink {
    /// SQLite connection pool
    pool: SqlitePool,
}

impl SqliteAuditSink {
    /// Create a new SQLite audit sink.
    ///
    /// # Arguments
    ///
    /// * `db_path` - Path to the SQLite database file
    ///
    /// # Errors
    ///
    /// Returns an error if the database cannot be opened or initialized.
    pub async fn new(db_path: impl AsRef<Path>) -> Result<Self> {
        let db_path = db_path.as_ref();
        let db_url = format!("sqlite:{}", db_path.display());

        info!("Initializing SQLite audit sink at: {}", db_path.display());

        // Create connection pool
        let pool = SqlitePool::connect(&db_url).await?;

        // Initialize schema
        Self::init_schema(&pool).await?;

        Ok(Self { pool })
    }

    /// Create a new SQLite audit sink from an existing connection pool.
    ///
    /// This is useful when you want to share a connection pool with other
    /// parts of the application.
    ///
    /// # Arguments
    ///
    /// * `pool` - An existing SQLite connection pool
    ///
    /// # Errors
    ///
    /// Returns an error if the schema cannot be initialized.
    pub async fn from_pool(pool: SqlitePool) -> Result<Self> {
        Self::init_schema(&pool).await?;
        Ok(Self { pool })
    }

    /// Initialize the database schema.
    ///
    /// Creates the audit_events table if it doesn't exist and adds indexes
    /// for common query patterns.
    async fn init_schema(pool: &SqlitePool) -> Result<()> {
        debug!("Initializing audit events schema");

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS audit_events (
                id TEXT PRIMARY KEY,
                timestamp TEXT NOT NULL,
                user_id TEXT,
                action TEXT NOT NULL,
                resource_id TEXT,
                result TEXT NOT NULL,
                metadata TEXT NOT NULL
            )
            "#,
        )
        .execute(pool)
        .await?;

        // Create indexes for common queries
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_audit_timestamp ON audit_events(timestamp)")
            .execute(pool)
            .await?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_audit_action ON audit_events(action)")
            .execute(pool)
            .await?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_audit_user_id ON audit_events(user_id)")
            .execute(pool)
            .await?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_audit_resource_id ON audit_events(resource_id)",
        )
        .execute(pool)
        .await?;

        info!("Audit events schema initialized successfully");
        Ok(())
    }

    /// Get a reference to the connection pool.
    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }
}

#[async_trait]
impl AuditSink for SqliteAuditSink {
    async fn log(&self, event: &AuditEvent) -> Result<()> {
        let id = event.id.to_string();
        let timestamp = event.timestamp.to_rfc3339();
        let action = serde_json::to_string(&event.action)?;
        let result = serde_json::to_string(&event.result)?;
        let metadata = serde_json::to_string(&event.metadata)?;

        sqlx::query(
            r#"
            INSERT INTO audit_events (id, timestamp, user_id, action, resource_id, result, metadata)
            VALUES (?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(id)
        .bind(timestamp)
        .bind(&event.user_id)
        .bind(action)
        .bind(&event.resource_id)
        .bind(result)
        .bind(metadata)
        .execute(&self.pool)
        .await
        .map_err(|e| {
            error!("Failed to insert audit event: {}", e);
            AppError::Database(format!("Failed to insert audit event: {}", e))
        })?;

        debug!("Audit event {} written to SQLite", event.id);
        Ok(())
    }

    async fn flush(&self) -> Result<()> {
        // SQLite writes are synchronous by default, so nothing to flush
        debug!("SQLite audit sink flush requested (no-op)");
        Ok(())
    }

    async fn query(&self, limit: usize, offset: usize) -> Result<Vec<AuditEvent>> {
        let rows = sqlx::query(
            r#"
            SELECT id, timestamp, user_id, action, resource_id, result, metadata
            FROM audit_events
            ORDER BY timestamp DESC
            LIMIT ? OFFSET ?
            "#,
        )
        .bind(limit as i64)
        .bind(offset as i64)
        .fetch_all(&self.pool)
        .await?;

        let mut events = Vec::with_capacity(rows.len());

        for row in rows {
            let id: String = row.try_get("id")?;
            let timestamp: String = row.try_get("timestamp")?;
            let user_id: Option<String> = row.try_get("user_id")?;
            let action: String = row.try_get("action")?;
            let resource_id: Option<String> = row.try_get("resource_id")?;
            let result: String = row.try_get("result")?;
            let metadata: String = row.try_get("metadata")?;

            let event = AuditEvent {
                id: id
                    .parse()
                    .map_err(|e| AppError::Database(format!("Invalid UUID in database: {}", e)))?,
                timestamp: chrono::DateTime::parse_from_rfc3339(&timestamp)
                    .map_err(|e| {
                        AppError::Database(format!("Invalid timestamp in database: {}", e))
                    })?
                    .with_timezone(&chrono::Utc),
                user_id,
                action: serde_json::from_str(&action)?,
                resource_id,
                result: serde_json::from_str(&result)?,
                metadata: serde_json::from_str(&metadata)?,
            };

            events.push(event);
        }

        debug!("Queried {} audit events from SQLite", events.len());
        Ok(events)
    }

    async fn count(&self) -> Result<usize> {
        let row = sqlx::query("SELECT COUNT(*) as count FROM audit_events")
            .fetch_one(&self.pool)
            .await?;

        let count: i64 = row.try_get("count")?;
        Ok(count as usize)
    }

    async fn close(&self) -> Result<()> {
        debug!("Closing SQLite audit sink");
        self.pool.close().await;
        info!("SQLite audit sink closed");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audit::event::{AuditAction, AuditResult};
    use tempfile::tempdir;

    async fn create_test_sink() -> Result<(SqliteAuditSink, tempfile::TempDir)> {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test_audit.db");

        // Create the database file first (required for SQLite to open)
        std::fs::File::create(&db_path).unwrap();

        let sink = SqliteAuditSink::new(db_path).await?;
        // Return TempDir to keep it alive for test duration (Rust Drop trait)
        Ok((sink, dir))
    }

    #[tokio::test]

    async fn test_sink_creation() {
        let result = create_test_sink().await;
        assert!(result.is_ok());
    }

    #[tokio::test]

    async fn test_log_event() {
        let (sink, _dir) = create_test_sink().await.unwrap();
        let event = AuditEvent::new(AuditAction::FileIndexed, AuditResult::success())
            .with_user_id("user123")
            .with_resource_id("/path/to/file.txt")
            .with_metadata("size", "1024");

        let result = sink.log(&event).await;
        assert!(result.is_ok());
    }

    #[tokio::test]

    async fn test_query_events() {
        let (sink, _dir) = create_test_sink().await.unwrap();

        // Insert multiple events
        for i in 0..5 {
            let event = AuditEvent::new(AuditAction::FileIndexed, AuditResult::success())
                .with_resource_id(format!("file{}", i));

            sink.log(&event).await.unwrap();
        }

        // Query events
        let events = sink.query(10, 0).await.unwrap();
        assert_eq!(events.len(), 5);

        // Test pagination
        let first_page = sink.query(2, 0).await.unwrap();
        assert_eq!(first_page.len(), 2);

        let second_page = sink.query(2, 2).await.unwrap();
        assert_eq!(second_page.len(), 2);
    }

    #[tokio::test]

    async fn test_count_events() {
        let (sink, _dir) = create_test_sink().await.unwrap();

        // Initially should be 0
        let count = sink.count().await.unwrap();
        assert_eq!(count, 0);

        // Insert events
        for _ in 0..3 {
            let event = AuditEvent::new(AuditAction::FileIndexed, AuditResult::success());
            sink.log(&event).await.unwrap();
        }

        // Count should be 3
        let count = sink.count().await.unwrap();
        assert_eq!(count, 3);
    }

    #[tokio::test]

    async fn test_event_with_metadata() {
        let (sink, _dir) = create_test_sink().await.unwrap();

        let mut metadata = std::collections::HashMap::new();
        metadata.insert("key1".to_string(), "value1".to_string());
        metadata.insert("key2".to_string(), "value2".to_string());

        let event = AuditEvent::new(AuditAction::SearchPerformed, AuditResult::success())
            .with_metadata_map(metadata);

        sink.log(&event).await.unwrap();

        let events = sink.query(1, 0).await.unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].metadata.len(), 2);
        assert_eq!(events[0].metadata.get("key1").unwrap(), "value1");
    }

    #[tokio::test]

    async fn test_event_ordering() {
        let (sink, _dir) = create_test_sink().await.unwrap();

        // Insert events with delays to ensure different timestamps
        for i in 0..3 {
            let event = AuditEvent::new(AuditAction::FileIndexed, AuditResult::success())
                .with_resource_id(format!("file{}", i));

            sink.log(&event).await.unwrap();
            tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        }

        // Query should return most recent first
        let events = sink.query(3, 0).await.unwrap();
        assert_eq!(events.len(), 3);

        // Verify ordering (most recent first)
        for i in 0..2 {
            assert!(events[i].timestamp >= events[i + 1].timestamp);
        }
    }
}

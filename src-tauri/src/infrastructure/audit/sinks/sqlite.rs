//! SQLite audit sink implementation.
//!
//! This sink persists audit events to a SQLite database table,
//! providing durable storage and query capabilities.

use crate::infrastructure::audit::event::AuditEvent;
use crate::infrastructure::audit::logger::AuditSink;
use crate::shared::error::{AppError, Result};
use async_trait::async_trait;
use sqlx::{Row, SqlitePool};
use std::collections::BTreeMap;
use std::path::Path;
use tracing::{debug, error, info};

const MAX_AUDIT_EVENTS: i64 = 10_000;
const RETENTION_DAYS: i64 = 30;
const SAFE_NUMERIC_METADATA: &[&str] = &[
    "context_messages",
    "message_tokens",
    "response_tokens",
    "file_size_bytes",
    "size",
    "results",
    "duration_ms",
    "doc_count",
    "chunk_count",
    "files_indexed",
    "tag_count",
    "tags_count",
    "tagged_count",
    "count",
    "error_code",
    "cancelled_count",
    "chunks",
    "content_length",
    "file_count",
    "file_size",
    "files_exported",
    "files_imported",
    "result_count",
    "rows_exported",
    "total_items",
    "url_count",
    "word_count",
];
const SAFE_BOOLEAN_METADATA: &[&str] = &["available", "file_deleted"];
const SAFE_ENUM_METADATA: &[&str] = &["operation", "source", "format", "backup_type"];

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

        info!("Initializing local SQLite audit sink");

        let pool = SqlitePool::connect(&db_url).await?;

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

        sqlx::query("CREATE TABLE IF NOT EXISTS audit_retention_state (singleton INTEGER PRIMARY KEY CHECK (singleton = 1), event_count INTEGER NOT NULL)")
            .execute(pool).await?;
        sqlx::query("CREATE TABLE IF NOT EXISTS audit_schema_metadata (name TEXT PRIMARY KEY, value TEXT NOT NULL)")
            .execute(pool).await?;
        sqlx::query(
            "INSERT OR IGNORE INTO audit_retention_state(singleton, event_count) VALUES (1, 0)",
        )
        .execute(pool)
        .await?;

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

        // Bound historical databases before enabling triggers. Audit rows are
        // metadata only; old unredacted fields are cleared on bootstrap.
        let cutoff = retention_cutoff();
        sqlx::query("DELETE FROM audit_events WHERE timestamp < ?")
            .bind(cutoff)
            .execute(pool)
            .await?;
        sqlx::query("DELETE FROM audit_events WHERE rowid NOT IN (SELECT rowid FROM audit_events ORDER BY timestamp DESC, rowid DESC LIMIT ?)")
            .bind(MAX_AUDIT_EVENTS)
            .execute(pool)
            .await?;
        let redaction_applied: Option<String> = sqlx::query_scalar(
            "SELECT value FROM audit_schema_metadata WHERE name = 'privacy_redaction_v1'",
        )
        .fetch_optional(pool)
        .await?;
        if redaction_applied.is_none() {
            sqlx::query("UPDATE audit_events SET user_id = NULL, resource_id = NULL, action = CASE WHEN action LIKE '%\"custom\"%' THEN '{\"custom\":\"custom\"}' ELSE action END, result = CASE WHEN result LIKE '%failure%' THEN '{\"failure\":{\"reason\":\"[redacted]\"}}' WHEN result LIKE '%denied%' THEN '{\"denied\":{\"reason\":\"[redacted]\"}}' ELSE '\"success\"' END")
                .execute(pool)
                .await?;
            let rows = sqlx::query("SELECT id, metadata FROM audit_events")
                .fetch_all(pool)
                .await?;
            for row in rows {
                let id: String = row.try_get("id")?;
                let serialized: String = row.try_get("metadata")?;
                let metadata = serde_json::from_str::<BTreeMap<String, String>>(&serialized)
                    .unwrap_or_default();
                let metadata = serde_json::to_string(&sanitize_metadata(&metadata))?;
                sqlx::query("UPDATE audit_events SET metadata = ? WHERE id = ?")
                    .bind(metadata)
                    .bind(id)
                    .execute(pool)
                    .await?;
            }
            sqlx::query("INSERT INTO audit_schema_metadata(name, value) VALUES ('privacy_redaction_v1', '1')")
                .execute(pool)
                .await?;
        }
        sqlx::query("UPDATE audit_retention_state SET event_count = (SELECT COUNT(*) FROM audit_events) WHERE singleton = 1")
            .execute(pool).await?;
        sqlx::query("CREATE TRIGGER IF NOT EXISTS audit_events_count_insert AFTER INSERT ON audit_events BEGIN UPDATE audit_retention_state SET event_count = event_count + 1 WHERE singleton = 1; DELETE FROM audit_events WHERE rowid = (SELECT rowid FROM audit_events ORDER BY timestamp ASC, rowid ASC LIMIT 1) AND (SELECT event_count FROM audit_retention_state WHERE singleton = 1) > 10000; END")
            .execute(pool).await?;
        sqlx::query("CREATE TRIGGER IF NOT EXISTS audit_events_count_delete AFTER DELETE ON audit_events BEGIN UPDATE audit_retention_state SET event_count = MAX(0, event_count - 1) WHERE singleton = 1; END")
            .execute(pool).await?;

        info!("Audit events schema initialized with bounded retention");
        Ok(())
    }

    /// Remove records older than the configured retention period.
    pub async fn prune_expired(&self) -> Result<u64> {
        Self::prune_expired_from_pool(&self.pool).await
    }

    pub async fn prune_expired_from_pool(pool: &SqlitePool) -> Result<u64> {
        let result = sqlx::query("DELETE FROM audit_events WHERE timestamp < ?")
            .bind(retention_cutoff())
            .execute(pool)
            .await?;
        Ok(result.rows_affected())
    }

    /// Get a reference to the connection pool.
    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }
}

#[async_trait]
impl AuditSink for SqliteAuditSink {
    async fn log(&self, event: &AuditEvent) -> Result<()> {
        let event = sanitized_event(event);
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
        .bind(None::<String>)
        .bind(action)
        .bind(None::<String>)
        .bind(result)
        .bind(metadata)
        .execute(&self.pool)
        .await
        .map_err(|_| {
            error!("Failed to insert audit event into the local audit store");
            AppError::Database("Failed to insert audit event into the local audit store".into())
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

fn retention_cutoff() -> String {
    (chrono::Utc::now() - chrono::Duration::days(RETENTION_DAYS)).to_rfc3339()
}

fn sanitized_event(event: &AuditEvent) -> AuditEvent {
    use crate::infrastructure::audit::event::{AuditAction, AuditResult};
    let action = match &event.action {
        AuditAction::Custom(_) => AuditAction::Custom("custom".to_string()),
        action => action.clone(),
    };
    let result = match &event.result {
        AuditResult::Success => AuditResult::Success,
        AuditResult::Failure { .. } => AuditResult::Failure {
            reason: "[redacted]".into(),
        },
        AuditResult::Denied { .. } => AuditResult::Denied {
            reason: "[redacted]".into(),
        },
    };
    let metadata = sanitize_metadata(&event.metadata);
    AuditEvent {
        id: event.id,
        timestamp: event.timestamp,
        user_id: None,
        action,
        resource_id: None,
        result,
        metadata,
    }
}

fn sanitize_metadata(metadata: &BTreeMap<String, String>) -> BTreeMap<String, String> {
    metadata
        .iter()
        .filter_map(|(key, value)| {
            let keep_number =
                SAFE_NUMERIC_METADATA.contains(&key.as_str()) && value.parse::<i64>().is_ok();
            let keep_boolean = SAFE_BOOLEAN_METADATA.contains(&key.as_str())
                && matches!(value.as_str(), "true" | "false");
            let keep_enum = SAFE_ENUM_METADATA.contains(&key.as_str())
                && !value.is_empty()
                && value.len() <= 40
                && value
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-');
            (keep_number || keep_boolean || keep_enum).then(|| (key.clone(), value.clone()))
        })
        .collect()
}

/// Register the bounded SQLite sink on an audit logger. Startup and tests use
/// the same initialization path so missing sink wiring is observable.
pub async fn configure_sqlite_audit_sink(
    logger: &crate::infrastructure::audit::logger::AuditLogger,
    pool: SqlitePool,
) -> Result<()> {
    let sink = SqliteAuditSink::from_pool(pool).await?;
    logger.add_sink(Box::new(sink)).await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audit::event::{AuditAction, AuditResult};
    use sqlx::sqlite::SqlitePoolOptions;
    use tempfile::tempdir;

    async fn create_test_sink() -> Result<(SqliteAuditSink, tempfile::TempDir)> {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test_audit.db");

        std::fs::File::create(&db_path).unwrap();

        let sink = SqliteAuditSink::new(db_path).await?;
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
    async fn startup_sink_registration_persists_only_redacted_allowlisted_metadata() {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        let logger = crate::infrastructure::audit::logger::AuditLogger::new();
        configure_sqlite_audit_sink(&logger, pool.clone())
            .await
            .unwrap();

        let event = AuditEvent::new(
            AuditAction::Custom("private path /Users/alice/secret".into()),
            AuditResult::failure("token=secret provider response"),
        )
        .with_user_id("alice")
        .with_resource_id("/Users/alice/private.txt")
        .with_metadata("file_size_bytes", "1234")
        .with_metadata("query", "private query text");
        logger.log(event).await.unwrap();

        let stored = logger.query(10, 0).await.unwrap();
        assert_eq!(stored.len(), 1);
        assert_eq!(stored[0].user_id, None);
        assert_eq!(stored[0].resource_id, None);
        assert_eq!(stored[0].action, AuditAction::Custom("custom".into()));
        assert_eq!(stored[0].result, AuditResult::failure("[redacted]"));
        assert_eq!(
            stored[0]
                .metadata
                .get("file_size_bytes")
                .map(String::as_str),
            Some("1234")
        );
        assert!(!stored[0].metadata.contains_key("query"));
    }

    #[tokio::test]
    async fn audit_sink_enforces_row_cap_and_age_retention() {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        let sink = SqliteAuditSink::from_pool(pool.clone()).await.unwrap();
        let old = (chrono::Utc::now() - chrono::Duration::days(31)).to_rfc3339();
        sqlx::query("INSERT INTO audit_events(id, timestamp, action, result, metadata) VALUES ('old-event', ?, 'file_indexed', '\"success\"', '{}')")
            .bind(old)
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(sink.prune_expired().await.unwrap(), 1);
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM audit_events WHERE id = 'old-event'"
            )
            .fetch_one(&pool)
            .await
            .unwrap(),
            0
        );

        sqlx::query("WITH RECURSIVE n(x) AS (VALUES(1) UNION ALL SELECT x + 1 FROM n WHERE x < 10005) INSERT INTO audit_events(id, timestamp, action, result, metadata) SELECT printf('%036d', x), '2026-09-28T00:00:00Z', 'file_indexed', '\"success\"', '{}' FROM n")
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(sink.count().await.unwrap(), MAX_AUDIT_EVENTS as usize);
    }

    #[tokio::test]

    async fn test_query_events() {
        let (sink, _dir) = create_test_sink().await.unwrap();

        for i in 0..5 {
            let event = AuditEvent::new(AuditAction::FileIndexed, AuditResult::success())
                .with_resource_id(format!("file{}", i));

            sink.log(&event).await.unwrap();
        }

        // Query events
        let events = sink.query(10, 0).await.unwrap();
        assert_eq!(events.len(), 5);

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
        metadata.insert("file_size_bytes".to_string(), "1024".to_string());
        metadata.insert("query".to_string(), "private query".to_string());

        let event = AuditEvent::new(AuditAction::SearchPerformed, AuditResult::success())
            .with_metadata_map(metadata);

        sink.log(&event).await.unwrap();

        let events = sink.query(1, 0).await.unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].metadata.len(), 1);
        assert_eq!(events[0].metadata.get("file_size_bytes").unwrap(), "1024");
        assert!(!events[0].metadata.contains_key("query"));
    }

    #[tokio::test]
    async fn startup_scrubs_legacy_custom_action_payloads() {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::query("CREATE TABLE audit_events (id TEXT PRIMARY KEY, timestamp TEXT NOT NULL, user_id TEXT, action TEXT NOT NULL, resource_id TEXT, result TEXT NOT NULL, metadata TEXT NOT NULL)")
            .execute(&pool).await.unwrap();
        let id = uuid::Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO audit_events(id, timestamp, user_id, action, resource_id, result, metadata) VALUES (?, ?, 'alice', ?, '/private/path', '{\"failure\":{\"reason\":\"secret\"}}', '{\"query\":\"secret\"}')")
            .bind(id)
            .bind(chrono::Utc::now().to_rfc3339())
            .bind(r#"{"custom":"private action"}"#)
            .execute(&pool).await.unwrap();
        let sink = SqliteAuditSink::from_pool(pool).await.unwrap();
        let event = sink.query(1, 0).await.unwrap().pop().unwrap();
        assert_eq!(event.action, AuditAction::Custom("custom".into()));
        assert_eq!(event.user_id, None);
        assert_eq!(event.resource_id, None);
        assert_eq!(event.result, AuditResult::failure("[redacted]"));
        assert!(event.metadata.is_empty());
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

        for i in 0..2 {
            assert!(events[i].timestamp >= events[i + 1].timestamp);
        }
    }
}

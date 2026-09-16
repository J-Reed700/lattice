use crate::domain::download::{
    Checksum, ChecksumAlgorithm, DownloadError, DownloadProgress, DownloadSession, DownloadState,
};
use crate::infrastructure::persistence::database::connection::DatabaseConnection;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::{FromRow, SqlitePool};
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Debug, FromRow)]
struct DownloadSessionRow {
    id: String,
    url: String,
    destination: String,
    state: String,
    bytes_downloaded: i64,
    total_bytes: Option<i64>,
    bytes_per_second: f64,
    checksum_algorithm: Option<String>,
    checksum_value: Option<String>,
    error_message: Option<String>,
    retry_count: i32,
    max_retries: i32,
    created_at: String,
    updated_at: String,
    started_at: Option<String>,
    completed_at: Option<String>,
    model_name: Option<String>,
    model_id: Option<String>,
    model_file_name: Option<String>,
}

impl TryFrom<DownloadSessionRow> for DownloadSession {
    type Error = DownloadError;

    fn try_from(row: DownloadSessionRow) -> Result<Self, Self::Error> {
        use serde::Deserialize;

        #[derive(Deserialize)]
        struct SessionData {
            id: String,
            url: String,
            destination: String,
            state: String,
            bytes_downloaded: u64,
            total_bytes: Option<u64>,
            bytes_per_second: f64,
            checksum_algorithm: Option<String>,
            checksum_value: Option<String>,
            error_message: Option<String>,
            retry_count: u32,
            max_retries: u32,
            created_at: String,
            updated_at: String,
            started_at: Option<String>,
            completed_at: Option<String>,
        }

        let checksum = match (row.checksum_algorithm, row.checksum_value) {
            (Some(algo), Some(value)) => {
                let algorithm = match algo.as_str() {
                    "sha256" => ChecksumAlgorithm::Sha256,
                    "md5" => ChecksumAlgorithm::Md5,
                    _ => {
                        return Err(DownloadError::InvalidUrl(format!(
                            "Invalid checksum algorithm: {}",
                            algo
                        )))
                    }
                };
                Some(Checksum::new(algorithm, value)?)
            }
            _ => None,
        };

        let total_bytes = row.total_bytes.map(|b| b as u64);

        let created_at = DateTime::parse_from_rfc3339(&row.created_at)
            .map_err(|e| DownloadError::InvalidUrl(format!("Invalid created_at: {}", e)))?
            .with_timezone(&Utc);

        let started_at = row.started_at.as_ref().and_then(|s| {
            DateTime::parse_from_rfc3339(s)
                .map(|dt| dt.with_timezone(&Utc))
                .ok()
        });

        let completed_at = row.completed_at.as_ref().and_then(|s| {
            DateTime::parse_from_rfc3339(s)
                .map(|dt| dt.with_timezone(&Utc))
                .ok()
        });

        let mut progress = DownloadProgress::new(total_bytes);
        progress.update(row.bytes_downloaded as u64, row.bytes_per_second);

        let state = match row.state.as_str() {
            "pending" => DownloadState::Pending,
            "downloading" => DownloadState::Downloading,
            "paused" => DownloadState::Paused,
            "completed" => DownloadState::Completed,
            "failed" => DownloadState::Failed,
            "cancelled" => DownloadState::Cancelled,
            _ => {
                return Err(DownloadError::InvalidUrl(format!(
                    "Invalid state: {}",
                    row.state
                )))
            }
        };

        let json_str = serde_json::json!({
            "id": row.id,
            "url": row.url,
            "destination": row.destination,
            "state": state,
            "progress": progress,
            "checksum": checksum,
            "error_message": row.error_message,
            "retry_count": row.retry_count,
            "max_retries": 3u32,
            "created_at": created_at,
            "updated_at": Utc::now(),
            "started_at": started_at,
            "completed_at": completed_at,
            "model_name": row.model_name,
            "model_id": row.model_id,
            "model_file_name": row.model_file_name,
        });

        serde_json::from_value(json_str)
            .map_err(|e| DownloadError::InvalidUrl(format!("Failed to deserialize session: {}", e)))
    }
}

#[async_trait]
pub trait DownloadRepository: Send + Sync {
    async fn create(&self, session: &DownloadSession) -> Result<(), DownloadError>;
    async fn get(&self, id: &str) -> Result<Option<DownloadSession>, DownloadError>;
    async fn update(&self, session: &DownloadSession) -> Result<(), DownloadError>;

    /// Persist transfer progress **without** touching `state`.
    ///
    /// Progress ticks arrive constantly and concurrently with state changes.
    /// Writing them through the full-row `update` meant a read-modify-write:
    /// a tick could read the row while it still said `downloading`, and land
    /// its UPDATE *after* the owning task wrote `completed` — reverting the
    /// row to `downloading` with stale byte counts. The session then looked
    /// like it was downloading forever, and on next boot startup
    /// reconciliation marked it `failed` even though the model had completed
    /// and been registered, leaving the sessions and models tables disagreeing.
    ///
    /// Scoping the statement to the progress columns makes that class of
    /// clobber impossible. The `WHERE state = 'downloading'` guard also drops
    /// ticks that arrive after a terminal transition instead of resurrecting
    /// the row.
    async fn update_progress(
        &self,
        id: &str,
        bytes_downloaded: u64,
        bytes_per_second: f64,
    ) -> Result<(), DownloadError>;
    async fn delete(&self, id: &str) -> Result<(), DownloadError>;
    async fn list(&self) -> Result<Vec<DownloadSession>, DownloadError>;
    async fn list_by_state(
        &self,
        state: &DownloadState,
    ) -> Result<Vec<DownloadSession>, DownloadError>;
    async fn list_active(&self) -> Result<Vec<DownloadSession>, DownloadError>;
    /// Delete all pending downloads for a specific model
    async fn delete_pending_by_model(&self, model_id: &str) -> Result<u64, DownloadError>;
}

pub struct SqliteDownloadRepository {
    db_conn: Arc<DatabaseConnection>, // Single source of truth for database access
}

impl SqliteDownloadRepository {
    pub fn new(db_conn: Arc<DatabaseConnection>) -> Self {
        Self { db_conn }
    }
}

#[async_trait]
impl DownloadRepository for SqliteDownloadRepository {
    async fn create(&self, session: &DownloadSession) -> Result<(), DownloadError> {
        tracing::info!(
            "→ Step 1: Acquiring BEGIN IMMEDIATE transaction... download_id={}",
            session.id()
        );

        // Use BEGIN IMMEDIATE to acquire write lock immediately, preventing deadlock
        let mut tx =
            self.db_conn.begin_immediate().await.map_err(|e| {
                DownloadError::IoError(format!("Failed to begin transaction: {}", e))
            })?;

        tracing::info!(
            "✓ Step 1 complete: BEGIN IMMEDIATE transaction acquired download_id={}",
            session.id()
        );

        let state_str = format!("{:?}", session.state()).to_lowercase();

        let (checksum_algorithm, checksum_value) = session
            .checksum()
            .map(|c| {
                (
                    Some(format!("{:?}", c.algorithm()).to_lowercase()),
                    Some(c.value().to_string()),
                )
            })
            .unwrap_or((None, None));

        tracing::info!(
            "→ Step 2: Executing INSERT query... download_id={}",
            session.id()
        );

        sqlx::query(
            r#"
            INSERT OR REPLACE INTO download_sessions (
                id, url, destination, state,
                bytes_downloaded, total_bytes, bytes_per_second,
                checksum_algorithm, checksum_value,
                error_message, retry_count, max_retries,
                created_at, updated_at, started_at, completed_at,
                model_name, model_id, model_file_name
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(session.id())
        .bind(session.url())
        .bind(session.destination().to_str().ok_or_else(|| {
            DownloadError::IoError("Invalid UTF-8 in destination path".to_string())
        })?)
        .bind(state_str)
        .bind(session.progress().bytes_downloaded() as i64)
        .bind(session.progress().total_bytes().map(|b| b as i64))
        .bind(session.progress().bytes_per_second())
        .bind(checksum_algorithm)
        .bind(checksum_value)
        .bind(session.error_message())
        .bind(session.retry_count() as i32)
        .bind(3i32) // max_retries
        .bind(session.created_at().to_rfc3339())
        .bind(session.created_at().to_rfc3339()) // updated_at initially same as created_at
        .bind(session.started_at().map(|dt| dt.to_rfc3339()))
        .bind(session.completed_at().map(|dt| dt.to_rfc3339()))
        .bind(session.model_name())
        .bind(session.model_id())
        .bind(session.model_file_name())
        .execute(&mut *tx)
        .await
        .map_err(|e| DownloadError::IoError(format!("Failed to create session: {}", e)))?;

        tracing::info!(
            "✓ Step 2 complete: INSERT query executed download_id={}",
            session.id()
        );
        tracing::info!(
            "→ Step 3: Committing transaction... download_id={}",
            session.id()
        );

        tx.commit()
            .await
            .map_err(|e| DownloadError::IoError(format!("Failed to commit transaction: {}", e)))?;

        tracing::info!(
            "✓ Step 3 complete: Transaction committed download_id={}",
            session.id()
        );

        Ok(())
    }

    async fn get(&self, id: &str) -> Result<Option<DownloadSession>, DownloadError> {
        let row = sqlx::query_as::<_, DownloadSessionRow>(
            r#"
            SELECT * FROM download_sessions WHERE id = ?
            "#,
        )
        .bind(id)
        .fetch_optional(self.db_conn.pool())
        .await
        .map_err(|e| DownloadError::IoError(format!("Failed to get session: {}", e)))?;

        match row {
            Some(row) => Ok(Some(row.try_into()?)),
            None => Ok(None),
        }
    }

    async fn update(&self, session: &DownloadSession) -> Result<(), DownloadError> {
        // Use BEGIN IMMEDIATE to acquire write lock immediately, preventing deadlock
        let mut tx =
            self.db_conn.begin_immediate().await.map_err(|e| {
                DownloadError::IoError(format!("Failed to begin transaction: {}", e))
            })?;

        let state_str = format!("{:?}", session.state()).to_lowercase();

        let (checksum_algorithm, checksum_value) = session
            .checksum()
            .map(|c| {
                (
                    Some(format!("{:?}", c.algorithm()).to_lowercase()),
                    Some(c.value().to_string()),
                )
            })
            .unwrap_or((None, None));

        sqlx::query(
            r#"
            UPDATE download_sessions
            SET url = ?, destination = ?, state = ?,
                bytes_downloaded = ?, total_bytes = ?, bytes_per_second = ?,
                checksum_algorithm = ?, checksum_value = ?,
                error_message = ?, retry_count = ?, max_retries = ?,
                updated_at = ?, started_at = ?, completed_at = ?,
                model_name = ?, model_id = ?, model_file_name = ?
            WHERE id = ?
            "#,
        )
        .bind(session.url())
        .bind(session.destination().to_str().ok_or_else(|| {
            DownloadError::IoError("Invalid UTF-8 in destination path".to_string())
        })?)
        .bind(state_str)
        .bind(session.progress().bytes_downloaded() as i64)
        .bind(session.progress().total_bytes().map(|b| b as i64))
        .bind(session.progress().bytes_per_second())
        .bind(checksum_algorithm)
        .bind(checksum_value)
        .bind(session.error_message())
        .bind(session.retry_count() as i32)
        .bind(3i32) // max_retries
        .bind(Utc::now().to_rfc3339())
        .bind(session.started_at().map(|dt| dt.to_rfc3339()))
        .bind(session.completed_at().map(|dt| dt.to_rfc3339()))
        .bind(session.model_name())
        .bind(session.model_id())
        .bind(session.model_file_name())
        .bind(session.id())
        .execute(&mut *tx)
        .await
        .map_err(|e| DownloadError::IoError(format!("Failed to update session: {}", e)))?;

        tx.commit()
            .await
            .map_err(|e| DownloadError::IoError(format!("Failed to commit transaction: {}", e)))?;

        Ok(())
    }

    async fn update_progress(
        &self,
        id: &str,
        bytes_downloaded: u64,
        bytes_per_second: f64,
    ) -> Result<(), DownloadError> {
        let mut tx =
            self.db_conn.begin_immediate().await.map_err(|e| {
                DownloadError::IoError(format!("Failed to begin transaction: {}", e))
            })?;

        // Touches only the progress columns, and only while the row is still
        // downloading — a tick that races a completion is dropped rather than
        // reverting the state.
        sqlx::query(
            r#"
            UPDATE download_sessions
            SET bytes_downloaded = ?, bytes_per_second = ?, updated_at = ?
            WHERE id = ? AND state = 'downloading'
            "#,
        )
        .bind(bytes_downloaded as i64)
        .bind(bytes_per_second)
        .bind(Utc::now().to_rfc3339())
        .bind(id)
        .execute(&mut *tx)
        .await
        .map_err(|e| DownloadError::IoError(format!("Failed to update progress: {}", e)))?;

        tx.commit()
            .await
            .map_err(|e| DownloadError::IoError(format!("Failed to commit transaction: {}", e)))?;

        Ok(())
    }

    async fn delete(&self, id: &str) -> Result<(), DownloadError> {
        // Use BEGIN IMMEDIATE to acquire write lock immediately, preventing deadlock
        let mut tx =
            self.db_conn.begin_immediate().await.map_err(|e| {
                DownloadError::IoError(format!("Failed to begin transaction: {}", e))
            })?;

        sqlx::query("DELETE FROM download_sessions WHERE id = ?")
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(|e| DownloadError::IoError(format!("Failed to delete session: {}", e)))?;

        tx.commit()
            .await
            .map_err(|e| DownloadError::IoError(format!("Failed to commit transaction: {}", e)))?;

        Ok(())
    }

    async fn list(&self) -> Result<Vec<DownloadSession>, DownloadError> {
        let rows = sqlx::query_as::<_, DownloadSessionRow>(
            r#"
            SELECT * FROM download_sessions ORDER BY created_at DESC
            "#,
        )
        .fetch_all(self.db_conn.pool())
        .await
        .map_err(|e| DownloadError::IoError(format!("Failed to list sessions: {}", e)))?;

        rows.into_iter().map(|row| row.try_into()).collect()
    }

    async fn list_by_state(
        &self,
        state: &DownloadState,
    ) -> Result<Vec<DownloadSession>, DownloadError> {
        let state_str = format!("{:?}", state).to_lowercase();

        let rows = sqlx::query_as::<_, DownloadSessionRow>(
            r#"
            SELECT * FROM download_sessions WHERE state = ? ORDER BY created_at DESC
            "#,
        )
        .bind(state_str)
        .fetch_all(self.db_conn.pool())
        .await
        .map_err(|e| DownloadError::IoError(format!("Failed to list sessions by state: {}", e)))?;

        rows.into_iter().map(|row| row.try_into()).collect()
    }

    async fn list_active(&self) -> Result<Vec<DownloadSession>, DownloadError> {
        let rows = sqlx::query_as::<_, DownloadSessionRow>(
            r#"
            SELECT * FROM download_sessions
            WHERE state IN ('pending', 'downloading', 'paused')
            ORDER BY created_at DESC
            "#,
        )
        .fetch_all(self.db_conn.pool())
        .await
        .map_err(|e| DownloadError::IoError(format!("Failed to list active sessions: {}", e)))?;

        rows.into_iter().map(|row| row.try_into()).collect()
    }

    async fn delete_pending_by_model(&self, model_id: &str) -> Result<u64, DownloadError> {
        // Use BEGIN IMMEDIATE to acquire write lock immediately, preventing deadlock
        let mut tx =
            self.db_conn.begin_immediate().await.map_err(|e| {
                DownloadError::IoError(format!("Failed to begin transaction: {}", e))
            })?;

        let result = sqlx::query(
            r#"
            DELETE FROM download_sessions
            WHERE model_id = ? AND state = 'pending'
            "#,
        )
        .bind(model_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            DownloadError::IoError(format!("Failed to delete pending downloads: {}", e))
        })?;

        let rows_affected = result.rows_affected();

        tx.commit()
            .await
            .map_err(|e| DownloadError::IoError(format!("Failed to commit transaction: {}", e)))?;

        Ok(rows_affected)
    }
}

#[cfg(any(test, feature = "test-utils"))]
#[allow(clippy::unwrap_used)] // Test/mock code - unwrap() is acceptable for test infrastructure
pub mod mock {
    use super::*;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    pub struct MockDownloadRepository {
        sessions: Arc<Mutex<HashMap<String, DownloadSession>>>,
    }

    impl MockDownloadRepository {
        pub fn new() -> Self {
            Self {
                sessions: Arc::new(Mutex::new(HashMap::new())),
            }
        }

        pub fn with_session(self, session: DownloadSession) -> Self {
            self.sessions
                .lock()
                .unwrap()
                .insert(session.id().to_string(), session);
            self
        }
    }

    impl Default for MockDownloadRepository {
        fn default() -> Self {
            Self::new()
        }
    }

    #[async_trait]
    impl DownloadRepository for MockDownloadRepository {
        async fn create(&self, session: &DownloadSession) -> Result<(), DownloadError> {
            let mut sessions = self.sessions.lock().unwrap();
            if sessions.contains_key(session.id()) {
                return Err(DownloadError::InvalidUrl(format!(
                    "Session already exists: {}",
                    session.id()
                )));
            }
            sessions.insert(session.id().to_string(), session.clone());
            Ok(())
        }

        async fn get(&self, id: &str) -> Result<Option<DownloadSession>, DownloadError> {
            let sessions = self.sessions.lock().unwrap();
            Ok(sessions.get(id).cloned())
        }

        async fn update(&self, session: &DownloadSession) -> Result<(), DownloadError> {
            let mut sessions = self.sessions.lock().unwrap();
            if !sessions.contains_key(session.id()) {
                return Err(DownloadError::SessionNotFound(session.id().to_string()));
            }
            sessions.insert(session.id().to_string(), session.clone());
            Ok(())
        }

        async fn update_progress(
            &self,
            id: &str,
            bytes_downloaded: u64,
            bytes_per_second: f64,
        ) -> Result<(), DownloadError> {
            let mut sessions = self.sessions.lock().unwrap();
            if let Some(session) = sessions.get_mut(id) {
                // Mirror the SQL guard: only a downloading session takes ticks.
                if *session.state() == DownloadState::Downloading {
                    session.update_progress(bytes_downloaded, bytes_per_second);
                }
            }
            Ok(())
        }

        async fn delete(&self, id: &str) -> Result<(), DownloadError> {
            let mut sessions = self.sessions.lock().unwrap();
            sessions.remove(id);
            Ok(())
        }

        async fn list(&self) -> Result<Vec<DownloadSession>, DownloadError> {
            let sessions = self.sessions.lock().unwrap();
            Ok(sessions.values().cloned().collect())
        }

        async fn list_by_state(
            &self,
            state: &DownloadState,
        ) -> Result<Vec<DownloadSession>, DownloadError> {
            let sessions = self.sessions.lock().unwrap();
            Ok(sessions
                .values()
                .filter(|s| s.state() == state)
                .cloned()
                .collect())
        }

        async fn list_active(&self) -> Result<Vec<DownloadSession>, DownloadError> {
            let sessions = self.sessions.lock().unwrap();
            Ok(sessions
                .values()
                .filter(|s| !s.state().is_terminal())
                .cloned()
                .collect())
        }

        async fn delete_pending_by_model(&self, model_id: &str) -> Result<u64, DownloadError> {
            let mut sessions = self.sessions.lock().unwrap();
            let to_delete: Vec<String> = sessions
                .values()
                .filter(|s| s.model_id() == Some(model_id) && s.state() == &DownloadState::Pending)
                .map(|s| s.id().to_string())
                .collect();

            let count = to_delete.len() as u64;
            for id in to_delete {
                sessions.remove(&id);
            }

            Ok(count)
        }
    }
}

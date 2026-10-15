//! The `web_page_snapshots` rows (§7).

use std::collections::HashSet;

use chrono::Utc;
use sqlx::SqlitePool;

use super::dto::WebPageSnapshotStatus;
use lattice_core::shared::error::{AppError, Result};

fn db(error: sqlx::Error) -> AppError {
    AppError::Database(error.to_string())
}

const COLUMNS: &str = "id, conversation_id, normalized_url, requested_url, final_url, title, \
    status, method, captured_at, byte_size, sha256, resource_count, missing_resource_count, \
    partial, partial_reasons, error_code, error_message, favicon_data_url, job_id, created_at";

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct SnapshotRow {
    pub id: String,
    pub conversation_id: String,
    pub normalized_url: String,
    pub requested_url: String,
    pub final_url: Option<String>,
    pub title: Option<String>,
    pub status: String,
    pub method: Option<String>,
    pub captured_at: Option<String>,
    pub byte_size: Option<i64>,
    pub sha256: Option<String>,
    pub resource_count: i64,
    pub missing_resource_count: i64,
    pub partial: bool,
    pub partial_reasons: Option<String>,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
    pub favicon_data_url: Option<String>,
    pub job_id: Option<String>,
    pub created_at: String,
}

impl SnapshotRow {
    pub fn status(&self) -> WebPageSnapshotStatus {
        WebPageSnapshotStatus::parse(&self.status)
    }
}

/// What a finished capture records, in the transaction that flips the row to
/// `ready`.
#[derive(Debug, Clone)]
pub struct ReadyCapture {
    pub final_url: String,
    pub title: Option<String>,
    pub method: &'static str,
    pub captured_at: String,
    pub byte_size: u64,
    pub sha256: String,
    pub resource_count: u32,
    pub missing_resource_count: u32,
    pub partial: bool,
    pub partial_reasons: Vec<String>,
    pub favicon_data_url: Option<String>,
}

#[derive(Clone)]
pub struct SnapshotRepository {
    pool: SqlitePool,
}

impl SnapshotRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// The row for `(conversation_id, normalized_url)`, creating a pending one
    /// with `new_id` when there is none. The flag says whether this call made it.
    pub async fn insert_or_get(
        &self,
        new_id: &str,
        conversation_id: &str,
        normalized_url: &str,
        requested_url: &str,
    ) -> Result<(SnapshotRow, bool)> {
        let inserted = sqlx::query(
            "INSERT INTO web_page_snapshots \
                 (id, conversation_id, normalized_url, requested_url, status, created_at) \
             VALUES (?, ?, ?, ?, 'pending', ?) \
             ON CONFLICT(conversation_id, normalized_url) DO NOTHING",
        )
        .bind(new_id)
        .bind(conversation_id)
        .bind(normalized_url)
        .bind(requested_url)
        .bind(Utc::now().to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(db)?
        .rows_affected()
            == 1;
        let row = self
            .find(conversation_id, normalized_url)
            .await?
            .ok_or_else(|| AppError::Database("snapshot row vanished after insert".into()))?;
        Ok((row, inserted))
    }

    pub async fn get(&self, id: &str) -> Result<Option<SnapshotRow>> {
        sqlx::query_as::<_, SnapshotRow>(&format!(
            "SELECT {COLUMNS} FROM web_page_snapshots WHERE id = ?"
        ))
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(db)
    }

    pub async fn find(
        &self,
        conversation_id: &str,
        normalized_url: &str,
    ) -> Result<Option<SnapshotRow>> {
        sqlx::query_as::<_, SnapshotRow>(&format!(
            "SELECT {COLUMNS} FROM web_page_snapshots \
             WHERE conversation_id = ? AND normalized_url = ?"
        ))
        .bind(conversation_id)
        .bind(normalized_url)
        .fetch_optional(&self.pool)
        .await
        .map_err(db)
    }

    pub async fn list(&self, conversation_id: &str) -> Result<Vec<SnapshotRow>> {
        sqlx::query_as::<_, SnapshotRow>(&format!(
            "SELECT {COLUMNS} FROM web_page_snapshots WHERE conversation_id = ? \
             ORDER BY created_at, id"
        ))
        .bind(conversation_id)
        .fetch_all(&self.pool)
        .await
        .map_err(db)
    }

    pub async fn set_job(&self, id: &str, job_id: &str) -> Result<()> {
        sqlx::query("UPDATE web_page_snapshots SET job_id = ? WHERE id = ?")
            .bind(job_id)
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(db)?;
        Ok(())
    }

    /// A failed row goes back to pending for a user's retry. False when the
    /// row is not failed (someone else already retried it).
    pub async fn reset_failed(&self, id: &str) -> Result<bool> {
        Ok(sqlx::query(
            "UPDATE web_page_snapshots SET status = 'pending', error_code = NULL, \
             error_message = NULL WHERE id = ? AND status = 'failed'",
        )
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(db)?
        .rows_affected()
            == 1)
    }

    /// Flips a pending row to ready with its hash and size, in one
    /// transaction. False when the row is gone or no longer pending.
    pub async fn mark_ready(&self, id: &str, capture: &ReadyCapture) -> Result<bool> {
        let reasons = if capture.partial_reasons.is_empty() {
            None
        } else {
            Some(serde_json::to_string(&capture.partial_reasons)?)
        };
        let mut tx = self.pool.begin().await.map_err(db)?;
        let updated = sqlx::query(
            "UPDATE web_page_snapshots SET status = 'ready', final_url = ?, title = ?, \
                 method = ?, captured_at = ?, byte_size = ?, sha256 = ?, resource_count = ?, \
                 missing_resource_count = ?, partial = ?, partial_reasons = ?, \
                 favicon_data_url = ?, error_code = NULL, error_message = NULL \
             WHERE id = ? AND status = 'pending'",
        )
        .bind(&capture.final_url)
        .bind(&capture.title)
        .bind(capture.method)
        .bind(&capture.captured_at)
        .bind(capture.byte_size as i64)
        .bind(&capture.sha256)
        .bind(capture.resource_count as i64)
        .bind(capture.missing_resource_count as i64)
        .bind(capture.partial)
        .bind(reasons)
        .bind(&capture.favicon_data_url)
        .bind(id)
        .execute(&mut *tx)
        .await
        .map_err(db)?
        .rows_affected()
            == 1;
        tx.commit().await.map_err(db)?;
        Ok(updated)
    }

    pub async fn mark_failed(&self, id: &str, code: &str, message: &str) -> Result<bool> {
        Ok(sqlx::query(
            "UPDATE web_page_snapshots SET status = 'failed', error_code = ?, \
             error_message = ? WHERE id = ? AND status = 'pending'",
        )
        .bind(code)
        .bind(message)
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(db)?
        .rows_affected()
            == 1)
    }

    /// Ids whose file must stay: pending (may be written any moment) and ready.
    pub async fn live_ids(&self) -> Result<HashSet<String>> {
        let ids: Vec<String> = sqlx::query_scalar(
            "SELECT id FROM web_page_snapshots WHERE status IN ('pending', 'ready')",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        Ok(ids.into_iter().collect())
    }
}

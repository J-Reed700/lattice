use super::super::{JobRecord, JobStatus};
use super::{not_found, now, parse, JobStore, STOPPED_THE_APP, SUMMARY_COLUMNS};
use crate::shared::error::{AppError, Result};
use crate::shared::persistence::Page;
use sqlx::SqliteConnection;

impl JobStore {
    pub async fn get(&self, id: &str) -> Result<JobRecord> {
        let mut read = self.db.begin_read().await?;
        let job = Self::find_in(&mut read, id).await?;
        read.commit().await?;
        job.ok_or_else(not_found)
    }

    /// Reads the job without loading or parsing `staged_result_json`.
    pub async fn get_summary(&self, id: &str) -> Result<JobRecord> {
        let mut read = self.db.begin_read().await?;
        let job = Self::find_summary_in(&mut read, id).await?;
        read.commit().await?;
        job.ok_or_else(not_found)
    }

    pub async fn find_in(connection: &mut SqliteConnection, id: &str) -> Result<Option<JobRecord>> {
        sqlx::query("SELECT * FROM jobs WHERE id=?")
            .bind(id)
            .fetch_optional(connection)
            .await?
            .as_ref()
            .map(parse)
            .transpose()
    }

    /// Finds the job without loading or parsing `staged_result_json`.
    pub async fn find_summary_in(
        connection: &mut SqliteConnection,
        id: &str,
    ) -> Result<Option<JobRecord>> {
        let sql = format!("SELECT {SUMMARY_COLUMNS} FROM jobs WHERE id=?");
        sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
            .bind(id)
            .fetch_optional(connection)
            .await?
            .as_ref()
            .map(parse)
            .transpose()
    }

    /// A subject's jobs of one kind, newest first.
    pub async fn list_for_subject(&self, kind: &str, subject_id: &str) -> Result<Vec<JobRecord>> {
        let mut read = self.db.begin_read().await?;
        let jobs = Self::list_for_subject_in(&mut read, kind, subject_id).await?;
        read.commit().await?;
        Ok(jobs)
    }

    pub async fn list_for_subject_in(
        connection: &mut SqliteConnection,
        kind: &str,
        subject_id: &str,
    ) -> Result<Vec<JobRecord>> {
        sqlx::query(
            "SELECT * FROM jobs WHERE kind=? AND subject_id=? ORDER BY created_at DESC,rowid DESC",
        )
        .bind(kind)
        .bind(subject_id)
        .fetch_all(connection)
        .await?
        .iter()
        .map(parse)
        .collect()
    }

    /// A subject's jobs of one kind without `staged_result_json`, newest first.
    pub async fn list_summaries_for_subject_in(
        connection: &mut SqliteConnection,
        kind: &str,
        subject_id: &str,
    ) -> Result<Vec<JobRecord>> {
        let sql = format!(
            "SELECT {SUMMARY_COLUMNS} FROM jobs WHERE kind=? AND subject_id=? ORDER BY created_at DESC,rowid DESC"
        );
        sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
            .bind(kind)
            .bind(subject_id)
            .fetch_all(connection)
            .await?
            .iter()
            .map(parse)
            .collect()
    }

    /// Pending and running jobs of a kind, oldest first.
    pub async fn live(&self, kind: &str) -> Result<Vec<JobRecord>> {
        let mut read = self.db.begin_read().await?;
        let sql = format!(
            "SELECT {SUMMARY_COLUMNS} FROM jobs WHERE kind=? AND status IN (?,?) ORDER BY created_at,rowid"
        );
        let rows = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
            .bind(kind)
            .bind(JobStatus::Pending.as_str())
            .bind(JobStatus::Running.as_str())
            .fetch_all(&mut *read)
            .await?;
        read.commit().await?;
        rows.iter().map(parse).collect()
    }

    /// Jobs of the given kinds, newest first, leaving out attempts that a
    /// later retry superseded. An unlimited `page` returns every such job.
    pub async fn list_latest(&self, kinds: &[&str], page: Page) -> Result<Vec<JobRecord>> {
        let kinds =
            serde_json::to_string(kinds).map_err(|e| AppError::Serialization(e.to_string()))?;
        let mut read = self.db.begin_read().await?;
        let sql = format!(
            "SELECT {SUMMARY_COLUMNS} FROM jobs WHERE kind IN (SELECT value FROM json_each(?)) AND NOT EXISTS (SELECT 1 FROM jobs retry WHERE retry.retry_of_job_id=jobs.id) ORDER BY created_at DESC,rowid DESC LIMIT ? OFFSET ?"
        );
        let rows = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
            .bind(kinds)
            .bind(page.sql_limit())
            .bind(page.offset())
            .fetch_all(&mut *read)
            .await?;
        read.commit().await?;
        rows.iter().map(parse).collect()
    }

    /// Pending jobs of a kind whose retry time, if any, has passed. Jobs are
    /// saved before their worker starts, so this also finds work whose process
    /// exited between those steps.
    pub async fn due(&self, kind: &str) -> Result<Vec<JobRecord>> {
        let mut read = self.db.begin_read().await?;
        let sql = format!(
            "SELECT {SUMMARY_COLUMNS} FROM jobs WHERE kind=? AND status=? AND (retry_not_before IS NULL OR retry_not_before<=?) ORDER BY coalesce(rank,9223372036854775807),created_at,rowid"
        );
        let rows = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
            .bind(kind)
            .bind(JobStatus::Pending.as_str())
            .bind(now())
            .fetch_all(&mut *read)
            .await?;
        read.commit().await?;
        rows.iter().map(parse).collect()
    }

    /// Whether the newest job of `kind` for `subject_id` ended because it
    /// stopped the app. Automatic resubmission must leave such work waiting
    /// for the learner's retry, which makes a newer job.
    pub async fn last_run_stopped_the_app(&self, kind: &str, subject_id: &str) -> Result<bool> {
        let mut read = self.db.begin_read().await?;
        let code: Option<Option<String>> = sqlx::query_scalar("SELECT CASE WHEN status=? THEN error_code END FROM jobs WHERE kind=? AND subject_id=? ORDER BY created_at DESC,rowid DESC LIMIT 1")
            .bind(JobStatus::Failed.as_str())
            .bind(kind)
            .bind(subject_id)
            .fetch_optional(&mut *read)
            .await?;
        read.commit().await?;
        Ok(code.flatten().as_deref() == Some(STOPPED_THE_APP))
    }
}

use crate::application::ports::batch_job_repository_port::{
    BatchItemState, BatchJobItem, BatchJobRepositoryPort, BatchJobStatus, BatchJobSummary,
};
use crate::shared::error::AppError;
use async_trait::async_trait;
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

pub struct BatchJobRepository {
    pool: SqlitePool,
}

impl BatchJobRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl BatchJobRepositoryPort for BatchJobRepository {
    async fn requeue_failed_files(
        &self,
        job_id: &str,
        item_id: Option<&str>,
        replacement_path: Option<&str>,
    ) -> Result<usize, AppError> {
        let mut tx = self.pool.begin().await?;
        let count =
            super::batch_job::ops::requeue_failed_files(&mut tx, job_id, item_id, replacement_path)
                .await?;
        tx.commit().await?;
        Ok(count)
    }

    async fn get_job_options(&self, job_id: &str) -> Result<Option<String>, AppError> {
        let mut conn = self.pool.acquire().await?;
        super::batch_job::ops::get_job_options(&mut conn, job_id).await
    }

    async fn create_batch_job(
        &self,
        job_id: &str,
        job_type: &str,
        total_items: i64,
        options: Option<&str>,
    ) -> Result<(), AppError> {
        sqlx::query!(
            r#"
            INSERT INTO batch_jobs (id, job_type, status, total_items, completed_items, failed_items, progress, options)
            VALUES (?1, ?2, 'pending', ?3, 0, 0, 0.0, ?4)
            "#,
            job_id,
            job_type,
            total_items,
            options
        )
        .execute(&self.pool)
        .await
        .map_err(|e| AppError::Database(format!("Failed to create batch job: {}", e)))?;

        Ok(())
    }

    async fn create_batch_items(&self, job_id: &str, urls: Vec<String>) -> Result<(), AppError> {
        for url in urls {
            let item_id = Uuid::new_v4().to_string();
            sqlx::query!(
                r#"
                INSERT INTO batch_job_items (id, job_id, item_url, status)
                VALUES (?1, ?2, ?3, 'pending')
                "#,
                item_id,
                job_id,
                url
            )
            .execute(&self.pool)
            .await
            .map_err(|e| AppError::Database(format!("Failed to create batch item: {}", e)))?;
        }

        Ok(())
    }

    async fn update_job_status(
        &self,
        job_id: &str,
        status: &str,
        started_at: Option<String>,
        completed_at: Option<String>,
    ) -> Result<(), AppError> {
        sqlx::query!(
            r#"
            UPDATE batch_jobs
            SET status = ?1,
                started_at = COALESCE(?2, started_at),
                completed_at = COALESCE(?3, completed_at)
            WHERE id = ?4
            "#,
            status,
            started_at,
            completed_at,
            job_id
        )
        .execute(&self.pool)
        .await
        .map_err(|e| AppError::Database(format!("Failed to update job status: {}", e)))?;

        Ok(())
    }

    async fn update_progress(
        &self,
        job_id: &str,
        completed: i64,
        failed: i64,
        progress: f64,
    ) -> Result<(), AppError> {
        sqlx::query!(
            r#"
            UPDATE batch_jobs
            SET completed_items = ?1,
                failed_items = ?2,
                progress = ?3
            WHERE id = ?4
            "#,
            completed,
            failed,
            progress,
            job_id
        )
        .execute(&self.pool)
        .await
        .map_err(|e| AppError::Database(format!("Failed to update progress: {}", e)))?;

        Ok(())
    }

    async fn update_item_status(
        &self,
        item_id: &str,
        status: BatchItemState,
        document_id: Option<&str>,
        error_message: Option<&str>,
    ) -> Result<(), AppError> {
        let mut conn = self.pool.acquire().await?;
        super::batch_job::ops::update_item_status(
            &mut conn,
            item_id,
            status,
            document_id,
            error_message,
        )
        .await
    }

    async fn get_batch_job(&self, job_id: &str) -> Result<BatchJobStatus, AppError> {
        // A retry changes the job and its items together. Read both from one
        // SQLite snapshot so a completed heading cannot accompany retrying rows.
        let mut tx = self.pool.begin().await?;
        let job = super::batch_job::ops::get_batch_job(&mut tx, job_id).await?;
        tx.commit().await?;
        Ok(job)
    }

    async fn get_pending_items(&self, job_id: &str) -> Result<Vec<BatchJobItem>, AppError> {
        let items = sqlx::query_as::<_, (String, String)>("SELECT id, item_url FROM batch_job_items WHERE job_id = ? AND status = 'pending' ORDER BY created_at ASC, rowid ASC").bind(job_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AppError::Database(format!("Failed to fetch pending items: {}", e)))?
        .into_iter()
        .map(|row| BatchJobItem {
            id: row.0,
            url: row.1,
        })
        .collect();

        Ok(items)
    }

    async fn cancel_pending_items(&self, job_id: &str) -> Result<usize, AppError> {
        let mut conn = self.pool.acquire().await?;
        super::batch_job::ops::cancel_pending_items(&mut conn, job_id).await
    }

    async fn list_batch_jobs(
        &self,
        limit: Option<i64>,
        offset: Option<i64>,
    ) -> Result<Vec<BatchJobSummary>, AppError> {
        let limit = limit.unwrap_or(100);
        let offset = offset.unwrap_or(0);

        let jobs = sqlx::query(
            r#"
            SELECT id, job_type, status, total_items, completed_items, failed_items,
                   progress, created_at, completed_at
            FROM batch_jobs
            ORDER BY created_at DESC
            LIMIT ?1 OFFSET ?2
            "#,
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AppError::Database(format!("Failed to list jobs: {}", e)))?;

        let summaries = jobs
            .into_iter()
            .map(|row| BatchJobSummary {
                id: row.get("id"),
                job_type: row.get("job_type"),
                status: row.get("status"),
                total_items: row.get("total_items"),
                completed_items: row.get("completed_items"),
                failed_items: row.get("failed_items"),
                progress: row.get("progress"),
                created_at: row.get("created_at"),
                completed_at: row.get("completed_at"),
            })
            .collect();

        Ok(summaries)
    }

    async fn delete_batch_job(&self, job_id: &str) -> Result<(), AppError> {
        sqlx::query!("DELETE FROM batch_jobs WHERE id = ?1", job_id)
            .execute(&self.pool)
            .await
            .map_err(|e| AppError::Database(format!("Failed to delete job: {}", e)))?;

        Ok(())
    }
}

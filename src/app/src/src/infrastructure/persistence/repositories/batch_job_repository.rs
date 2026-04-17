use crate::application::ports::batch_job_repository_port::{
    BatchJobItem, BatchJobItemStatus, BatchJobRepositoryPort, BatchJobStatus, BatchJobSummary,
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
        status: &str,
        document_id: Option<&str>,
        error_message: Option<&str>,
    ) -> Result<(), AppError> {
        let processed_at = if status != "pending" {
            Some(chrono::Utc::now().to_rfc3339())
        } else {
            None
        };

        sqlx::query!(
            r#"
            UPDATE batch_job_items
            SET status = ?1,
                document_id = COALESCE(?2, document_id),
                error_message = ?3,
                processed_at = COALESCE(?4, processed_at)
            WHERE id = ?5
            "#,
            status,
            document_id,
            error_message,
            processed_at,
            item_id
        )
        .execute(&self.pool)
        .await
        .map_err(|e| AppError::Database(format!("Failed to update item status: {}", e)))?;

        Ok(())
    }

    async fn get_batch_job(&self, job_id: &str) -> Result<BatchJobStatus, AppError> {
        // Get job
        let job_row = sqlx::query(
            r#"
            SELECT id, job_type, status, total_items, completed_items, failed_items, progress,
                   created_at, started_at, completed_at, error_message
            FROM batch_jobs
            WHERE id = ?1
            "#,
        )
        .bind(job_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AppError::Database(format!("Failed to fetch batch job: {}", e)))?
        .ok_or_else(|| AppError::NotFound(format!("Batch job not found: {}", job_id)))?;

        // Get items
        let items = sqlx::query!(
            r#"
            SELECT id, item_url, document_id, status, error_message
            FROM batch_job_items
            WHERE job_id = ?1
            ORDER BY created_at ASC
            "#,
            job_id
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AppError::Database(format!("Failed to fetch batch items: {}", e)))?
        .into_iter()
        .map(|row| BatchJobItemStatus {
            id: row.id,
            url: row.item_url,
            document_id: row.document_id,
            status: row.status,
            error_message: row.error_message,
        })
        .collect();

        Ok(BatchJobStatus {
            id: job_row.get("id"),
            job_type: job_row.get("job_type"),
            status: job_row.get("status"),
            total_items: job_row.get("total_items"),
            completed_items: job_row.get("completed_items"),
            failed_items: job_row.get("failed_items"),
            progress: job_row.get("progress"),
            created_at: job_row.get("created_at"),
            started_at: job_row.get("started_at"),
            completed_at: job_row.get("completed_at"),
            error_message: job_row.get("error_message"),
            items,
        })
    }

    async fn get_pending_items(&self, job_id: &str) -> Result<Vec<BatchJobItem>, AppError> {
        let items = sqlx::query!(
            r#"
            SELECT id, item_url
            FROM batch_job_items
            WHERE job_id = ?1 AND status = 'pending'
            ORDER BY created_at ASC
            "#,
            job_id
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AppError::Database(format!("Failed to fetch pending items: {}", e)))?
        .into_iter()
        .map(|row| BatchJobItem {
            id: row.id,
            url: row.item_url,
        })
        .collect();

        Ok(items)
    }

    async fn cancel_pending_items(&self, job_id: &str) -> Result<usize, AppError> {
        let result = sqlx::query!(
            r#"
            UPDATE batch_job_items
            SET status = 'cancelled'
            WHERE job_id = ?1 AND status = 'pending'
            "#,
            job_id
        )
        .execute(&self.pool)
        .await
        .map_err(|e| AppError::Database(format!("Failed to cancel items: {}", e)))?;

        Ok(result.rows_affected() as usize)
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

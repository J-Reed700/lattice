use super::ops;
use crate::application::ports::batch_job_repository_port::{
    BatchJobItem, BatchJobRepositoryPort, BatchJobStatus, BatchJobSummary,
};
use crate::shared::error::AppError;
use async_trait::async_trait;
use sqlx::SqlitePool;

pub struct SqliteBatchJobRepository {
    pool: SqlitePool,
}

impl SqliteBatchJobRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl BatchJobRepositoryPort for SqliteBatchJobRepository {
    async fn requeue_failed_files(
        &self,
        job_id: &str,
        item_id: Option<&str>,
        replacement_path: Option<&str>,
    ) -> Result<usize, AppError> {
        let mut tx = self.pool.begin().await?;
        let count = ops::requeue_failed_files(&mut tx, job_id, item_id, replacement_path).await?;
        tx.commit().await?;
        Ok(count)
    }

    async fn get_job_options(&self, job_id: &str) -> Result<Option<String>, AppError> {
        let mut conn = self.pool.acquire().await?;
        ops::get_job_options(&mut conn, job_id).await
    }

    async fn create_batch_job(
        &self,
        job_id: &str,
        job_type: &str,
        total_items: i64,
        options: Option<&str>,
    ) -> Result<(), AppError> {
        let mut conn = self.pool.acquire().await?;
        ops::create_batch_job(&mut conn, job_id, job_type, total_items, options).await
    }

    async fn create_batch_items(&self, job_id: &str, urls: Vec<String>) -> Result<(), AppError> {
        let mut conn = self.pool.acquire().await?;
        ops::create_batch_items(&mut conn, job_id, urls).await
    }

    async fn update_job_status(
        &self,
        job_id: &str,
        status: &str,
        started_at: Option<String>,
        completed_at: Option<String>,
    ) -> Result<(), AppError> {
        let mut conn = self.pool.acquire().await?;
        ops::update_job_status(&mut conn, job_id, status, started_at, completed_at).await
    }

    async fn update_progress(
        &self,
        job_id: &str,
        completed: i64,
        failed: i64,
        progress: f64,
    ) -> Result<(), AppError> {
        let mut conn = self.pool.acquire().await?;
        ops::update_progress(&mut conn, job_id, completed, failed, progress).await
    }

    async fn update_item_status(
        &self,
        item_id: &str,
        status: &str,
        document_id: Option<&str>,
        error_message: Option<&str>,
    ) -> Result<(), AppError> {
        let mut conn = self.pool.acquire().await?;
        ops::update_item_status(&mut conn, item_id, status, document_id, error_message).await
    }

    async fn get_batch_job(&self, job_id: &str) -> Result<BatchJobStatus, AppError> {
        let mut tx = self.pool.begin().await?;
        let job = ops::get_batch_job(&mut tx, job_id).await?;
        tx.commit().await?;
        Ok(job)
    }

    async fn get_pending_items(&self, job_id: &str) -> Result<Vec<BatchJobItem>, AppError> {
        let mut conn = self.pool.acquire().await?;
        ops::get_pending_items(&mut conn, job_id).await
    }

    async fn cancel_pending_items(&self, job_id: &str) -> Result<usize, AppError> {
        let mut conn = self.pool.acquire().await?;
        ops::cancel_pending_items(&mut conn, job_id).await
    }

    async fn list_batch_jobs(
        &self,
        limit: Option<i64>,
        offset: Option<i64>,
    ) -> Result<Vec<BatchJobSummary>, AppError> {
        let mut conn = self.pool.acquire().await?;
        ops::list_batch_jobs(&mut conn, limit, offset).await
    }

    async fn delete_batch_job(&self, job_id: &str) -> Result<(), AppError> {
        let mut conn = self.pool.acquire().await?;
        ops::delete_batch_job(&mut conn, job_id).await
    }
}

use crate::shared::error::AppError;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Repository port for batch job persistence
#[async_trait]
pub trait BatchJobRepositoryPort: Send + Sync {
    /// Create a new batch job
    async fn create_batch_job(
        &self,
        job_id: &str,
        job_type: &str,
        total_items: i64,
        options: Option<&str>,
    ) -> Result<(), AppError>;

    /// Create batch job items
    async fn create_batch_items(&self, job_id: &str, urls: Vec<String>) -> Result<(), AppError>;

    /// Update batch job status
    async fn update_job_status(
        &self,
        job_id: &str,
        status: &str,
        started_at: Option<String>,
        completed_at: Option<String>,
    ) -> Result<(), AppError>;

    /// Update progress counters
    async fn update_progress(
        &self,
        job_id: &str,
        completed: i64,
        failed: i64,
        progress: f64,
    ) -> Result<(), AppError>;

    /// Update batch item status
    async fn update_item_status(
        &self,
        item_id: &str,
        status: &str,
        document_id: Option<&str>,
        error_message: Option<&str>,
    ) -> Result<(), AppError>;

    /// Get batch job with items
    async fn get_batch_job(&self, job_id: &str) -> Result<BatchJobStatus, AppError>;

    /// Get pending items for a job
    async fn get_pending_items(&self, job_id: &str) -> Result<Vec<BatchJobItem>, AppError>;

    /// Cancel all pending items
    async fn cancel_pending_items(&self, job_id: &str) -> Result<usize, AppError>;

    /// List all batch jobs (chronological, newest first)
    async fn list_batch_jobs(
        &self,
        limit: Option<i64>,
        offset: Option<i64>,
    ) -> Result<Vec<BatchJobSummary>, AppError>;

    /// Delete a batch job and its items (cascade)
    async fn delete_batch_job(&self, job_id: &str) -> Result<(), AppError>;
}

/// Lightweight job summary for history list
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchJobSummary {
    pub id: String,
    pub job_type: String,
    pub status: String,
    pub total_items: i64,
    pub completed_items: i64,
    pub failed_items: i64,
    pub progress: f64,
    pub created_at: String,
    pub completed_at: Option<String>,
}

/// Batch job status DTO for frontend
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchJobStatus {
    pub id: String,
    pub job_type: String,
    pub status: String,
    pub total_items: i64,
    pub completed_items: i64,
    pub failed_items: i64,
    pub progress: f64,
    pub created_at: String,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    pub error_message: Option<String>,
    pub items: Vec<BatchJobItemStatus>,
}

/// Batch job item status DTO
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchJobItemStatus {
    pub id: String,
    pub url: String,
    pub document_id: Option<String>,
    pub status: String,
    pub error_message: Option<String>,
}

/// Internal batch job item
#[derive(Debug, Clone)]
pub struct BatchJobItem {
    pub id: String,
    pub url: String,
}

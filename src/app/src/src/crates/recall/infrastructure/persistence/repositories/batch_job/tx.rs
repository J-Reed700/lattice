use super::ops;
use crate::application::ports::batch_job_repository_port::{
    BatchJobItem, BatchJobRepositoryPort, BatchJobStatus, BatchJobSummary,
};
use crate::shared::error::AppError;
use async_trait::async_trait;
use sqlx::{Sqlite, Transaction};
use std::sync::{Arc, Weak};
use tokio::sync::Mutex;

pub struct SqliteBatchJobRepositoryTx {
    transaction: Weak<Mutex<Transaction<'static, Sqlite>>>,
}

impl SqliteBatchJobRepositoryTx {
    pub fn new(transaction: Arc<Mutex<Transaction<'static, Sqlite>>>) -> Self {
        Self {
            transaction: Arc::downgrade(&transaction),
        }
    }

    fn get_transaction(&self) -> Result<Arc<Mutex<Transaction<'static, Sqlite>>>, AppError> {
        self.transaction
            .upgrade()
            .ok_or_else(|| AppError::InternalError("Transaction dropped".into()))
    }
}

#[async_trait]
impl BatchJobRepositoryPort for SqliteBatchJobRepositoryTx {
    async fn create_batch_job(
        &self,
        job_id: &str,
        job_type: &str,
        total_items: i64,
        options: Option<&str>,
    ) -> Result<(), AppError> {
        let tx_arc = self.get_transaction()?;
        let mut tx = tx_arc.lock().await;
        ops::create_batch_job(&mut tx, job_id, job_type, total_items, options).await
    }

    async fn create_batch_items(&self, job_id: &str, urls: Vec<String>) -> Result<(), AppError> {
        let tx_arc = self.get_transaction()?;
        let mut tx = tx_arc.lock().await;
        ops::create_batch_items(&mut tx, job_id, urls).await
    }

    async fn update_job_status(
        &self,
        job_id: &str,
        status: &str,
        started_at: Option<String>,
        completed_at: Option<String>,
    ) -> Result<(), AppError> {
        let tx_arc = self.get_transaction()?;
        let mut tx = tx_arc.lock().await;
        ops::update_job_status(&mut tx, job_id, status, started_at, completed_at).await
    }

    async fn update_progress(
        &self,
        job_id: &str,
        completed: i64,
        failed: i64,
        progress: f64,
    ) -> Result<(), AppError> {
        let tx_arc = self.get_transaction()?;
        let mut tx = tx_arc.lock().await;
        ops::update_progress(&mut tx, job_id, completed, failed, progress).await
    }

    async fn update_item_status(
        &self,
        item_id: &str,
        status: &str,
        document_id: Option<&str>,
        error_message: Option<&str>,
    ) -> Result<(), AppError> {
        let tx_arc = self.get_transaction()?;
        let mut tx = tx_arc.lock().await;
        ops::update_item_status(&mut tx, item_id, status, document_id, error_message).await
    }

    async fn get_batch_job(&self, job_id: &str) -> Result<BatchJobStatus, AppError> {
        let tx_arc = self.get_transaction()?;
        let mut tx = tx_arc.lock().await;
        ops::get_batch_job(&mut tx, job_id).await
    }

    async fn get_pending_items(&self, job_id: &str) -> Result<Vec<BatchJobItem>, AppError> {
        let tx_arc = self.get_transaction()?;
        let mut tx = tx_arc.lock().await;
        ops::get_pending_items(&mut tx, job_id).await
    }

    async fn cancel_pending_items(&self, job_id: &str) -> Result<usize, AppError> {
        let tx_arc = self.get_transaction()?;
        let mut tx = tx_arc.lock().await;
        ops::cancel_pending_items(&mut tx, job_id).await
    }

    async fn list_batch_jobs(
        &self,
        limit: Option<i64>,
        offset: Option<i64>,
    ) -> Result<Vec<BatchJobSummary>, AppError> {
        let tx_arc = self.get_transaction()?;
        let mut tx = tx_arc.lock().await;
        ops::list_batch_jobs(&mut tx, limit, offset).await
    }

    async fn delete_batch_job(&self, job_id: &str) -> Result<(), AppError> {
        let tx_arc = self.get_transaction()?;
        let mut tx = tx_arc.lock().await;
        ops::delete_batch_job(&mut tx, job_id).await
    }
}

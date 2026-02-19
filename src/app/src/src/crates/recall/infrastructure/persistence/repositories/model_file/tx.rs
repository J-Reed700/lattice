use crate::domain::entities::model_file::ModelFile;
use crate::domain::repositories::unit_of_work::ModelFileRepositoryPort;
use crate::domain::value_objects::model_status::FileStatus;
use crate::shared::error::{AppError, Result};
use async_trait::async_trait;
use sqlx::{Sqlite, Transaction};
use std::sync::{Arc, Weak};
use tokio::sync::Mutex;

use super::ops;

pub struct SqliteModelFileRepositoryTx {
    transaction: Weak<Mutex<Transaction<'static, Sqlite>>>,
}

impl SqliteModelFileRepositoryTx {
    pub fn new(transaction: Arc<Mutex<Transaction<'static, Sqlite>>>) -> Self {
        Self {
            transaction: Arc::downgrade(&transaction),
        }
    }

    fn get_transaction(&self) -> Result<Arc<Mutex<Transaction<'static, Sqlite>>>> {
        self.transaction
            .upgrade()
            .ok_or_else(|| AppError::InvalidState("Transaction dropped".into()))
    }

    pub async fn find_by_id(&self, file_id: &str) -> Result<Option<ModelFile>> {
        let tx_arc = self.get_transaction()?;
        let mut tx = tx_arc.lock().await;
        ops::find_by_id(&mut tx, file_id).await
    }

    pub async fn create(&self, model_file: &ModelFile) -> Result<()> {
        let tx_arc = self.get_transaction()?;
        let mut tx = tx_arc.lock().await;
        ops::create(&mut tx, model_file).await
    }

    pub async fn find_by_model_id(&self, model_id: &str) -> Result<Vec<ModelFile>> {
        let tx_arc = self.get_transaction()?;
        let mut tx = tx_arc.lock().await;
        ops::find_by_model_id(&mut tx, model_id).await
    }

    pub async fn update_file_status(&self, file_id: &str, status: FileStatus) -> Result<()> {
        let tx_arc = self.get_transaction()?;
        let mut tx = tx_arc.lock().await;
        ops::update_file_status(&mut tx, file_id, status).await
    }

    pub async fn count_completed_files(&self, model_id: &str) -> Result<usize> {
        let tx_arc = self.get_transaction()?;
        let mut tx = tx_arc.lock().await;
        ops::count_completed_files(&mut tx, model_id).await
    }

    pub async fn count_total_files(&self, model_id: &str) -> Result<usize> {
        let tx_arc = self.get_transaction()?;
        let mut tx = tx_arc.lock().await;
        ops::count_total_files(&mut tx, model_id).await
    }

    pub async fn update_file_status_by_model_and_name(
        &self,
        model_id: &str,
        file_name: &str,
        status: FileStatus,
        size_bytes: Option<i64>,
    ) -> Result<()> {
        let tx_arc = self.get_transaction()?;
        let mut tx = tx_arc.lock().await;
        ops::update_file_status_by_model_and_name(&mut tx, model_id, file_name, status, size_bytes)
            .await
    }
}

#[async_trait]
impl ModelFileRepositoryPort for SqliteModelFileRepositoryTx {
    async fn create(&self, model_file: &ModelFile) -> Result<()> {
        self.create(model_file).await
    }

    async fn find_by_model_id(&self, model_id: &str) -> Result<Vec<ModelFile>> {
        self.find_by_model_id(model_id).await
    }

    async fn count_total_files(&self, model_id: &str) -> Result<usize> {
        self.count_total_files(model_id).await
    }

    async fn count_completed_files(&self, model_id: &str) -> Result<usize> {
        self.count_completed_files(model_id).await
    }

    async fn update_file_status_by_model_and_name(
        &self,
        model_id: &str,
        file_name: &str,
        status: FileStatus,
        size_bytes: Option<i64>,
    ) -> Result<()> {
        self.update_file_status_by_model_and_name(model_id, file_name, status, size_bytes)
            .await
    }
}

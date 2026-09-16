use crate::domain::repositories::system_repository::SystemRepository;
use crate::shared::error::AppError;
use async_trait::async_trait;
use sqlx::{Sqlite, Transaction};
use std::sync::{Arc, Weak};
use tokio::sync::Mutex;

pub struct SqliteSystemRepositoryTx {
    tx: Weak<Mutex<Transaction<'static, Sqlite>>>,
}

impl SqliteSystemRepositoryTx {
    pub fn new(tx: Arc<Mutex<Transaction<'static, Sqlite>>>) -> Self {
        Self {
            tx: Arc::downgrade(&tx),
        }
    }

    fn get_transaction(&self) -> Result<Arc<Mutex<Transaction<'static, Sqlite>>>, AppError> {
        self.tx
            .upgrade()
            .ok_or_else(|| AppError::InternalError("Transaction dropped".into()))
    }
}

#[async_trait]
impl SystemRepository for SqliteSystemRepositoryTx {
    async fn vacuum(&self) -> Result<(), AppError> {
        let tx_arc = self.get_transaction()?;
        let mut tx = tx_arc.lock().await;
        super::ops::vacuum(&mut tx).await
    }

    async fn analyze(&self) -> Result<(), AppError> {
        let tx_arc = self.get_transaction()?;
        let mut tx = tx_arc.lock().await;
        super::ops::analyze(&mut tx).await
    }

    async fn get_database_size(&self) -> Result<i64, AppError> {
        let tx_arc = self.get_transaction()?;
        let mut tx = tx_arc.lock().await;
        super::ops::get_database_size(&mut tx).await
    }

    async fn integrity_check(&self) -> Result<bool, AppError> {
        let tx_arc = self.get_transaction()?;
        let mut tx = tx_arc.lock().await;
        super::ops::integrity_check(&mut tx).await
    }
}

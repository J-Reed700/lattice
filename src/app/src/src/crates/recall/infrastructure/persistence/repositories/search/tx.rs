use crate::domain::repositories::search_repository::{SearchRepository, SearchResult};
use crate::error::AppError;
use async_trait::async_trait;
use sqlx::{Sqlite, Transaction};
use std::sync::{Arc, Weak};
use tokio::sync::Mutex;

use super::ops;

#[derive(Debug)]
pub struct SqliteSearchRepositoryTx {
    transaction: Weak<Mutex<Transaction<'static, Sqlite>>>,
}

impl SqliteSearchRepositoryTx {
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
impl SearchRepository for SqliteSearchRepositoryTx {
    async fn search_bm25(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>, AppError> {
        let tx_arc = self.get_transaction()?;
        let mut tx = tx_arc.lock().await;
        ops::search_bm25(&mut tx, query, limit).await
    }

    async fn count_searchable_chunks(&self) -> Result<i64, AppError> {
        let tx_arc = self.get_transaction()?;
        let mut tx = tx_arc.lock().await;
        ops::count_searchable_chunks(&mut tx).await
    }

    async fn optimize_index(&self) -> Result<(), AppError> {
        let tx_arc = self.get_transaction()?;
        let mut tx = tx_arc.lock().await;
        ops::optimize_index(&mut tx).await
    }

    async fn rebuild_index(&self) -> Result<(), AppError> {
        let tx_arc = self.get_transaction()?;
        let mut tx = tx_arc.lock().await;
        ops::rebuild_index(&mut tx).await
    }
}

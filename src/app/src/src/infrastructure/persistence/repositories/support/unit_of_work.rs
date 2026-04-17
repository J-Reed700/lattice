//! SQLite Unit of Work Implementation
//!
//! # Transaction Lifetime Safety
//!
//! This implementation uses `Transaction<'static, Sqlite>` which appears unsafe
//! but is actually the correct design for SQLx's architecture:
//!
//! ## Why `Transaction<'static, Sqlite>` is Safe:
//!
//! 1. **SQLx's Design**: The `Pool::begin()` method returns `Transaction<'static, Sqlite>`
//!    by design. This is not a mistake - it's SQLx's explicit API contract.
//!
//! 2. **Owned Connection**: The transaction owns its database connection handle,
//!    retrieved from the pool. It's not borrowing from the pool with a limited lifetime.
//!
//! 3. **Drop Safety**: When the Transaction is dropped (via commit/rollback or panic),
//!    the connection is automatically returned to the pool. The 'static lifetime
//!    is bounded by the Transaction's Drop implementation.
//!
//! 4. **No Use-After-Free**: The pool itself is application-scoped (lives for the
//!    entire application lifetime), so there's no risk of the transaction outliving
//!    the pool.
//!
//! ## Architecture Pattern:
//!
//! - `SqliteUnitOfWork` holds `Transaction<'static, Sqlite>` (owned)
//! - Repository Tx structs borrow the transaction: `&'a mut Transaction<'static, Sqlite>`
//! - The borrow lifetime `'a` ensures repositories can't outlive the UnitOfWork
//! - The 'static on Transaction ensures the owned connection is valid
//!
//! This is the standard pattern for SQLx transaction management in Rust.

use async_trait::async_trait;
use sqlx::{Sqlite, SqlitePool, Transaction};
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::application::ports::{
    BatchJobRepositoryPort, ChunkRepositoryPort, DocumentRepositoryPort, EmbeddingRepositoryPort,
};
use crate::domain::repositories::{
    unit_of_work::{ModelFileRepositoryPort, ModelRepositoryPort},
    SearchRepository, SystemRepository, UnitOfWork as UnitOfWorkTrait,
    UnitOfWorkFactory as UnitOfWorkFactoryTrait,
};
use crate::shared::error::AppError;
use crate::shared::result::Result;

use super::batch_job::SqliteBatchJobRepositoryTx;
use super::chunk::SqliteChunkRepositoryTx;
use super::document::SqliteDocumentRepositoryTx;
use crate::features::embedding::repository_tx::SqliteEmbeddingRepositoryTx;
use super::model::SqliteModelRepositoryTx;
use super::model_file::SqliteModelFileRepositoryTx;
use super::search::SqliteSearchRepositoryTx;
use super::system::SqliteSystemRepositoryTx;

pub struct SqliteUnitOfWork {
    pub(crate) transaction: Option<Arc<Mutex<Transaction<'static, Sqlite>>>>,
}

impl SqliteUnitOfWork {
    fn new(transaction: Transaction<'static, Sqlite>) -> Self {
        Self {
            transaction: Some(Arc::new(Mutex::new(transaction))),
        }
    }

    fn transaction(&self) -> Result<Arc<Mutex<Transaction<'static, Sqlite>>>> {
        self.transaction
            .clone()
            .ok_or_else(|| AppError::Database("Transaction already consumed".to_string()))
    }

    fn take_unique_transaction(&mut self) -> Result<Transaction<'static, Sqlite>> {
        let transaction_arc = self
            .transaction
            .as_ref()
            .ok_or_else(|| AppError::Database("Transaction already consumed".to_string()))?;

        // If repositories are still alive, keep transaction in place so caller can retry.
        if Arc::strong_count(transaction_arc) > 1 {
            return Err(AppError::Database(
                "Transaction still has references".to_string(),
            ));
        }

        let transaction_arc = self
            .transaction
            .take()
            .ok_or_else(|| AppError::Database("Transaction already consumed".to_string()))?;

        match Arc::try_unwrap(transaction_arc) {
            Ok(mutex) => Ok(mutex.into_inner()),
            Err(transaction_arc) => {
                self.transaction = Some(transaction_arc);
                Err(AppError::Database(
                    "Transaction still has references".to_string(),
                ))
            }
        }
    }
}

#[async_trait]
impl UnitOfWorkTrait for SqliteUnitOfWork {
    fn chunk_repository(&self) -> Result<Box<dyn ChunkRepositoryPort + Send + '_>> {
        let tx = self
            .transaction
            .as_ref()
            .ok_or_else(|| AppError::InvalidState("Transaction already consumed".to_string()))?
            .clone();
        Ok(Box::new(SqliteChunkRepositoryTx::new(tx)))
    }

    fn document_repository(&self) -> Result<Box<dyn DocumentRepositoryPort + Send + '_>> {
        let tx = self
            .transaction
            .as_ref()
            .ok_or_else(|| AppError::InvalidState("Transaction already consumed".to_string()))?
            .clone();
        Ok(Box::new(SqliteDocumentRepositoryTx::new(tx)))
    }

    fn embedding_repository(&self) -> Result<Box<dyn EmbeddingRepositoryPort + Send + '_>> {
        let tx = self
            .transaction
            .as_ref()
            .ok_or_else(|| AppError::InvalidState("Transaction already consumed".to_string()))?
            .clone();
        Ok(Box::new(SqliteEmbeddingRepositoryTx::new(tx)))
    }

    fn search_repository(&self) -> Result<Box<dyn SearchRepository + Send + '_>> {
        let tx = self
            .transaction
            .as_ref()
            .ok_or_else(|| AppError::InvalidState("Transaction already consumed".to_string()))?
            .clone();
        Ok(Box::new(SqliteSearchRepositoryTx::new(tx)))
    }

    fn batch_job_repository(&self) -> Result<Box<dyn BatchJobRepositoryPort + Send + '_>> {
        let tx = self
            .transaction
            .as_ref()
            .ok_or_else(|| AppError::InvalidState("Transaction already consumed".to_string()))?
            .clone();
        let repository: Box<dyn BatchJobRepositoryPort + Send + '_> =
            Box::new(SqliteBatchJobRepositoryTx::new(tx));
        Ok(repository)
    }

    fn system_repository(&self) -> Result<Box<dyn SystemRepository + Send + '_>> {
        let tx = self
            .transaction
            .as_ref()
            .ok_or_else(|| AppError::InvalidState("Transaction already consumed".to_string()))?
            .clone();
        Ok(Box::new(SqliteSystemRepositoryTx::new(tx)))
    }

    fn model_repository(&self) -> Result<Box<dyn ModelRepositoryPort + '_>> {
        let tx = self
            .transaction
            .as_ref()
            .ok_or_else(|| AppError::InvalidState("Transaction already consumed".to_string()))?
            .clone();
        Ok(Box::new(SqliteModelRepositoryTx::new(tx)))
    }

    fn model_file_repository(&self) -> Result<Box<dyn ModelFileRepositoryPort + '_>> {
        let tx = self
            .transaction
            .as_ref()
            .ok_or_else(|| AppError::InvalidState("Transaction already consumed".to_string()))?
            .clone();
        Ok(Box::new(SqliteModelFileRepositoryTx::new(tx)))
    }

    async fn commit(&mut self) -> Result<()> {
        let transaction = self.take_unique_transaction()?;

        transaction
            .commit()
            .await
            .map_err(|e| AppError::Database(format!("Failed to commit transaction: {}", e)))
    }

    async fn rollback(&mut self) -> Result<()> {
        let transaction = self.take_unique_transaction()?;

        transaction
            .rollback()
            .await
            .map_err(|e| AppError::Database(format!("Failed to rollback transaction: {}", e)))
    }
}

impl Drop for SqliteUnitOfWork {
    fn drop(&mut self) {
        if self.transaction.is_some() {
            eprintln!("Warning: SqliteUnitOfWork dropped without explicit commit/rollback - transaction will be rolled back by SQLx on connection return");
        }
    }
}

pub struct SqliteUnitOfWorkFactory {
    pool: SqlitePool,
}

impl SqliteUnitOfWorkFactory {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl UnitOfWorkFactoryTrait for SqliteUnitOfWorkFactory {
    async fn create(&self) -> Result<Box<dyn UnitOfWorkTrait + Send>> {
        let transaction: Transaction<'static, Sqlite> = self
            .pool
            .begin()
            .await
            .map_err(|e| AppError::Database(format!("Failed to begin transaction: {}", e)))?;

        Ok(Box::new(SqliteUnitOfWork::new(transaction)))
    }
}

#[cfg(test)]
mod tests {
    use super::SqliteUnitOfWork;
    use crate::domain::repositories::UnitOfWork as UnitOfWorkTrait;
    use sqlx::SqlitePool;

    #[tokio::test]
    async fn commit_with_live_reference_does_not_consume_transaction() {
        let pool = SqlitePool::connect(":memory:").await.unwrap();
        let tx = pool.begin().await.unwrap();
        let mut uow = SqliteUnitOfWork::new(tx);

        let live_ref = uow.transaction.as_ref().unwrap().clone();

        let commit_err = UnitOfWorkTrait::commit(&mut uow).await.unwrap_err();
        assert!(
            commit_err
                .to_string()
                .contains("Transaction still has references"),
            "unexpected commit error: {}",
            commit_err
        );
        assert!(
            uow.transaction.is_some(),
            "transaction should remain available after failed commit"
        );

        drop(live_ref);
        UnitOfWorkTrait::rollback(&mut uow).await.unwrap();
    }

    #[tokio::test]
    async fn rollback_with_live_reference_does_not_consume_transaction() {
        let pool = SqlitePool::connect(":memory:").await.unwrap();
        let tx = pool.begin().await.unwrap();
        let mut uow = SqliteUnitOfWork::new(tx);

        let live_ref = uow.transaction.as_ref().unwrap().clone();

        let rollback_err = UnitOfWorkTrait::rollback(&mut uow).await.unwrap_err();
        assert!(
            rollback_err
                .to_string()
                .contains("Transaction still has references"),
            "unexpected rollback error: {}",
            rollback_err
        );
        assert!(
            uow.transaction.is_some(),
            "transaction should remain available after failed rollback"
        );

        drop(live_ref);
        UnitOfWorkTrait::rollback(&mut uow).await.unwrap();
    }
}

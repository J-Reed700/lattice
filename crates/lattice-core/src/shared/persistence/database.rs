//! The database as code outside a data module holds it.
use async_trait::async_trait;
use sqlx::SqlitePool;

use super::error::PersistenceError;
use super::transactions::{ReadTx, Transactions, WriteTx};

/// The application's database: it begins units of work and closes; only a
/// data module reaches the driver's pool inside it.
#[derive(Clone, Debug)]
pub struct Database(pub(super) SqlitePool);

impl Database {
    /// Whether a connection can be had.
    pub async fn is_reachable(&self) -> bool {
        self.0.acquire().await.is_ok()
    }

    pub async fn close(&self) {
        self.0.close().await;
    }

    /// Connections the pool holds open, for shutdown diagnostics.
    pub fn open_connections(&self) -> u32 {
        self.0.size()
    }
}

impl From<SqlitePool> for Database {
    fn from(pool: SqlitePool) -> Self {
        Self(pool)
    }
}

#[async_trait]
impl Transactions for Database {
    async fn begin_write(&self) -> Result<WriteTx, PersistenceError> {
        self.0.begin_write().await
    }

    async fn begin_read(&self) -> Result<ReadTx, PersistenceError> {
        self.0.begin_read().await
    }
}

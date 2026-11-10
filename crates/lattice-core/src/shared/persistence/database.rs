//! The database as code outside a data module holds it.
use async_trait::async_trait;
use sqlx::SqlitePool;

use super::transactions::{ReadTx, Transactions, WriteTx};

/// The application's database: it begins units of work and closes; only a
/// data module reaches the driver's pool inside it.
#[derive(Clone, Debug)]
pub struct Database(SqlitePool);

impl Database {
    /// Whether a connection can be had.
    pub async fn is_reachable(&self) -> bool {
        self.0.acquire().await.is_ok()
    }

    pub async fn close(&self) {
        self.0.close().await;
    }
}

impl From<SqlitePool> for Database {
    fn from(pool: SqlitePool) -> Self {
        Self(pool)
    }
}

/// What a data module's function takes: the database, or the driver's pool itself.
pub trait DatabaseHandle: Transactions + Send + Sync + std::fmt::Debug {
    /// The driver's pool. Called only in data modules.
    fn sqlx_pool(&self) -> &SqlitePool;

    /// The database, for code outside data modules to hold.
    fn database(&self) -> Database;
}

impl DatabaseHandle for Database {
    fn sqlx_pool(&self) -> &SqlitePool {
        &self.0
    }

    fn database(&self) -> Database {
        self.clone()
    }
}

impl DatabaseHandle for SqlitePool {
    fn sqlx_pool(&self) -> &SqlitePool {
        self
    }

    fn database(&self) -> Database {
        Database(self.clone())
    }
}

#[async_trait]
impl Transactions for Database {
    async fn begin_write(&self) -> Result<WriteTx, sqlx::Error> {
        self.0.begin_write().await
    }

    async fn begin_read(&self) -> Result<ReadTx, sqlx::Error> {
        self.0.begin_read().await
    }
}

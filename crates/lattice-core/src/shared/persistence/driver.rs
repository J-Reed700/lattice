//! The database driver, which only data modules (and tests) may name.
use sqlx::SqlitePool;

use super::database::Database;
use super::error::PersistenceError;
use super::transactions::Transactions;

/// What a data module's function takes: the database, or the driver's pool itself.
pub trait DriverPool: Transactions + Send + Sync + std::fmt::Debug {
    fn sqlx_pool(&self) -> &SqlitePool;
}

impl DriverPool for Database {
    fn sqlx_pool(&self) -> &SqlitePool {
        &self.0
    }
}

impl DriverPool for SqlitePool {
    fn sqlx_pool(&self) -> &SqlitePool {
        self
    }
}

impl From<PersistenceError> for sqlx::Error {
    fn from(error: PersistenceError) -> Self {
        error.0
    }
}

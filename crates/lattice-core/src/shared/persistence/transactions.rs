//! Transactions that say whether they write.
//!
//! In WAL mode a deferred transaction that has read and then writes while
//! another connection holds the write lock fails at once with SQLITE_BUSY:
//! `busy_timeout` does not apply, because waiting could deadlock. A
//! transaction that writes therefore takes the write lock when it begins,
//! where `busy_timeout` does apply. One that never writes stays deferred and
//! off the lock.
use async_trait::async_trait;
use sqlx::{Sqlite, SqlitePool, Transaction};

#[async_trait]
pub trait Transactions {
    /// Begins a transaction that writes, holding the write lock from the start.
    async fn begin_write(&self) -> Result<Transaction<'static, Sqlite>, sqlx::Error>;

    /// Begins a transaction that never writes.
    async fn begin_read(&self) -> Result<Transaction<'static, Sqlite>, sqlx::Error>;
}

#[async_trait]
impl Transactions for SqlitePool {
    async fn begin_write(&self) -> Result<Transaction<'static, Sqlite>, sqlx::Error> {
        self.begin_with("BEGIN IMMEDIATE").await
    }

    async fn begin_read(&self) -> Result<Transaction<'static, Sqlite>, sqlx::Error> {
        self.begin().await
    }
}

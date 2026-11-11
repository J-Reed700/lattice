//! Transactions that say whether they write.
//!
//! In WAL mode a deferred transaction that has read and then writes while
//! another connection holds the write lock fails at once with SQLITE_BUSY:
//! `busy_timeout` does not apply, because waiting could deadlock. A
//! transaction that writes therefore takes the write lock when it begins,
//! where `busy_timeout` does apply. One that never writes stays deferred and
//! off the lock.
use std::ops::{Deref, DerefMut};

use super::error::PersistenceError;
use async_trait::async_trait;
use sqlx::{Sqlite, SqliteConnection, SqlitePool, Transaction};

/// A unit of work that writes; dropping it without `commit` rolls back.
pub struct WriteTx(Transaction<'static, Sqlite>);

/// A unit of work that never writes; dropping it without `commit` rolls back.
pub struct ReadTx(Transaction<'static, Sqlite>);

impl WriteTx {
    pub async fn commit(self) -> Result<(), PersistenceError> {
        Ok(self.0.commit().await?)
    }

    pub async fn rollback(self) -> Result<(), PersistenceError> {
        Ok(self.0.rollback().await?)
    }
}

impl ReadTx {
    pub async fn commit(self) -> Result<(), PersistenceError> {
        Ok(self.0.commit().await?)
    }

    pub async fn rollback(self) -> Result<(), PersistenceError> {
        Ok(self.0.rollback().await?)
    }
}

impl Deref for WriteTx {
    type Target = SqliteConnection;

    fn deref(&self) -> &SqliteConnection {
        &self.0
    }
}

impl DerefMut for WriteTx {
    fn deref_mut(&mut self) -> &mut SqliteConnection {
        &mut self.0
    }
}

impl Deref for ReadTx {
    type Target = SqliteConnection;

    fn deref(&self) -> &SqliteConnection {
        &self.0
    }
}

impl DerefMut for ReadTx {
    fn deref_mut(&mut self) -> &mut SqliteConnection {
        &mut self.0
    }
}

#[async_trait]
pub trait Transactions {
    /// Begins a transaction that writes, holding the write lock from the start.
    async fn begin_write(&self) -> Result<WriteTx, PersistenceError>;

    /// Begins a transaction that never writes.
    async fn begin_read(&self) -> Result<ReadTx, PersistenceError>;
}

#[async_trait]
impl Transactions for SqlitePool {
    async fn begin_write(&self) -> Result<WriteTx, PersistenceError> {
        let tx = self.begin_with("BEGIN IMMEDIATE").await?;
        Ok(WriteTx(tx))
    }

    async fn begin_read(&self) -> Result<ReadTx, PersistenceError> {
        let tx = self.begin().await?;
        Ok(ReadTx(tx))
    }
}

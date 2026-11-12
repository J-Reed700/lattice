//! Units of work that say whether they write.
//!
//! In WAL mode a deferred transaction that has read and then writes while
//! another connection holds the write lock fails at once with SQLITE_BUSY:
//! `busy_timeout` does not apply, because waiting could deadlock. A unit that
//! writes therefore takes the write lock when it begins. One that never writes
//! stays deferred and off the lock.
use std::ops::{Deref, DerefMut};

use sqlx::{Sqlite, SqliteConnection, SqlitePool, Transaction};

use super::error::PersistenceError;
use super::write_turns::WriteTurn;

/// A unit of work that writes. It holds the write turn from its begin until it
/// commits, rolls back or is dropped; dropping it without `commit` rolls back.
pub struct WriteTx {
    tx: Transaction<'static, Sqlite>,
    turn: WriteTurn,
}

/// A unit of work that never writes; dropping it without `commit` rolls back.
pub struct ReadTx {
    tx: Transaction<'static, Sqlite>,
    changed_before: i64,
}

impl WriteTx {
    pub(super) async fn start(
        pool: &SqlitePool,
        turn: WriteTurn,
    ) -> Result<Self, PersistenceError> {
        let tx = pool.begin_with("BEGIN IMMEDIATE").await?;
        Ok(Self { tx, turn })
    }

    pub async fn commit(self) -> Result<(), PersistenceError> {
        let Self { tx, turn } = self;
        let committed = tx.commit().await;
        drop(turn);
        Ok(committed?)
    }

    pub async fn rollback(self) -> Result<(), PersistenceError> {
        let Self { tx, turn } = self;
        let rolled_back = tx.rollback().await;
        drop(turn);
        Ok(rolled_back?)
    }
}

impl ReadTx {
    pub(super) async fn start(pool: &SqlitePool) -> Result<Self, PersistenceError> {
        let mut tx = pool.begin().await?;
        let changed_before = changed_rows(&mut tx).await?;
        Ok(Self { tx, changed_before })
    }

    /// Commits, unless the unit changed rows: those it rolls back, and fails.
    pub async fn commit(mut self) -> Result<(), PersistenceError> {
        if changed_rows(&mut self.tx).await? != self.changed_before {
            self.tx.rollback().await?;
            return Err(PersistenceError::changed_while_reading());
        }
        Ok(self.tx.commit().await?)
    }

    pub async fn rollback(self) -> Result<(), PersistenceError> {
        Ok(self.tx.rollback().await?)
    }
}

/// Rows the connection's statements have inserted, updated or deleted since it
/// opened.
async fn changed_rows(connection: &mut SqliteConnection) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar("SELECT total_changes()")
        .fetch_one(connection)
        .await
}

impl Deref for WriteTx {
    type Target = SqliteConnection;

    fn deref(&self) -> &SqliteConnection {
        &self.tx
    }
}

impl DerefMut for WriteTx {
    fn deref_mut(&mut self) -> &mut SqliteConnection {
        &mut self.tx
    }
}

impl Deref for ReadTx {
    type Target = SqliteConnection;

    fn deref(&self) -> &SqliteConnection {
        &self.tx
    }
}

impl DerefMut for ReadTx {
    fn deref_mut(&mut self) -> &mut SqliteConnection {
        &mut self.tx
    }
}

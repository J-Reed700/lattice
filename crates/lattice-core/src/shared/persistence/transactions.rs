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
pub struct ReadTx(Transaction<'static, Sqlite>);

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
        Ok(Self(pool.begin().await?))
    }

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
        &self.0
    }
}

impl DerefMut for ReadTx {
    fn deref_mut(&mut self) -> &mut SqliteConnection {
        &mut self.0
    }
}

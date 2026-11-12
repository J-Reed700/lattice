//! The database. The only way to a connection is a unit of work, and every
//! unit that writes waits its turn.
use std::future::Future;
use std::panic::Location;
use std::sync::Arc;

use sqlx::SqlitePool;

use super::error::PersistenceError;
use super::transactions::{ReadTx, WriteTx};
use super::write_turns::WriteTurns;

/// One database file. Clones share its write queue.
#[derive(Clone, Debug)]
pub struct Database {
    pool: SqlitePool,
    turns: Arc<WriteTurns>,
}

impl Database {
    /// The database over a pool opened on one file. Make one per file: a second
    /// `Database` on the same file would have a write queue of its own.
    pub fn new(pool: SqlitePool) -> Self {
        Self {
            pool,
            turns: Arc::new(WriteTurns::new()),
        }
    }

    /// Begins a unit of work that writes. It waits behind every unit that asked
    /// to write before it, holding no connection while it waits, then takes
    /// SQLite's write lock as it begins.
    #[track_caller]
    pub fn begin_write(
        &self,
    ) -> impl Future<Output = Result<WriteTx, PersistenceError>> + Send + '_ {
        let at = Location::caller();
        async move {
            let turn = self.turns.take(at).await?;
            WriteTx::start(&self.pool, turn).await
        }
    }

    /// Begins a unit of work that only reads. Readers take no turn and never
    /// wait for writers.
    pub async fn begin_read(&self) -> Result<ReadTx, PersistenceError> {
        ReadTx::start(&self.pool).await
    }

    /// Copies the whole database into a new file (`VACUUM INTO`). It only
    /// reads this database, so writers carry on meanwhile.
    pub async fn vacuum_into(&self, destination: &str) -> Result<(), PersistenceError> {
        let mut connection = self.pool.acquire().await?;
        sqlx::query("VACUUM INTO ?")
            .bind(destination)
            .execute(&mut *connection)
            .await?;
        Ok(())
    }

    /// Rewrites the file without its free pages. SQLite refuses it inside a
    /// transaction, and it writes the whole file, so it waits its turn.
    #[track_caller]
    pub fn vacuum(&self) -> impl Future<Output = Result<(), PersistenceError>> + Send + '_ {
        let at = Location::caller();
        async move {
            let _turn = self.turns.take(at).await?;
            let mut connection = self.pool.acquire().await?;
            sqlx::query("VACUUM").execute(&mut *connection).await?;
            Ok(())
        }
    }

    /// Whether a connection can be had.
    pub async fn is_reachable(&self) -> bool {
        self.pool.acquire().await.is_ok()
    }

    /// Ends every writer still waiting for its turn with `Closed`, then waits
    /// for the connections in use to come back and closes the pool.
    pub async fn close(&self) {
        self.turns.close();
        self.pool.close().await;
    }

    /// Connections the pool holds open, for shutdown diagnostics.
    pub fn open_connections(&self) -> u32 {
        self.pool.size()
    }
}

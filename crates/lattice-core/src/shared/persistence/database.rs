//! The database. The only way to a connection is a unit of work, and every
//! unit that writes waits its turn.
use std::future::Future;
use std::panic::Location;
use std::sync::Arc;

use sqlx::SqlitePool;

use super::error::PersistenceError;
use super::transactions::{disable_query_only, is_query_only, ReadTx, WriteTx};
use super::write_turns::WriteTurns;

/// One database file. Clones share its write queue.
#[derive(Clone, Debug)]
pub struct Database {
    reader_pool: SqlitePool,
    writer_pool: Option<SqlitePool>,
    turns: Arc<WriteTurns>,
}

impl Database {
    /// The database over a single pool (for tests / in-memory pools where
    /// separate pools would not share `:memory:`). Make one per file: a second
    /// `Database` on the same file would have a write queue of its own.
    pub fn new(pool: SqlitePool) -> Self {
        Self {
            reader_pool: pool,
            writer_pool: None,
            turns: Arc::new(WriteTurns::new()),
        }
    }

    /// The database with a reader pool and a dedicated writer pool. Make one
    /// per file: a second `Database` on the same file would have a write queue
    /// of its own.
    pub fn with_writer_pool(reader_pool: SqlitePool, writer_pool: SqlitePool) -> Self {
        Self {
            reader_pool,
            writer_pool: Some(writer_pool),
            turns: Arc::new(WriteTurns::new()),
        }
    }

    fn writer(&self) -> &SqlitePool {
        self.writer_pool.as_ref().unwrap_or(&self.reader_pool)
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
            WriteTx::start(self.writer(), turn).await
        }
    }

    /// Begins a unit of work that only reads. Readers take no turn and never
    /// wait for writers.
    pub async fn begin_read(&self) -> Result<ReadTx, PersistenceError> {
        ReadTx::start(&self.reader_pool).await
    }

    /// Copies the whole database into a new file (`VACUUM INTO`). It only
    /// reads this database, so writers carry on meanwhile.
    pub async fn vacuum_into(&self, destination: &str) -> Result<(), PersistenceError> {
        let mut connection = self.reader_pool.acquire().await?;
        let mut conn = if is_query_only(&mut connection).await? {
            // Detach before disabling query_only so task cancellation mid-VACUUM
            // can never return a writable connection to the read-only pool.
            let mut detached = connection.detach();
            disable_query_only(&mut detached).await?;
            VacuumConn::Detached(detached)
        } else {
            VacuumConn::Pooled(connection)
        };
        let result = sqlx::query("VACUUM INTO ?")
            .bind(destination)
            .execute(conn.as_mut())
            .await;
        if let VacuumConn::Detached(detached) = conn {
            let _ = sqlx::Connection::close(detached).await;
        }
        result?;
        Ok(())
    }

    /// Rewrites the file without its free pages. SQLite refuses it inside a
    /// transaction, and it writes the whole file, so it waits its turn.
    #[track_caller]
    pub fn vacuum(&self) -> impl Future<Output = Result<(), PersistenceError>> + Send + '_ {
        let at = Location::caller();
        async move {
            let _turn = self.turns.take(at).await?;
            let mut connection = self.writer().acquire().await?;
            sqlx::query("VACUUM").execute(&mut *connection).await?;
            Ok(())
        }
    }

    /// Whether a connection can be had.
    pub async fn is_reachable(&self) -> bool {
        self.reader_pool.acquire().await.is_ok()
    }

    /// Ends every writer still waiting for its turn with `Closed`, then waits
    /// for the connections in use to come back and closes the pool(s).
    pub async fn close(&self) {
        self.turns.close();
        if let Some(writer_pool) = &self.writer_pool {
            tokio::join!(self.reader_pool.close(), writer_pool.close());
        } else {
            self.reader_pool.close().await;
        }
    }

    /// Connections the pool(s) hold open, for shutdown diagnostics.
    pub fn open_connections(&self) -> u32 {
        self.reader_pool.size() + self.writer_pool.as_ref().map_or(0, SqlitePool::size)
    }
}

enum VacuumConn {
    Pooled(sqlx::pool::PoolConnection<sqlx::Sqlite>),
    Detached(sqlx::SqliteConnection),
}

impl VacuumConn {
    fn as_mut(&mut self) -> &mut sqlx::SqliteConnection {
        match self {
            Self::Pooled(conn) => conn,
            Self::Detached(conn) => conn,
        }
    }
}

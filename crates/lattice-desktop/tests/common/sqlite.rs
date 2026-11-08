//! Test databases opened the way production opens its own.

use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqliteSynchronous};
use std::{path::Path, time::Duration};

/// Options for a database file in a test, set the way production sets its own:
/// write-ahead log, normal sync, foreign keys on and a 5 s busy timeout.
pub fn file_options(path: impl AsRef<Path>) -> SqliteConnectOptions {
    SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true)
        .foreign_keys(true)
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Normal)
        .busy_timeout(Duration::from_secs(5))
}

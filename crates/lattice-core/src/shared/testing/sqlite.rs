//! Test databases at the app's current schema, opened the way production opens its own.

use crate::shared::error::{AppError, Result};
use crate::shared::hash::sha256_hex;
use sqlx::migrate::Migrator;
use sqlx::sqlite::{
    SqliteConnectOptions, SqliteJournalMode, SqliteOwnedBuf, SqlitePoolOptions, SqliteSynchronous,
};
use sqlx::{Connection, SqliteConnection, SqlitePool};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::sync::OnceCell;

static MIGRATOR: Migrator = sqlx::migrate!("../lattice-desktop/migrations");
static IMAGE: OnceCell<Vec<u8>> = OnceCell::const_new();
const IMAGE_PREFIX: &str = "lattice-schema-";

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

/// A database of the test's own at the current schema, held in memory on one connection.
pub async fn migrated_in_memory() -> Result<SqlitePool> {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        // A long model call must not retire the only in-memory database connection.
        .idle_timeout(None)
        .max_lifetime(None)
        .connect(":memory:")
        .await?;
    let image = SqliteOwnedBuf::try_from(migrated_image().await?)?;
    pool.acquire()
        .await?
        .deserialize(None, image, false)
        .await?;
    Ok(pool)
}

/// Writes a database at the current schema to `path`, for a test to open with [`file_options`].
pub async fn write_migrated(path: impl AsRef<Path>) -> Result<()> {
    std::fs::write(path, migrated_image().await?)?;
    Ok(())
}

/// The migrated database's bytes. The first test process of a build to need them runs the
/// migrations and leaves the image beside the test binary for every later process.
async fn migrated_image() -> Result<&'static [u8]> {
    IMAGE
        .get_or_try_init(|| async {
            let path = image_path()?;
            if let Ok(image) = std::fs::read(&path) {
                return Ok(image);
            }
            let mut connection = SqliteConnection::connect(":memory:").await?;
            MIGRATOR
                .run(&mut connection)
                .await
                .map_err(|e| AppError::Database(e.to_string()))?;
            let image = connection.serialize(None).await?.to_vec();
            keep_image(&path, &image)?;
            Ok(image)
        })
        .await
        .map(Vec::as_slice)
}

fn image_path() -> Result<PathBuf> {
    let mut migrations = Vec::new();
    for migration in MIGRATOR.iter() {
        migrations.extend_from_slice(&migration.version.to_le_bytes());
        migrations.extend_from_slice(&migration.checksum);
    }
    let name = format!("{IMAGE_PREFIX}{}.sqlite", sha256_hex(migrations));
    Ok(std::env::current_exe()?.with_file_name(name))
}

/// Saves the image whole or not at all, and removes the images of older migration sets.
fn keep_image(path: &Path, image: &[u8]) -> Result<()> {
    let partial = path.with_extension(format!("partial-{}", std::process::id()));
    std::fs::write(&partial, image)?;
    if std::fs::rename(&partial, path).is_err() {
        std::fs::remove_file(&partial).ok();
        return Ok(());
    }
    let Some(directory) = path.parent() else {
        return Ok(());
    };
    for entry in std::fs::read_dir(directory)?.flatten() {
        let other = entry.path();
        let stale = other != path
            && entry
                .file_name()
                .to_string_lossy()
                .starts_with(IMAGE_PREFIX)
            && other
                .extension()
                .is_some_and(|extension| extension == "sqlite");
        if stale {
            std::fs::remove_file(&other).ok();
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;

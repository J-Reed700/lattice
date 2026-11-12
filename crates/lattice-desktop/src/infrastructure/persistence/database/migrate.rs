//! Database migration utilities for upgrading existing databases
//!
//! This module provides a simple wrapper around sqlx's migration functionality.
//! Migrations are automatically discovered from the migrations/ directory.

use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::persistence::{Database, WriteTx};
use sha2::{Digest, Sha384};
use sqlx::migrate::Migrator;
use std::collections::HashMap;

/// Run all pending migrations from the migrations/ directory
///
/// This function uses sqlx::migrate!() to automatically discover and run
/// migration files from the migrations/ directory at compile time. Stored
/// checksums that differ from the embedded ones only in line endings are
/// reconciled first. It is all one write unit: a start that fails changes nothing.
pub async fn run_migrations(db: &Database) -> Result<()> {
    tracing::info!("Starting database migrations...");

    let migrator = sqlx::migrate!("./migrations");
    let mut tx = db.begin_write().await?;
    let reconciled = reconcile_line_endings(&mut tx, &migrator).await?;
    migrator
        .run_direct(None, &mut *tx, false)
        .await
        .map_err(|error| {
            let error_text = error.to_string();
            tracing::error!("Migration failed: {}", error_text);
            AppError::Database(format!("Migration failed: {}", error_text))
        })?;

    // Migrations may create or change indexes; let SQLite refresh only the
    // planner statistics that its bounded optimizer considers useful.
    sqlx::query("PRAGMA optimize").execute(&mut *tx).await?;
    tx.commit().await?;

    for version in reconciled {
        tracing::info!(
            version,
            "Migration checksum updated to this build's line endings"
        );
    }
    tracing::info!("All migrations completed successfully");
    Ok(())
}

/// Rewrites the stored checksum of an applied migration to the embedded one when it is the
/// checksum of the same SQL with the other line endings, and returns the versions it rewrote.
/// Any other difference is left for sqlx to refuse. Does nothing before `_sqlx_migrations`
/// exists.
pub(crate) async fn reconcile_line_endings(
    tx: &mut WriteTx,
    migrator: &Migrator,
) -> Result<Vec<i64>> {
    let has_table: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = '_sqlx_migrations'",
    )
    .fetch_one(&mut **tx)
    .await?;
    if has_table == 0 {
        return Ok(Vec::new());
    }

    let stored: HashMap<i64, Vec<u8>> =
        sqlx::query_as::<_, (i64, Vec<u8>)>("SELECT version, checksum FROM _sqlx_migrations")
            .fetch_all(&mut **tx)
            .await?
            .into_iter()
            .collect();

    let mut reconciled = Vec::new();
    for migration in migrator
        .iter()
        .filter(|migration| !migration.migration_type.is_down_migration())
    {
        let Some(checksum) = stored.get(&migration.version) else {
            continue;
        };
        if checksum.as_slice() == &*migration.checksum
            || !is_line_ending_variant(checksum, migration.sql.as_str())
        {
            continue;
        }
        let updated = sqlx::query(
            "UPDATE _sqlx_migrations SET checksum = ?1 WHERE version = ?2 AND checksum = ?3",
        )
        .bind(&*migration.checksum)
        .bind(migration.version)
        .bind(checksum.as_slice())
        .execute(&mut **tx)
        .await?;
        if updated.rows_affected() == 1 {
            reconciled.push(migration.version);
        }
    }
    Ok(reconciled)
}

/// Whether `checksum` is the SHA-384 of `sql` with every line ending LF or with every one CRLF.
fn is_line_ending_variant(checksum: &[u8], sql: &str) -> bool {
    let lf = sql.replace("\r\n", "\n");
    let crlf = lf.replace('\n', "\r\n");
    [lf, crlf]
        .iter()
        .any(|variant| Sha384::digest(variant.as_bytes()).as_slice() == checksum)
}

#[cfg(test)]
mod tests;

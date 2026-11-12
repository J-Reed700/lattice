//! The database statements of a snapshot: copying the live database, clearing
//! the derivable tables in the copy, and reading facts back from it.
use std::collections::HashSet;
use std::path::Path;

use lattice_core::shared::persistence::{Database, WriteTx};
use sqlx::SqliteConnection;
use tracing::{debug, warn};

use super::{db_err, fts_external_content_table, is_blob_hash, SnapshotInfo, EXCLUDED_TABLES};
use crate::features::backup::archive::format::ArchiveError;
use crate::features::backup::database_repository;
use crate::infrastructure::persistence::database::{open_to_change, open_to_read};

/// `VACUUM INTO dest_str` of the live database, onto `dest`.
pub(super) async fn copy_database(
    database: &Database,
    dest: &Path,
    dest_str: &str,
) -> Result<(), ArchiveError> {
    database.vacuum_into(dest_str).await.map_err(|error| {
        ArchiveError::Database(format!("VACUUM INTO {} failed: {error}", dest.display()))
    })
}

/// Scratch database over the copy, foreign keys off: every excluded table is
/// re-derivable, so we do not care about the order the rows disappear in.
/// (For the record, `PRAGMA foreign_key_list` on them gives
/// text_embeddings -> text_chunks, image_embeddings -> documents,
/// conversation_memory_vectors -> conversations, cluster_members ->
/// clusters/documents, clusters -> cluster_runs.
/// The list above is already child-before-parent, so the order would be
/// safe even with enforcement on.)
pub(super) async fn open_snapshot_copy(dest: &Path) -> Result<Database, ArchiveError> {
    open_to_change(dest)
        .await
        .map_err(|error| ArchiveError::Database(format!("failed to open snapshot copy: {error}")))
}

/// Does `table` exist in the connected database?
async fn table_exists(
    connection: &mut SqliteConnection,
    table: &str,
) -> Result<bool, ArchiveError> {
    database_repository::table_exists(connection, table)
        .await
        .map_err(db_err)
}

/// Deals with the FTS5 indexes after the excluded tables have been emptied.
/// They are ordinary FTS5 tables kept in sync by triggers on kept tables, so
/// their content is carried verbatim: a rebuild would only re-derive what is
/// copied, and a delete would lose text no restore trigger rebuilds. The loop
/// handles a future external-content index over an excluded table by
/// rebuilding it.
async fn reconcile_fts_tables(
    tx: &mut WriteTx,
    cleared: &[String],
) -> Result<Vec<String>, ArchiveError> {
    let rows = database_repository::fts5_tables(&mut *tx)
        .await
        .map_err(db_err)?;

    let mut rebuilt = Vec::new();
    for (name, sql) in rows {
        let Some(sql) = sql else { continue };
        let Some(content_table) = fts_external_content_table(&sql) else {
            debug!(table = %name, "fts5 index stores its own content; kept verbatim");
            continue;
        };
        if !cleared.iter().any(|t| t == &content_table) {
            debug!(
                table = %name,
                content = %content_table,
                "external-content fts5 index over a table we keep; left alone"
            );
            continue;
        }
        warn!(
            table = %name,
            content = %content_table,
            "external-content fts5 index over a cleared table; rebuilding it empty"
        );
        database_repository::rebuild_fts5_index(tx, &name)
            .await
            .map_err(db_err)?;
        rebuilt.push(name);
    }
    Ok(rebuilt)
}

/// Everything that happens on the scratch pool, so the caller can always
/// close it afterwards.
pub(super) async fn finish_snapshot(
    database: &Database,
) -> Result<(Vec<String>, Option<i64>), ArchiveError> {
    let cleared = clear_snapshot_tables(database).await?;

    // Reclaim the pages the deletes freed. SQLite refuses VACUUM inside a
    // transaction, so it runs after the commit.
    database
        .vacuum()
        .await
        .map_err(|error| ArchiveError::Database(format!("VACUUM of snapshot failed: {error}")))?;

    let mut read = database.begin_read().await.map_err(db_err)?;
    let migration_version = if table_exists(&mut read, "_sqlx_migrations").await? {
        database_repository::newest_migration(&mut read)
            .await
            .map_err(db_err)?
    } else {
        None
    };
    read.commit().await.map_err(db_err)?;

    Ok((cleared, migration_version))
}

/// Empties the excluded tables in one unit of work and returns the ones cleared.
async fn clear_snapshot_tables(database: &Database) -> Result<Vec<String>, ArchiveError> {
    let mut tx = database.begin_write().await.map_err(db_err)?;
    let mut cleared: Vec<String> = Vec::new();
    for table in EXCLUDED_TABLES {
        if !database_repository::table_exists(&mut tx, table)
            .await
            .map_err(db_err)?
        {
            debug!(table = %table, "excluded table absent from snapshot; skipping");
            continue;
        }
        database_repository::clear_table(&mut tx, table)
            .await
            .map_err(|error| ArchiveError::Database(format!("failed to clear {table}: {error}")))?;
        cleared.push((*table).to_string());
    }

    reconcile_fts_tables(&mut tx, &cleared).await?;
    tx.commit().await.map_err(db_err)?;
    Ok(cleared)
}

/// Does `table` have a column called `column`?
async fn column_exists(
    connection: &mut SqliteConnection,
    table: &str,
    column: &str,
) -> Result<bool, ArchiveError> {
    database_repository::column_exists(connection, table, column)
        .await
        .map_err(db_err)
}

/// The blob hashes the archived database still points at.
///
/// Read from the snapshot rather than the live database, so the set and the
/// `documents` rows inside the archive can never disagree: a document deleted
/// after `VACUUM INTO` is not in the snapshot, and its blob is not packed.
///
/// `Ok(None)` means "this database cannot say" — no `documents` table, or no
/// `checksum` column — and the planner then packs every blob directory.
/// Shipping a few orphans is a much better failure than silently dropping the
/// user's files out of their only backup.
pub async fn referenced_blob_hashes(db: &Path) -> Result<Option<HashSet<String>>, ArchiveError> {
    let database = open_to_read(db).await.map_err(|error| {
        ArchiveError::Database(format!(
            "failed to open snapshot {} to list referenced blobs: {error}",
            db.display()
        ))
    })?;

    let result = read_referenced_blob_hashes(&database).await;
    database.close().await;
    result
}

async fn read_referenced_blob_hashes(
    database: &Database,
) -> Result<Option<HashSet<String>>, ArchiveError> {
    let mut read = database.begin_read().await.map_err(db_err)?;
    if !table_exists(&mut read, "documents").await? {
        warn!("snapshot has no documents table; every library blob will be packed");
        return Ok(None);
    }
    if !column_exists(&mut read, "documents", "checksum").await? {
        warn!("snapshot documents table has no checksum column; every library blob will be packed");
        return Ok(None);
    }

    let checksums = database_repository::distinct_checksums(&mut read)
        .await
        .map_err(db_err)?;
    read.commit().await.map_err(db_err)?;

    let total = checksums.len();
    let hashes: HashSet<String> = checksums
        .into_iter()
        .flatten()
        .map(|checksum| checksum.trim().to_ascii_lowercase())
        .filter(|checksum| is_blob_hash(checksum))
        .collect();
    debug!(
        distinct_checksums = total,
        referenced_blobs = hashes.len(),
        "read the referenced blob set from the snapshot"
    );
    Ok(Some(hashes))
}

async fn count_rows(connection: &mut SqliteConnection, table: &str) -> Result<u64, ArchiveError> {
    if !table_exists(connection, table).await? {
        return Ok(0);
    }
    let count = database_repository::count_rows(connection, table)
        .await
        .map_err(db_err)?;
    Ok(u64::try_from(count).unwrap_or(0))
}

/// `PRAGMA integrity_check` plus a few counts on a snapshot file.
pub async fn validate_snapshot(db: &Path) -> Result<SnapshotInfo, ArchiveError> {
    if !db.is_file() {
        return Err(ArchiveError::Corrupt(format!(
            "snapshot {} is missing",
            db.display()
        )));
    }

    let database = open_to_read(db).await.map_err(|error| {
        ArchiveError::Corrupt(format!("{} is not a database: {error}", db.display()))
    })?;

    let result = inspect_snapshot(&database).await;
    database.close().await;
    result
}

async fn inspect_snapshot(database: &Database) -> Result<SnapshotInfo, ArchiveError> {
    let mut read = database.begin_read().await.map_err(db_err)?;
    let integrity = database_repository::integrity_check(&mut read)
        .await
        .map_err(|error| ArchiveError::Corrupt(format!("integrity check failed: {error}")))?;
    if integrity != "ok" {
        return Err(ArchiveError::Corrupt(format!(
            "snapshot failed integrity check: {integrity}"
        )));
    }

    let migration_version = if table_exists(&mut read, "_sqlx_migrations").await? {
        database_repository::newest_migration(&mut read)
            .await
            .map_err(db_err)?
    } else {
        None
    };

    let tables = database_repository::table_count(&mut read)
        .await
        .map_err(db_err)?;

    let memory_tables_present = table_exists(&mut read, "conversation_memory_state").await?
        && table_exists(&mut read, "conversation_memory_items").await?
        && table_exists(&mut read, "conversation_memory_evidence").await?;
    let memory_schema_version = if memory_tables_present {
        database_repository::newest_memory_schema_version(&mut read)
            .await
            .map_err(db_err)?
    } else {
        None
    };

    let info = SnapshotInfo {
        migration_version,
        table_count: u64::try_from(tables).unwrap_or(0),
        document_count: count_rows(&mut read, "documents").await?,
        conversation_count: count_rows(&mut read, "conversations").await?,
        memory_tables_present,
        memory_schema_version,
    };
    read.commit().await.map_err(db_err)?;
    Ok(info)
}

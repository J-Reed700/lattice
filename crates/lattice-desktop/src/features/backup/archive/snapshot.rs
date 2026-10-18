//! Payload side of a `.lattice-backup` archive: the SQLite snapshot with
//! derivable tables cleared, plus the files library, vault, saved web
//! articles, and settings, packed as a zstd-compressed tar.
//!
//! Layout of the plaintext payload (see `format.rs` for the framing):
//!
//! ```text
//! zstd( tar( manifest.json, db/lattice.db, files/**, vault/**, web-archive/**,
//!            settings.json ) )
//! ```
//!
//! `manifest.json` is always the first tar entry so a reader can learn the
//! counts and byte totals without decompressing the rest of the stream.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};
use sqlx::SqlitePool;
use tracing::{debug, info, warn};

use super::format::ArchiveError;

mod extract;
mod payload;

pub use extract::{extract_payload, read_manifest, ExtractedPayload};
pub use payload::{write_payload, ArchiveInputs, ProgressEvent};

/// Tables whose rows are cleared from the snapshot. All are re-derivable
/// from documents, chunks and conversations after a restore.
pub const EXCLUDED_TABLES: &[&str] = &[
    "text_embeddings",
    "image_embeddings",
    "conversation_memory_vectors",
    "chunk_sparse_terms",
    "cluster_members",
    "clusters",
    "cluster_runs",
    "chat_starter_cache",
];

/// zstd compression level for the payload. 3 is the zstd default: it keeps
/// the CPU cost low enough that a scheduled backup stays invisible, while
/// still roughly halving a SQLite snapshot.
const ZSTD_LEVEL: i32 = 3;

/// Hard ceiling on the manifest entry, so a hostile payload cannot make the
/// reader allocate arbitrarily before the rest of the tar is even inspected.
const MAX_MANIFEST_BYTES: u64 = 4 * 1024 * 1024;

/// Sidecar suffixes SQLite may leave next to a database file.
const DB_SIDECAR_SUFFIXES: [&str; 3] = ["-wal", "-shm", "-journal"];

/// A SHA-256 digest rendered as hex: the name of a library blob directory,
/// and the value of `documents.checksum`.
const BLOB_HASH_LEN: usize = 64;

/// True for the only names the content-addressed library gives a top-level
/// directory: 64 lowercase hex characters. `ContentAddressedStorage` writes
/// them with `format!("{:x}", digest)`, so anything else under the library
/// root is not a blob and is never packed.
fn is_blob_hash(name: &str) -> bool {
    name.len() == BLOB_HASH_LEN && name.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// What `snapshot_database` produced.
#[derive(Debug, Clone)]
pub struct SnapshotReport {
    pub path: PathBuf,
    pub bytes: u64,
    pub excluded_tables: Vec<String>,
    pub migration_version: Option<i64>,
}

fn db_err(err: impl std::fmt::Display) -> ArchiveError {
    ArchiveError::Database(err.to_string())
}

/// Reject the handful of characters that could end a `VACUUM INTO '...'`
/// statement early. Mirrors `BackupAdapter::validate_sql_safe_path`: the real
/// protection is the single-quote escaping below, this is defence in depth.
fn validate_sql_safe_path(path: &str) -> Result<(), ArchiveError> {
    if path.contains(';') {
        return Err(ArchiveError::Other(
            "snapshot path contains SQL statement terminator (;)".to_string(),
        ));
    }
    if path.contains("--") {
        return Err(ArchiveError::Other(
            "snapshot path contains SQL comment sequence (--)".to_string(),
        ));
    }
    if path.contains("/*") || path.contains("*/") {
        return Err(ArchiveError::Other(
            "snapshot path contains SQL comment sequence (/* */)".to_string(),
        ));
    }
    if path.contains('\0') {
        return Err(ArchiveError::Other(
            "snapshot path contains null byte".to_string(),
        ));
    }
    Ok(())
}

/// Append a suffix to a path's file name (`foo.db` -> `foo.db-wal`).
fn sidecar_path(db: &Path, suffix: &str) -> PathBuf {
    let mut raw = db.as_os_str().to_os_string();
    raw.push(suffix);
    PathBuf::from(raw)
}

/// Delete any `-wal`/`-shm`/`-journal` file left beside `db`. A snapshot must
/// be a single self-contained file: the tar only carries `db/lattice.db`, so a
/// stray sidecar would silently make the archived database stale.
fn remove_db_sidecars(db: &Path) {
    for suffix in DB_SIDECAR_SUFFIXES {
        let path = sidecar_path(db, suffix);
        if !path.exists() {
            continue;
        }
        match std::fs::remove_file(&path) {
            Ok(()) => debug!(path = %path.display(), "removed snapshot sidecar"),
            Err(error) => warn!(
                path = %path.display(),
                %error,
                "failed to remove snapshot sidecar; the archive may be incomplete"
            ),
        }
    }
}

/// Does `table` exist in the connected database?
async fn table_exists(pool: &SqlitePool, table: &str) -> Result<bool, ArchiveError> {
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?")
            .bind(table)
            .fetch_one(pool)
            .await
            .map_err(db_err)?;
    Ok(count > 0)
}

/// Extract the table named by an `content='...'` FTS5 option, if any.
fn fts_external_content_table(sql: &str) -> Option<String> {
    let lowered = sql.to_lowercase();
    let at = lowered.find("content=")?;
    let rest = sql.get(at.checked_add("content=".len())?..)?;
    let mut chars = rest.chars();
    let quote = chars.next()?;
    if quote != '\'' && quote != '"' && quote != '`' {
        return None;
    }
    let value: String = chars.take_while(|c| *c != quote).collect();
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}

/// Deal with the FTS5 indexes after the excluded tables have been emptied.
///
/// Decision, from reading the schema (`migrations/20260916000000_init_schema.sql`):
///
/// `chunks_fts`, `documents_fts` and `conversation_search_fts` are *ordinary*
/// FTS5 tables. None of them declares a `content=` option, so each stores its
/// own text in its `%_content` shadow table, and each is kept in sync by
/// AFTER INSERT/UPDATE/DELETE triggers on `text_chunks`, `conversations`,
/// `conversation_messages` and `conversation_message_bookmarks` — every one of
/// which is a table we *keep*. Consequently:
///
/// * they are not external-content indexes over a cleared table, so they are
///   not stale after the deletes above;
/// * `INSERT INTO x(x) VALUES('rebuild')` would only re-derive the index from
///   the very shadow content we are already copying — pure cost, no benefit;
/// * `DELETE FROM x` would throw the text away with nothing left to rebuild
///   it from, since no trigger fires during a restore (the base rows are
///   copied by the snapshot, not re-inserted), permanently breaking search.
///
/// So the FTS content is carried verbatim. The loop below only exists so that
/// a *future* migration introducing `content='<an excluded table>'` is handled
/// correctly (rebuild against the now-empty content table) instead of shipping
/// an index that points at rows the archive no longer has.
async fn reconcile_fts_tables(
    pool: &SqlitePool,
    cleared: &[String],
) -> Result<Vec<String>, ArchiveError> {
    let rows: Vec<(String, Option<String>)> = sqlx::query_as(
        "SELECT name, sql FROM sqlite_master \
         WHERE type = 'table' AND sql IS NOT NULL AND lower(sql) LIKE '%using fts5%'",
    )
    .fetch_all(pool)
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
        let escaped = name.replace('"', "\"\"");
        sqlx::query(&format!(
            "INSERT INTO \"{escaped}\"(\"{escaped}\") VALUES('rebuild')"
        ))
        .execute(pool)
        .await
        .map_err(db_err)?;
        rebuilt.push(name);
    }
    Ok(rebuilt)
}

/// `VACUUM INTO dest` from the live pool, then clear `EXCLUDED_TABLES` (and
/// FTS shadow content where needed) in the copy and `VACUUM` it so the file
/// shrinks. The result has no `-wal`/`-shm` sidecars.
pub async fn snapshot_database(
    pool: &SqlitePool,
    dest: &Path,
) -> Result<SnapshotReport, ArchiveError> {
    let dest_str = dest.to_string_lossy().to_string();
    validate_sql_safe_path(&dest_str)?;

    if dest.exists() {
        return Err(ArchiveError::Other(format!(
            "snapshot destination already exists: {}",
            dest.display()
        )));
    }
    if let Some(parent) = dest.parent() {
        if !parent.as_os_str().is_empty() {
            tokio::fs::create_dir_all(parent).await?;
        }
    }

    // `VACUUM INTO` takes a string literal, not a bind parameter. Escape the
    // single quotes exactly like `BackupAdapter::create_backup` does.
    let escaped = dest_str.replace('\'', "''");
    sqlx::query(&format!("VACUUM INTO '{escaped}'"))
        .execute(pool)
        .await
        .map_err(|error| {
            ArchiveError::Database(format!("VACUUM INTO {} failed: {error}", dest.display()))
        })?;

    // Scratch pool over the copy. One connection, rollback journal (so no
    // `-wal`/`-shm` appear), foreign keys off: every excluded table is
    // re-derivable, so we do not care about the order the rows disappear in.
    // (For the record, `PRAGMA foreign_key_list` on them gives
    // text_embeddings -> text_chunks, image_embeddings -> documents,
    // conversation_memory_vectors -> conversations, chunk_sparse_terms ->
    // text_chunks, cluster_members -> clusters/documents, clusters ->
    // cluster_runs.
    // The list above is already child-before-parent, so the order would be
    // safe even with enforcement on.)
    let options = SqliteConnectOptions::new()
        .filename(dest)
        .create_if_missing(false)
        .journal_mode(SqliteJournalMode::Delete)
        .foreign_keys(false);
    let scratch = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .map_err(|error| {
            ArchiveError::Database(format!("failed to open snapshot copy: {error}"))
        })?;

    let outcome = clear_snapshot(&scratch).await;
    scratch.close().await;
    let (excluded_tables, migration_version) = match outcome {
        Ok(value) => value,
        Err(error) => {
            remove_db_sidecars(dest);
            return Err(error);
        }
    };

    remove_db_sidecars(dest);

    let bytes = tokio::fs::metadata(dest).await?.len();
    info!(
        path = %dest.display(),
        bytes,
        cleared = excluded_tables.len(),
        migration_version,
        "prepared backup database snapshot"
    );

    Ok(SnapshotReport {
        path: dest.to_path_buf(),
        bytes,
        excluded_tables,
        migration_version,
    })
}

/// Everything that happens on the scratch pool, so the caller can always
/// close it afterwards.
async fn clear_snapshot(pool: &SqlitePool) -> Result<(Vec<String>, Option<i64>), ArchiveError> {
    let mut cleared: Vec<String> = Vec::new();
    for table in EXCLUDED_TABLES {
        if !table_exists(pool, table).await? {
            debug!(table = %table, "excluded table absent from snapshot; skipping");
            continue;
        }
        let escaped = table.replace('"', "\"\"");
        sqlx::query(&format!("DELETE FROM \"{escaped}\""))
            .execute(pool)
            .await
            .map_err(|error| ArchiveError::Database(format!("failed to clear {table}: {error}")))?;
        cleared.push((*table).to_string());
    }

    reconcile_fts_tables(pool, &cleared).await?;

    // Reclaim the pages the deletes freed. Must run outside a transaction,
    // which it does here because sqlx executes it on a bare connection.
    sqlx::query("VACUUM")
        .execute(pool)
        .await
        .map_err(|error| ArchiveError::Database(format!("VACUUM of snapshot failed: {error}")))?;

    let migration_version = if table_exists(pool, "_sqlx_migrations").await? {
        sqlx::query_scalar::<_, Option<i64>>("SELECT MAX(version) FROM _sqlx_migrations")
            .fetch_one(pool)
            .await
            .map_err(db_err)?
    } else {
        None
    };

    Ok((cleared, migration_version))
}

/// Does `table` have a column called `column`?
async fn column_exists(pool: &SqlitePool, table: &str, column: &str) -> Result<bool, ArchiveError> {
    // `pragma_table_info` is a table-valued function, so its argument is a
    // string literal rather than a bind parameter; the quote doubling is the
    // same defence `VACUUM INTO` above uses.
    let escaped = table.replace('\'', "''");
    let names: Vec<String> =
        sqlx::query_scalar(&format!("SELECT name FROM pragma_table_info('{escaped}')"))
            .fetch_all(pool)
            .await
            .map_err(db_err)?;
    Ok(names.iter().any(|name| name == column))
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
    // Read-only with a rollback journal, exactly like `validate_snapshot`:
    // reading the set must not leave a `-wal`/`-shm` beside the file that is
    // about to be packed.
    let options = SqliteConnectOptions::new()
        .filename(db)
        .read_only(true)
        .create_if_missing(false)
        .journal_mode(SqliteJournalMode::Delete)
        .foreign_keys(false);
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .map_err(|error| {
            ArchiveError::Database(format!(
                "failed to open snapshot {} to list referenced blobs: {error}",
                db.display()
            ))
        })?;

    let result = read_referenced_blob_hashes(&pool).await;
    pool.close().await;
    result
}

async fn read_referenced_blob_hashes(
    pool: &SqlitePool,
) -> Result<Option<HashSet<String>>, ArchiveError> {
    if !table_exists(pool, "documents").await? {
        warn!("snapshot has no documents table; every library blob will be packed");
        return Ok(None);
    }
    if !column_exists(pool, "documents", "checksum").await? {
        warn!("snapshot documents table has no checksum column; every library blob will be packed");
        return Ok(None);
    }

    let checksums: Vec<Option<String>> =
        sqlx::query_scalar("SELECT DISTINCT checksum FROM documents")
            .fetch_all(pool)
            .await
            .map_err(db_err)?;

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

/// Facts about an extracted snapshot, checked before it is swapped in.
#[derive(Debug, Clone)]
pub struct SnapshotInfo {
    pub migration_version: Option<i64>,
    pub table_count: u64,
    pub document_count: u64,
    pub conversation_count: u64,
    /// Whether the snapshot carries the conversation-memory ledger at all.
    ///
    /// `false` means the backup predates bounded memory. That is restorable —
    /// the ledger is derived and rebuilds from the transcript — and is a
    /// different situation from a ledger this build cannot read.
    pub memory_tables_present: bool,
    /// Highest `conversation_memory_state.schema_version` in the snapshot.
    ///
    /// Checked separately from `migration_version` because the schema is one
    /// squashed migration that is edited in place: two builds can share a
    /// migration version and still disagree about the memory layout, so the
    /// migration number alone cannot tell them apart.
    pub memory_schema_version: Option<i64>,
}

async fn count_rows(pool: &SqlitePool, table: &str) -> Result<u64, ArchiveError> {
    if !table_exists(pool, table).await? {
        return Ok(0);
    }
    let escaped = table.replace('"', "\"\"");
    let count: i64 = sqlx::query_scalar(&format!("SELECT COUNT(*) FROM \"{escaped}\""))
        .fetch_one(pool)
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

    // Read-only, rollback journal: never create a `-wal`/`-shm` beside a file
    // that may live on read-only or sync-backed storage.
    let options = SqliteConnectOptions::new()
        .filename(db)
        .read_only(true)
        .create_if_missing(false)
        .journal_mode(SqliteJournalMode::Delete)
        .foreign_keys(false);
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .map_err(|error| {
            ArchiveError::Corrupt(format!("{} is not a database: {error}", db.display()))
        })?;

    let result = inspect_snapshot(&pool).await;
    pool.close().await;
    result
}

async fn inspect_snapshot(pool: &SqlitePool) -> Result<SnapshotInfo, ArchiveError> {
    let integrity: String = sqlx::query_scalar("PRAGMA integrity_check")
        .fetch_one(pool)
        .await
        .map_err(|error| ArchiveError::Corrupt(format!("integrity check failed: {error}")))?;
    if integrity != "ok" {
        return Err(ArchiveError::Corrupt(format!(
            "snapshot failed integrity check: {integrity}"
        )));
    }

    let migration_version = if table_exists(pool, "_sqlx_migrations").await? {
        sqlx::query_scalar::<_, Option<i64>>("SELECT MAX(version) FROM _sqlx_migrations")
            .fetch_one(pool)
            .await
            .map_err(db_err)?
    } else {
        None
    };

    let tables: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sqlite_master WHERE type = 'table'")
        .fetch_one(pool)
        .await
        .map_err(db_err)?;

    let memory_tables_present = table_exists(pool, "conversation_memory_state").await?
        && table_exists(pool, "conversation_memory_items").await?
        && table_exists(pool, "conversation_memory_evidence").await?;
    let memory_schema_version = if memory_tables_present {
        sqlx::query_scalar::<_, Option<i64>>(
            "SELECT MAX(schema_version) FROM conversation_memory_state",
        )
        .fetch_one(pool)
        .await
        .map_err(db_err)?
    } else {
        None
    };

    Ok(SnapshotInfo {
        migration_version,
        table_count: u64::try_from(tables).unwrap_or(0),
        document_count: count_rows(pool, "documents").await?,
        conversation_count: count_rows(pool, "conversations").await?,
        memory_tables_present,
        memory_schema_version,
    })
}

#[cfg(test)]
mod tests;

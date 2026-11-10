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

use std::path::{Path, PathBuf};

use tracing::{debug, info, warn};

use super::format::ArchiveError;
use lattice_core::shared::persistence::Database;

mod extract;
mod payload;
mod store;

pub use extract::{extract_payload, read_manifest, ExtractedPayload};
pub use payload::{write_payload, ArchiveInputs, ProgressEvent};
pub use store::{referenced_blob_hashes, validate_snapshot};

/// Tables whose rows are cleared from the snapshot. All are re-derivable
/// from documents, chunks and conversations after a restore.
pub const EXCLUDED_TABLES: &[&str] = &[
    "text_embeddings",
    "image_embeddings",
    "conversation_memory_vectors",
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

/// `VACUUM INTO dest` from the live database, then clear `EXCLUDED_TABLES` (and
/// FTS shadow content where needed) in the copy and `VACUUM` it so the file
/// shrinks. The result has no `-wal`/`-shm` sidecars.
pub async fn snapshot_database(
    database: &Database,
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

    store::copy_database(database, dest, &dest_str).await?;

    let scratch = store::open_snapshot_copy(dest).await?;

    let outcome = store::finish_snapshot(&scratch).await;
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

#[cfg(test)]
mod tests;

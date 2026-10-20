//! The index directories under one base, read without opening them.
//!
//! Each directory's `passages.db` is read through a short-lived connection
//! of its own, so a store some build or search holds open is untouched. What
//! the meta keys mean is the owner's: the registry hands them over as they
//! are stored.

use super::store::{connect, db_error, read_counts, read_meta, Counts, DB_FILE};
use lattice_core::shared::{AppError, Result};
use sqlx::Connection;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// One index directory as the registry sees it.
#[derive(Debug, Clone)]
pub struct Entry {
    pub dir: PathBuf,
    /// Its store's meta; empty for a directory whose `passages.db` is
    /// missing or unreadable.
    pub meta: HashMap<String, String>,
}

/// What one index directory holds under a prefix.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Inspected {
    pub counts: Counts,
    pub meta: HashMap<String, String>,
}

/// Every index directory under `base`.
pub async fn list(base: &Path) -> Vec<Entry> {
    let Ok(entries) = std::fs::read_dir(base) else {
        return Vec::new();
    };
    let mut indexes = Vec::new();
    for entry in entries.filter_map(|entry| entry.ok()) {
        let dir = entry.path();
        if !dir.is_dir() {
            continue;
        }
        let meta = match connect(&dir).await {
            Some(mut connection) => {
                let meta = read_meta(&mut connection).await;
                let _ = connection.close().await;
                meta.unwrap_or_default()
            }
            None => HashMap::new(),
        };
        indexes.push(Entry { dir, meta });
    }
    indexes
}

/// Reads `dir`'s index under `prefix` without opening it for writing.
/// `Ok(None)` when the directory has no `passages.db`.
pub async fn inspect(dir: &Path, prefix: &str) -> Result<Option<Inspected>> {
    if !dir.join(DB_FILE).exists() {
        return Ok(None);
    }
    let mut connection = connect(dir).await.ok_or_else(|| {
        AppError::Database(format!(
            "Passage index: could not open {}",
            dir.join(DB_FILE).display()
        ))
    })?;
    let read = async {
        let meta = read_meta(&mut connection).await?;
        let counts = read_counts(&mut connection, prefix).await?;
        Ok::<_, sqlx::Error>(Inspected { counts, meta })
    }
    .await;
    let _ = connection.close().await;
    read.map(Some).map_err(db_error("read the index"))
}

/// The document keys `dir`'s index holds, read without opening it for
/// writing. `Ok(None)` when the directory has no `passages.db`.
pub async fn document_keys(dir: &Path) -> Result<Option<Vec<String>>> {
    if !dir.join(DB_FILE).exists() {
        return Ok(None);
    }
    let mut connection = connect(dir).await.ok_or_else(|| {
        AppError::Database(format!(
            "Passage index: could not open {}",
            dir.join(DB_FILE).display()
        ))
    })?;
    let keys = sqlx::query_scalar::<_, String>("SELECT key FROM index_documents")
        .fetch_all(&mut connection)
        .await;
    let _ = connection.close().await;
    keys.map(Some)
        .map_err(db_error("read the index's documents"))
}

/// Bytes on disk of the files directly in `dir`: `passages.db` with its
/// journal, and the vectors files. 0 when it does not exist.
pub fn dir_bytes(dir: &Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    entries
        .filter_map(|entry| entry.ok()?.metadata().ok())
        .filter(|metadata| metadata.is_file())
        .map(|metadata| metadata.len())
        .sum()
}

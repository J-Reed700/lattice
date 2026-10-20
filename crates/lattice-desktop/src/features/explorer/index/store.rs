//! What a folder's index means on top of the shared passage store.
//!
//! Each indexed folder gets a directory under `<data_dir>/folder-index/`,
//! named for its root, holding a passage index
//! (`crate::infrastructure::passage_index`). Nothing goes into `lattice.db`:
//! backups copy that file whole and would carry every indexed repository,
//! and a separate store cannot leak into library search.
//!
//! A document is a file, keyed by its `/`-separated path under the index
//! root and stamped `size:mtime`; its passages are spans of lines. Each store
//! records its root, when it was last opened, and what its last run found
//! (too many files, or finished), so a closed folder can be described and a
//! parent's index found by reading a handful of small databases. The folders
//! list in `lattice.db` names the folders; these directories hold what each
//! index is.

use crate::infrastructure::passage_index::registry;
use crate::infrastructure::passage_index::store::Counts;
use lattice_core::shared::Result;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicI64, Ordering};

pub const META_ROOT: &str = "root";
pub const META_LAST_OPENED: &str = "last_opened";
/// The last walk found more files than the cap: `"<count>"`, or
/// `"<count>+"` when it stopped counting. Cleared by a walk that fits.
pub const META_TOO_LARGE: &str = "too_large";
/// When a full run last finished, so an index with nothing pending reads as
/// done rather than never run.
pub const META_COMPLETE: &str = "complete";

/// The index directory for a canonical root: the first 16 hex digits of the
/// root's SHA-256, so the name says nothing about the folder.
pub fn index_dir(base: &Path, root: &Path) -> PathBuf {
    let digest = Sha256::digest(root.as_os_str().as_encoded_bytes());
    let name: String = digest
        .iter()
        .take(8)
        .map(|byte| format!("{byte:02x}"))
        .collect();
    base.join(name)
}

/// A file's stamp in the store: its size and modification time, which a
/// walk compares before reading anything.
pub fn file_stamp(size: i64, mtime: i64) -> String {
    format!("{size}:{mtime}")
}

/// When folders were opened: milliseconds since the epoch, strictly increasing
/// across every stamp one source hands out, so two folders opened in the same
/// millisecond still have an order. The folder index manager owns the one its
/// folders are stamped from.
#[derive(Debug, Default)]
pub struct OpenStamps {
    last: AtomicI64,
}

impl OpenStamps {
    pub fn next(&self) -> i64 {
        let now = chrono::Utc::now().timestamp_millis();
        let mut previous = self.last.load(Ordering::SeqCst);
        loop {
            let stamp = now.max(previous + 1);
            match self
                .last
                .compare_exchange(previous, stamp, Ordering::SeqCst, Ordering::SeqCst)
            {
                Ok(_) => return stamp,
                Err(actual) => previous = actual,
            }
        }
    }
}

/// What a walk past the file bound found, as [`META_TOO_LARGE`] records it:
/// every indexable file, counted in full, of which the first up to the bound
/// are indexed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TooLarge {
    pub count: usize,
}

impl TooLarge {
    pub fn encode(&self) -> String {
        self.count.to_string()
    }

    pub fn decode(value: &str) -> Option<Self> {
        Some(Self {
            count: value.parse().ok()?,
        })
    }
}

/// One index directory as the folder index sees it.
#[derive(Debug, Clone)]
pub struct IndexEntry {
    pub dir: PathBuf,
    /// `None` for a directory whose store is missing or unreadable.
    pub root: Option<PathBuf>,
    /// Milliseconds since the epoch; 0 when never recorded.
    pub last_opened: i64,
    /// Its last walk found too many files, so it holds nothing to reuse.
    pub too_large: bool,
}

/// Every folder index directory under `base`, read without opening any.
pub async fn list_indexes(base: &Path) -> Vec<IndexEntry> {
    registry::list(base)
        .await
        .into_iter()
        .map(|entry| IndexEntry {
            root: entry.meta.get(META_ROOT).map(PathBuf::from),
            last_opened: entry
                .meta
                .get(META_LAST_OPENED)
                .and_then(|value| value.parse().ok())
                .unwrap_or(0),
            too_large: entry.meta.contains_key(META_TOO_LARGE),
            dir: entry.dir,
        })
        .collect()
}

/// What one index directory holds under a prefix.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Snapshot {
    pub counts: Counts,
    pub too_large: Option<TooLarge>,
    /// A full run has finished at least once.
    pub complete: bool,
}

/// Reads `dir`'s index under `prefix` without opening it for writing, so an
/// open folder's pool and run are untouched. `Ok(None)` when the directory
/// has no store.
pub async fn inspect(dir: &Path, prefix: &str) -> Result<Option<Snapshot>> {
    Ok(registry::inspect(dir, prefix)
        .await?
        .map(|inspected| Snapshot {
            counts: inspected.counts,
            too_large: inspected
                .meta
                .get(META_TOO_LARGE)
                .and_then(|value| TooLarge::decode(value)),
            complete: inspected.meta.contains_key(META_COMPLETE),
        }))
}

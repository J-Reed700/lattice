//! Finding and reading files: the walk that lists a folder's candidates,
//! and what reading one changed file decides its rows become.

use super::StatusCell;
use crate::features::explorer::fs;
use crate::features::explorer::index::chunker::{self, Chunk};
use crate::features::explorer::scope::Scope;
use crate::shared::Result;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;
use tokio_util::sync::CancellationToken;

/// How far past the cap a too-large walk keeps counting, so the message can
/// say how large rather than only "too large".
const COUNT_CEILING_FACTOR: usize = 10;
/// The walk reports how many files it has found once per this many.
const SCAN_REPORT_EVERY: usize = 200;

/// A file the walk found, before it is read.
#[derive(Debug, Clone)]
pub(super) struct Candidate {
    pub(super) relative: String,
    pub(super) absolute: PathBuf,
    pub(super) size: i64,
    pub(super) mtime: i64,
}

pub(super) enum Walked {
    Files(Vec<Candidate>),
    TooLarge { count: usize, capped: bool },
    Cancelled,
}

/// What reading one changed file decided.
pub(super) enum Prepared {
    /// Same bytes as before; only the size or time moved.
    Touch,
    Replace {
        hash: String,
        chunks: Vec<Chunk>,
    },
    /// Gone, unreadable, or over the size cap since the walk.
    Gone,
}

fn mtime_of(metadata: &std::fs::Metadata) -> i64 {
    metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |since| {
            i64::try_from(since.as_nanos()).unwrap_or(i64::MAX)
        })
}

/// Whether the walk should consider a file at all: decided by name and size
/// alone, so counting a too-large folder reads nothing.
pub(super) fn candidate(scope: &Scope, path: &Path) -> Option<Candidate> {
    let relative = scope.relative_of(path)?;
    if chunker::skipped_by_name(&relative) {
        return None;
    }
    let metadata = std::fs::metadata(path).ok()?;
    if !metadata.is_file() || metadata.len() > chunker::MAX_FILE_BYTES {
        return None;
    }
    Some(Candidate {
        relative,
        absolute: path.to_path_buf(),
        size: i64::try_from(metadata.len()).unwrap_or(i64::MAX),
        mtime: mtime_of(&metadata),
    })
}

/// Every indexable file under `prefix`, the way `search_files` walks:
/// `.gitignore` honoured, `.git` skipped, symlinks not followed. Stops early
/// when `cancel` fires, so closing a folder does not wait out a large walk.
/// With `progress`, the count found so far shows as the status's total once
/// it passes what the total already says, so a rescan does not count down
/// from the last run's number to zero and back.
pub(super) fn walk(
    scope: &Scope,
    prefix: &str,
    max_files: usize,
    cancel: &CancellationToken,
    progress: Option<&StatusCell>,
) -> Result<Walked> {
    let start = fs::walk_start(scope, Some(prefix))?;
    let ceiling = max_files.saturating_mul(COUNT_CEILING_FACTOR);
    let mut files = Vec::new();
    let mut count = 0usize;
    let mut capped = false;
    let mut cancelled = false;
    fs::walk_files(scope, start.as_ref(), |path| {
        if cancel.is_cancelled() {
            cancelled = true;
            return false;
        }
        let Some(found) = candidate(scope, path) else {
            return true;
        };
        count += 1;
        if let Some(status) = progress.filter(|_| count.is_multiple_of(SCAN_REPORT_EVERY)) {
            let found = u32::try_from(count).unwrap_or(u32::MAX);
            status.update(|status| status.files_total = status.files_total.max(found));
        }
        if count > ceiling {
            capped = true;
            return false;
        }
        if count <= max_files {
            files.push(found);
        }
        true
    });
    Ok(if cancelled {
        Walked::Cancelled
    } else if count > max_files {
        Walked::TooLarge {
            count: count.min(ceiling),
            capped,
        }
    } else {
        Walked::Files(files)
    })
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// Reads one changed file and decides what its rows become. Binary and
/// minified files are recorded with no chunks, so the next run skips them on
/// size and time without reading them again.
pub(super) fn prepare(path: &Path, previous_hash: Option<&str>) -> Prepared {
    let Ok((bytes, cut)) = fs::read_prefix(path, chunker::MAX_FILE_BYTES) else {
        return Prepared::Gone;
    };
    if cut {
        return Prepared::Gone;
    }
    let hash = sha256_hex(&bytes);
    if previous_hash == Some(hash.as_str()) {
        return Prepared::Touch;
    }
    if fs::looks_binary(&bytes) {
        return Prepared::Replace {
            hash,
            chunks: Vec::new(),
        };
    }
    let text = String::from_utf8_lossy(&bytes);
    let chunks = if chunker::looks_minified(&text) {
        Vec::new()
    } else {
        chunker::chunk_text(&text)
    };
    Prepared::Replace { hash, chunks }
}

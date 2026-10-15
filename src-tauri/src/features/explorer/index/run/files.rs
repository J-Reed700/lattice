//! Finding and reading files: the walk that lists a folder's candidates,
//! and what reading one changed file decides its rows become.

use super::StatusCell;
use crate::features::explorer::fs;
use crate::features::explorer::index::chunker::{self, Chunk};
use crate::features::explorer::scope::Scope;
use lattice_core::shared::Result;
use sha2::{Digest, Sha256};
use std::collections::{BinaryHeap, HashSet};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;
use tokio_util::sync::CancellationToken;

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
    /// The files to index, the first the walk found up to the bound, and how
    /// many more it found past it.
    Files {
        files: Vec<Candidate>,
        skipped: usize,
    },
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
    /// Gone or unreadable since the walk.
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

/// Whether the walk should consider a file at all: decided by name alone, so
/// counting a too-large folder reads nothing. Size decides nothing: a large
/// text file is indexed like any other, in passages.
pub(super) fn candidate(scope: &Scope, path: &Path) -> Option<Candidate> {
    let relative = scope.relative_of(path)?;
    if chunker::skipped_by_name(&relative) {
        return None;
    }
    let metadata = std::fs::metadata(path).ok()?;
    if !metadata.is_file() {
        return None;
    }
    Some(Candidate {
        relative,
        absolute: path.to_path_buf(),
        size: i64::try_from(metadata.len()).unwrap_or(i64::MAX),
        mtime: mtime_of(&metadata),
    })
}

/// A candidate ordered by its path, so the walk keeps the same files
/// whatever order the disk lists them in.
struct ByPath(Candidate);

impl PartialEq for ByPath {
    fn eq(&self, other: &Self) -> bool {
        self.0.relative == other.0.relative
    }
}

impl Eq for ByPath {}

impl PartialOrd for ByPath {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for ByPath {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0.relative.cmp(&other.0.relative)
    }
}

/// Every indexable file under `prefix`, the way `search_files` walks:
/// `.gitignore` honoured, `.git` skipped, symlinks not followed. Stops early
/// when `cancel` fires, so closing a folder does not wait out a large walk.
/// With `progress`, the count found so far shows as the status's total once
/// it passes what the total already says, so a rescan does not count down
/// from the last run's number to zero and back.
///
/// Past `max_files`, which files are kept does not depend on the order the
/// disk lists them: files already `indexed` come first, then the rest by
/// path. Kept by first-found, a folder past the bound indexed a different
/// set each run, re-embedding what it had just dropped.
pub(super) fn walk(
    scope: &Scope,
    prefix: &str,
    max_files: usize,
    indexed: &HashSet<String>,
    cancel: &CancellationToken,
    progress: Option<&StatusCell>,
) -> Result<Walked> {
    let start = fs::walk_start(scope, Some(prefix))?;
    let mut kept = Vec::new();
    // The first `max_files` new files by path: a max-heap, so the last by
    // path is the one a better file pushes out.
    let mut others: BinaryHeap<ByPath> = BinaryHeap::new();
    let mut count = 0usize;
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
        if indexed.contains(&found.relative) {
            kept.push(ByPath(found));
        } else {
            others.push(ByPath(found));
            if others.len() > max_files {
                others.pop();
            }
        }
        true
    });
    if cancelled {
        return Ok(Walked::Cancelled);
    }
    kept.sort();
    kept.truncate(max_files);
    let room = max_files - kept.len();
    let files: Vec<Candidate> = kept
        .into_iter()
        .chain(others.into_sorted_vec().into_iter().take(room))
        .map(|ByPath(found)| found)
        .collect();
    Ok(Walked::Files {
        skipped: count - files.len(),
        files,
    })
}

/// The SHA-256 of everything `reader` yields, read in fixed-size blocks.
fn stream_sha256(mut reader: impl std::io::Read) -> std::io::Result<String> {
    let mut hasher = Sha256::new();
    let mut block = vec![0u8; fs::LINE_PIECE_BYTES];
    loop {
        let read = reader.read(&mut block)?;
        if read == 0 {
            return Ok(format!("{:x}", hasher.finalize()));
        }
        hasher.update(block.get(..read).unwrap_or_default());
    }
}

/// Reads one changed file and decides what its rows become, in constant
/// memory whatever its size. Binary files are known from their first bytes
/// and recorded with no chunks, identified by those bytes and the length, so
/// a 6 GB video is neither loaded nor hashed whole; the next run skips them
/// on size and time. Text is hashed as it streams, and only a changed file
/// is read again to chunk, line by line with long lines in pieces. Minified
/// files are recorded with no chunks.
pub(super) fn prepare(path: &Path, previous_hash: Option<&str>) -> Prepared {
    prepare_streaming(path, previous_hash).unwrap_or(Prepared::Gone)
}

fn prepare_streaming(path: &Path, previous_hash: Option<&str>) -> std::io::Result<Prepared> {
    use std::io::BufRead;
    let open = || -> std::io::Result<_> {
        Ok(std::io::BufReader::with_capacity(
            fs::LINE_PIECE_BYTES,
            std::fs::File::open(path)?,
        ))
    };
    let mut reader = open()?;
    let head = reader.fill_buf()?;
    if fs::looks_binary(head) {
        let length = std::fs::metadata(path)?.len();
        let hash = format!("binary:{length}:{:x}", Sha256::digest(head));
        return Ok(if previous_hash == Some(hash.as_str()) {
            Prepared::Touch
        } else {
            Prepared::Replace {
                hash,
                chunks: Vec::new(),
            }
        });
    }
    let hash = stream_sha256(reader)?;
    if previous_hash == Some(hash.as_str()) {
        return Ok(Prepared::Touch);
    }

    let mut reader = open()?;
    let mut chunker = chunker::LineChunker::default();
    let mut piece = Vec::new();
    let (mut number, mut lines, mut bytes) = (0u32, 0usize, 0usize);
    let mut continuing = false;
    loop {
        let ends_line = fs::read_line_piece(&mut reader, &mut piece)?;
        if piece.is_empty() {
            break;
        }
        if !continuing {
            number = number.saturating_add(1);
            lines += 1;
        }
        continuing = !ends_line;
        bytes += piece.len();
        let text = String::from_utf8_lossy(&piece);
        chunker.push(number, text.trim_end_matches(['\n', '\r']).to_string());
    }
    let chunks = if chunker::is_minified(bytes, lines) {
        Vec::new()
    } else {
        chunker.finish()
    };
    Ok(Prepared::Replace { hash, chunks })
}

#[cfg(test)]
mod tests;

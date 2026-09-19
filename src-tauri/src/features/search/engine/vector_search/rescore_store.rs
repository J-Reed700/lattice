//! Full-precision side store backing exact rescoring of compressed candidates.
//!
//! When [`VectorIndexCompression`](super::compression::VectorIndexCompression)
//! is active the USearch index no longer holds enough information to produce a
//! true cosine score: the vectors in it are truncated, and usually `i8`. The
//! index becomes a candidate generator and something else has to hold the
//! full-precision vectors used to rank the over-fetched candidates.
//!
//! # Why not read them back out of SQLite
//!
//! The f32 vectors *are* already persisted — `text_embeddings.embedding` and
//! `embedding_generation_vectors.embedding` both hold them, and the startup
//! rebuild in `features::search::di` streams them straight back into the index.
//! But `VectorSearchPort::search` is synchronous while `sqlx` is not, so the
//! query path cannot reach those rows without blocking a runtime thread inside
//! a lock. This store is instead written by the same `insert_internal` call
//! that writes USearch, so the two cannot drift apart, and read at query time
//! with a positional read — `pread` on Unix, `ReadFile` with an explicit
//! offset on Windows. A positional read carries its own offset instead of
//! moving the file cursor, so it needs no exclusive access to the handle and
//! concurrent searches never queue behind one another.
//!
//! # Why not keep them in memory
//!
//! Because that would defeat the point. Compression exists to shrink resident
//! vector storage; holding a 4 KB `f32` copy of every 1024-d vector in a
//! `HashMap` would make a compressed index cost *more* RAM than an
//! uncompressed one. Only the over-fetched candidates — `top_k * factor`
//! vectors per query — are ever materialized.
//!
//! # Layout
//!
//! A flat array of fixed-width slots: slot `key - 1` starts at
//! `(key - 1) * dimension * 4`. USearch keys are allocated monotonically from
//! 1 and never reused, so no offset table is needed and none has to be kept in
//! sync. Removing a vector zeroes its slot; an all-zero slot reads back as
//! absent, as does a slot past the end of a truncated or missing file. Every
//! absent read degrades to the approximate USearch score rather than failing
//! the query.
//!
//! Slots of removed vectors are not reclaimed — the file is sparse and a
//! rebuild (`clear`) truncates it back to nothing, which is the same bargain
//! the USearch index file itself makes.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use parking_lot::{Mutex, RwLock};

use crate::shared::error::AppError;
use crate::shared::result::Result;

/// Width of one `f32` on disk. Matches `features::embedding::encoding`.
const F32_BYTES: usize = 4;

/// The lazily opened slot file.
///
/// `Arc` so a reader can take a clone under a read lock and then do its I/O
/// with no lock held at all; `RwLock` so the only writer is the one call that
/// opens the file, and every call after that is an uncontended read.
type FileHandle = RwLock<Option<Arc<std::fs::File>>>;

/// Compute the side store path from the index path
/// (`foo.usearch` → `foo.usearch.vectors`).
pub fn vectors_path_for(index_path: &Path) -> PathBuf {
    let mut p = index_path.as_os_str().to_owned();
    p.push(".vectors");
    PathBuf::from(p)
}

enum Backing {
    /// In-memory index (no `index_path`), as used by unit tests and the
    /// transient indexes some feature tests build.
    Memory(Mutex<HashMap<u64, Vec<f32>>>),
    /// Slot file next to the USearch index.
    File { path: PathBuf, handle: FileHandle },
}

/// Full-precision vectors keyed by USearch key.
pub struct RescoreVectorStore {
    dimension: usize,
    backing: Backing,
}

impl RescoreVectorStore {
    /// Create a store for `dimension`-length vectors, file-backed when the
    /// index itself is persisted and memory-backed otherwise.
    pub fn new(dimension: usize, index_path: Option<&Path>) -> Self {
        let backing = match index_path {
            Some(path) => Backing::File {
                path: vectors_path_for(path),
                handle: RwLock::new(None),
            },
            None => Backing::Memory(Mutex::new(HashMap::new())),
        };
        Self { dimension, backing }
    }

    /// Store the full-precision vector for `key`.
    pub fn put(&self, key: u64, vector: &[f32]) -> Result<()> {
        if vector.len() != self.dimension {
            return Err(AppError::InvalidInput(format!(
                "Rescore store expects {}-dimension vectors, got {}",
                self.dimension,
                vector.len()
            )));
        }
        match &self.backing {
            Backing::Memory(map) => {
                map.lock().insert(key, vector.to_vec());
                Ok(())
            }
            Backing::File { path, handle } => {
                let mut bytes = Vec::with_capacity(vector.len() * F32_BYTES);
                for value in vector {
                    bytes.extend_from_slice(&value.to_le_bytes());
                }
                self.write_slot(path, handle, key, &bytes)
            }
        }
    }

    /// Fetch the full-precision vector for `key`, or `None` when it was never
    /// written, was removed, or the backing file cannot serve it.
    ///
    /// Deliberately infallible: a missing vector costs the query exact scoring
    /// for one candidate, not the query itself.
    pub fn get(&self, key: u64) -> Option<Vec<f32>> {
        match &self.backing {
            Backing::Memory(map) => map.lock().get(&key).cloned(),
            Backing::File { path, handle } => {
                let mut buffer = vec![0u8; self.dimension * F32_BYTES];
                self.read_slot(path, handle, key, &mut buffer).ok()?;
                let vector: Vec<f32> = buffer
                    .chunks_exact(F32_BYTES)
                    .map(|chunk| {
                        let mut raw = [0u8; F32_BYTES];
                        raw.copy_from_slice(chunk);
                        f32::from_le_bytes(raw)
                    })
                    .collect();
                // An unwritten slot in a sparse file reads as zeros, which is
                // not a vector any embedding model produces.
                if vector.iter().all(|v| *v == 0.0) || vector.iter().any(|v| !v.is_finite()) {
                    return None;
                }
                Some(vector)
            }
        }
    }

    /// Drop the vector for `key`, so a stale slot can never be rescored back
    /// into a result set after its chunk was deleted.
    pub fn remove(&self, key: u64) -> Result<()> {
        match &self.backing {
            Backing::Memory(map) => {
                map.lock().remove(&key);
                Ok(())
            }
            Backing::File { path, handle } => {
                let zeros = vec![0u8; self.dimension * F32_BYTES];
                // Nothing to clear if the file never reached this slot.
                if self.slot_offset(key)? >= self.file_len(path, handle)? {
                    return Ok(());
                }
                self.write_slot(path, handle, key, &zeros)
            }
        }
    }

    /// Discard every vector. Paired with the index reset in `clear`/rebuild,
    /// where USearch keys restart from 1 and slots would otherwise be reused
    /// with stale contents.
    pub fn clear(&self) -> Result<()> {
        match &self.backing {
            Backing::Memory(map) => {
                map.lock().clear();
                Ok(())
            }
            Backing::File { path, handle } => match Self::handle(path, handle) {
                Ok(file) => file.set_len(0).map_err(|e| {
                    AppError::FileStorage(format!(
                        "truncate rescore vector store {}: {}",
                        path.display(),
                        e
                    ))
                }),
                // A store that was never created is already empty.
                Err(_) if !path.exists() => Ok(()),
                Err(e) => Err(e),
            },
        }
    }

    fn slot_offset(&self, key: u64) -> Result<u64> {
        let slot = key.checked_sub(1).ok_or_else(|| {
            AppError::InvalidInput("Rescore store keys start at 1, got 0".to_string())
        })?;
        slot.checked_mul((self.dimension * F32_BYTES) as u64)
            .ok_or_else(|| {
                AppError::InvalidInput(format!("Rescore store offset overflow for key {}", key))
            })
    }

    /// The slot file, opened — and created — on first use.
    ///
    /// The steady state is a read lock plus an `Arc` clone, so the I/O itself
    /// happens with no lock held. Opening still has to be lazy: the store is
    /// constructed alongside every index, including ones that never get a
    /// vector written to them, and creating the file eagerly would litter the
    /// data directory.
    fn handle(path: &Path, handle: &FileHandle) -> Result<Arc<std::fs::File>> {
        if let Some(file) = handle.read().as_ref() {
            return Ok(Arc::clone(file));
        }
        let mut guard = handle.write();
        // Another thread may have opened it while this one queued for the
        // write lock; re-opening would leave two handles for the same path.
        if let Some(file) = guard.as_ref() {
            return Ok(Arc::clone(file));
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                AppError::FileStorage(format!(
                    "create rescore vector store directory {}: {}",
                    parent.display(),
                    e
                ))
            })?;
        }
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)
            .map_err(|e| {
                AppError::FileStorage(format!(
                    "open rescore vector store {}: {}",
                    path.display(),
                    e
                ))
            })?;
        let file = Arc::new(file);
        *guard = Some(Arc::clone(&file));
        Ok(file)
    }

    fn write_slot(&self, path: &Path, handle: &FileHandle, key: u64, bytes: &[u8]) -> Result<()> {
        let offset = self.slot_offset(key)?;
        let file = Self::handle(path, handle)?;
        write_all_at(&file, bytes, offset).map_err(|e| {
            AppError::FileStorage(format!(
                "write rescore vector store {}: {}",
                path.display(),
                e
            ))
        })
    }

    fn read_slot(
        &self,
        path: &Path,
        handle: &FileHandle,
        key: u64,
        buffer: &mut [u8],
    ) -> Result<()> {
        let offset = self.slot_offset(key)?;
        let file = Self::handle(path, handle)?;
        read_exact_at(&file, buffer, offset)
            .map_err(|e| AppError::FileStorage(format!("read rescore vector store: {}", e)))
    }

    fn file_len(&self, path: &Path, handle: &FileHandle) -> Result<u64> {
        let file = Self::handle(path, handle)?;
        let metadata = file
            .metadata()
            .map_err(|e| AppError::FileStorage(format!("stat rescore vector store: {}", e)))?;
        Ok(metadata.len())
    }
}

/// One positional read. `pread`-style on both platforms: the offset travels
/// with the call, so nothing here touches the shared file cursor and no two
/// readers can steal each other's position.
#[cfg(unix)]
fn read_once_at(file: &std::fs::File, buffer: &mut [u8], offset: u64) -> std::io::Result<usize> {
    use std::os::unix::fs::FileExt;
    file.read_at(buffer, offset)
}

/// Windows has no `pread`; `seek_read` is `ReadFile` with an explicit
/// `OVERLAPPED` offset, which is equally safe to issue concurrently on a
/// shared handle. It does move the file pointer as a side effect, which is
/// harmless because nothing in this module reads or writes through it.
#[cfg(windows)]
fn read_once_at(file: &std::fs::File, buffer: &mut [u8], offset: u64) -> std::io::Result<usize> {
    use std::os::windows::fs::FileExt;
    file.seek_read(buffer, offset)
}

/// One positional write, the mirror of [`read_once_at`].
#[cfg(unix)]
fn write_once_at(file: &std::fs::File, bytes: &[u8], offset: u64) -> std::io::Result<usize> {
    use std::os::unix::fs::FileExt;
    file.write_at(bytes, offset)
}

#[cfg(windows)]
fn write_once_at(file: &std::fs::File, bytes: &[u8], offset: u64) -> std::io::Result<usize> {
    use std::os::windows::fs::FileExt;
    file.seek_write(bytes, offset)
}

/// Fill `buffer` from `offset`, or fail.
///
/// Both positional primitives are allowed to come back short, so this loops
/// the way [`std::io::Read::read_exact`] does. A slot that runs off the end of
/// a truncated file must be an error rather than a partly filled buffer:
/// `get` would otherwise hand back half a vector padded with zeros and score a
/// candidate against it.
fn read_exact_at(file: &std::fs::File, buffer: &mut [u8], offset: u64) -> std::io::Result<()> {
    let mut remaining = buffer;
    let mut offset = offset;
    while !remaining.is_empty() {
        match read_once_at(file, remaining, offset) {
            Ok(0) => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::UnexpectedEof,
                    "rescore slot ends past the end of the file",
                ))
            }
            Ok(read) => {
                let rest = std::mem::take(&mut remaining);
                // Clamping keeps a nonsensical return from the OS from
                // running the split off the end of the buffer.
                let read = read.min(rest.len());
                let (_, tail) = rest.split_at_mut(read);
                remaining = tail;
                offset = offset.saturating_add(read as u64);
            }
            // A signal arriving mid-read is not a failure to read.
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
    Ok(())
}

/// Write all of `bytes` at `offset`, looping on short writes the way
/// [`std::io::Write::write_all`] does. A short write here would leave a slot
/// holding the head of the new vector and the tail of whatever preceded it.
fn write_all_at(file: &std::fs::File, bytes: &[u8], offset: u64) -> std::io::Result<()> {
    let mut remaining = bytes;
    let mut offset = offset;
    while !remaining.is_empty() {
        match write_once_at(file, remaining, offset) {
            Ok(0) => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::WriteZero,
                    "rescore vector store accepted no bytes",
                ))
            }
            Ok(written) => {
                let written = written.min(remaining.len());
                let (_, tail) = remaining.split_at(written);
                remaining = tail;
                offset = offset.saturating_add(written as u64);
            }
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
    Ok(())
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]
mod tests {
    use super::*;

    fn vector(seed: f32, dim: usize) -> Vec<f32> {
        (0..dim).map(|i| seed + i as f32 * 0.25).collect()
    }

    #[test]
    fn path_appends_vectors_suffix() {
        assert_eq!(
            vectors_path_for(Path::new("/tmp/foo.usearch")),
            PathBuf::from("/tmp/foo.usearch.vectors")
        );
    }

    #[test]
    fn memory_backing_round_trips_and_forgets() {
        let store = RescoreVectorStore::new(4, None);
        store.put(1, &vector(1.0, 4)).unwrap();
        store.put(7, &vector(2.0, 4)).unwrap();

        assert_eq!(store.get(1), Some(vector(1.0, 4)));
        assert_eq!(store.get(7), Some(vector(2.0, 4)));
        assert_eq!(store.get(2), None, "unwritten key must read as absent");

        store.remove(7).unwrap();
        assert_eq!(store.get(7), None);

        store.clear().unwrap();
        assert_eq!(store.get(1), None);
    }

    #[test]
    fn file_backing_round_trips_across_sparse_slots() {
        let dir = tempfile::tempdir().unwrap();
        let index = dir.path().join("sub").join("index.usearch");
        let store = RescoreVectorStore::new(8, Some(&index));

        // Deliberately skip keys so the file is sparse.
        store.put(1, &vector(0.5, 8)).unwrap();
        store.put(64, &vector(-3.0, 8)).unwrap();

        assert_eq!(store.get(1), Some(vector(0.5, 8)));
        assert_eq!(store.get(64), Some(vector(-3.0, 8)));
        assert_eq!(store.get(32), None, "hole in a sparse file reads as absent");
        assert_eq!(store.get(65), None, "past end of file reads as absent");

        assert!(vectors_path_for(&index).exists());
    }

    #[test]
    fn removal_and_clear_stop_serving_vectors() {
        let dir = tempfile::tempdir().unwrap();
        let index = dir.path().join("index.usearch");
        let store = RescoreVectorStore::new(4, Some(&index));

        store.put(1, &vector(1.0, 4)).unwrap();
        store.put(2, &vector(2.0, 4)).unwrap();
        store.remove(2).unwrap();
        assert_eq!(store.get(2), None);
        assert_eq!(store.get(1), Some(vector(1.0, 4)));

        // Removing a key the file never reached must not extend the file.
        let before = std::fs::metadata(vectors_path_for(&index)).unwrap().len();
        store.remove(4096).unwrap();
        let after = std::fs::metadata(vectors_path_for(&index)).unwrap().len();
        assert_eq!(before, after);

        store.clear().unwrap();
        assert_eq!(store.get(1), None);
        assert_eq!(
            std::fs::metadata(vectors_path_for(&index)).unwrap().len(),
            0
        );
    }

    #[test]
    fn a_new_handle_sees_what_a_previous_one_wrote() {
        let dir = tempfile::tempdir().unwrap();
        let index = dir.path().join("index.usearch");
        {
            let store = RescoreVectorStore::new(4, Some(&index));
            store.put(3, &vector(9.0, 4)).unwrap();
        }
        let reopened = RescoreVectorStore::new(4, Some(&index));
        assert_eq!(reopened.get(3), Some(vector(9.0, 4)));
    }

    #[test]
    fn many_threads_read_the_same_store_without_crossing_slots() {
        let dir = tempfile::tempdir().unwrap();
        let index = dir.path().join("index.usearch");
        let store = Arc::new(RescoreVectorStore::new(16, Some(&index)));
        for key in 1..=32u64 {
            store.put(key, &vector(key as f32, 16)).unwrap();
        }

        // The point of the positional read: nothing below serializes, so a
        // seek-then-read implementation would let one reader move another
        // reader's cursor and return a neighbouring slot.
        let readers: Vec<_> = (0..8)
            .map(|_| {
                let store = Arc::clone(&store);
                std::thread::spawn(move || {
                    for _ in 0..50 {
                        for key in 1..=32u64 {
                            assert_eq!(store.get(key), Some(vector(key as f32, 16)));
                        }
                    }
                })
            })
            .collect();
        for reader in readers {
            reader.join().unwrap();
        }
    }

    #[test]
    fn a_slot_cut_in_half_by_truncation_reads_as_absent() {
        let dir = tempfile::tempdir().unwrap();
        let index = dir.path().join("index.usearch");
        {
            let store = RescoreVectorStore::new(8, Some(&index));
            store.put(1, &vector(2.0, 8)).unwrap();
        }

        // Half a slot is what an interrupted copy or a torn file leaves
        // behind, and it is also what a short read looks like from the
        // inside: the first read returns some bytes, the next returns none.
        let path = vectors_path_for(&index);
        let half = (8 * F32_BYTES / 2) as u64;
        std::fs::OpenOptions::new()
            .write(true)
            .open(&path)
            .unwrap()
            .set_len(half)
            .unwrap();

        let store = RescoreVectorStore::new(8, Some(&index));
        assert_eq!(store.get(1), None, "a partial slot is not a vector");
        assert_eq!(std::fs::metadata(&path).unwrap().len(), half);
    }

    #[test]
    fn wrong_dimension_is_rejected() {
        let store = RescoreVectorStore::new(4, None);
        assert!(store.put(1, &[1.0, 2.0]).is_err());
    }

    #[test]
    fn key_zero_is_rejected_rather_than_wrapping() {
        let store = RescoreVectorStore::new(4, None);
        // Memory backing ignores offsets, so exercise the slot math directly.
        assert!(store.slot_offset(0).is_err());
        assert_eq!(store.slot_offset(1).unwrap(), 0);
        assert_eq!(store.slot_offset(3).unwrap(), 32);
    }
}

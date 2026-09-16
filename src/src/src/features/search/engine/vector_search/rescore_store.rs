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
//! that writes USearch, so the two cannot drift apart, and read with a plain
//! positioned read at query time.
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
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use parking_lot::Mutex;

use crate::shared::error::AppError;
use crate::shared::result::Result;

/// Width of one `f32` on disk. Matches `features::embedding::encoding`.
const F32_BYTES: usize = 4;

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
    File {
        path: PathBuf,
        handle: Mutex<Option<std::fs::File>>,
    },
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
                handle: Mutex::new(None),
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
            Backing::File { path, handle } => {
                let mut guard = handle.lock();
                match Self::ensure_open(path, &mut guard) {
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
                }
            }
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

    fn ensure_open<'a>(
        path: &Path,
        guard: &'a mut Option<std::fs::File>,
    ) -> Result<&'a mut std::fs::File> {
        if guard.is_none() {
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
            *guard = Some(file);
        }
        guard.as_mut().ok_or_else(|| {
            AppError::InternalError("Rescore vector store handle vanished".to_string())
        })
    }

    fn write_slot(
        &self,
        path: &Path,
        handle: &Mutex<Option<std::fs::File>>,
        key: u64,
        bytes: &[u8],
    ) -> Result<()> {
        let offset = self.slot_offset(key)?;
        let mut guard = handle.lock();
        let file = Self::ensure_open(path, &mut guard)?;
        file.seek(SeekFrom::Start(offset)).map_err(|e| {
            AppError::FileStorage(format!("seek rescore vector store to {}: {}", offset, e))
        })?;
        file.write_all(bytes).map_err(|e| {
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
        handle: &Mutex<Option<std::fs::File>>,
        key: u64,
        buffer: &mut [u8],
    ) -> Result<()> {
        let offset = self.slot_offset(key)?;
        let mut guard = handle.lock();
        let file = Self::ensure_open(path, &mut guard)?;
        file.seek(SeekFrom::Start(offset))
            .map_err(|e| AppError::FileStorage(format!("seek rescore vector store: {}", e)))?;
        file.read_exact(buffer)
            .map_err(|e| AppError::FileStorage(format!("read rescore vector store: {}", e)))
    }

    fn file_len(&self, path: &Path, handle: &Mutex<Option<std::fs::File>>) -> Result<u64> {
        let mut guard = handle.lock();
        let file = Self::ensure_open(path, &mut guard)?;
        let metadata = file
            .metadata()
            .map_err(|e| AppError::FileStorage(format!("stat rescore vector store: {}", e)))?;
        Ok(metadata.len())
    }
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

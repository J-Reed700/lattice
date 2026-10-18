//! USearch-based vector index implementation.
//!
//! Replaces the deprecated `instant-distance` HNSW and in-memory `FlatVectorIndex`
//! with USearch v2.x — a production-grade, SIMD-accelerated HNSW library with
//! built-in persistence and mmap support.
//!
//! # Architecture
//!
//! - USearch owns vectors + integer keys (u64)
//! - SQLite owns document/chunk metadata
//! - A bidirectional key map translates between string IDs and u64 keys
//! - The index persists to disk and can be loaded via mmap for instant startup
//!
//! # Thread Safety
//!
//! USearch's `Index` is `Send + Sync`. All mutable state (key maps, metadata)
//! is protected by a single `RwLock<KeyState>` to prevent desynchronization
//! and TOCTOU races.

use super::compression::{VectorIndexCompression, VectorQuantization};
use super::index_config::IndexConfig;
use super::rescore_store::RescoreVectorStore;
use lattice_core::shared::error::AppError;
use lattice_core::shared::error::Result;
use parking_lot::{Mutex, RwLock};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicU64;
#[cfg(test)]
use std::sync::atomic::AtomicUsize;
use std::time::Instant;
use usearch::{Index, IndexOptions, MetricKind, ScalarKind};

mod mutation;
mod persistence;
mod ports;
mod search;

/// Capacity reserved the first time a vector is inserted into an empty index.
///
/// Deliberately modest: `ensure_capacity` doubles from here, so a large corpus
/// reaches its working size in a handful of reservations, while a small vault
/// (or a unit test) doesn't pay for 10k slots it will never use.
const INITIAL_INDEX_CAPACITY: usize = 1_024;

/// HNSW graph parameters. Recorded in the manifest, so retuning any of them
/// rebuilds rather than reusing a graph built to a different shape.
///
/// Measured on the v3 retrieval fixture (1,145 Qwen3 vectors, 194 queries)
/// against exact cosine: 16/128/64 lost the true nearest chunk on 13 queries
/// (recall@10 0.953), because a templated corpus packs into tight clusters and
/// a narrow walk never leaves the one it lands in. 32/256/256 lost none
/// (recall@10 0.999) for about 0.3 ms more per query.
const HNSW_CONNECTIVITY: usize = 32;
const HNSW_EXPANSION_ADD: usize = 256;
const HNSW_EXPANSION_SEARCH: usize = 256;

/// Up to this many vectors a query is answered by an exact scan instead of the
/// graph. This removes graph traversal misses in the stored vector space;
/// compressed candidate selection can still differ from full-vector cosine.
/// The graph is still built and saved, so crossing the line needs no rebuild.
const EXACT_SEARCH_MAX_VECTORS: usize = 20_000;
/// Limit an approximate scoped walk even when the eligible scope is huge.
const MAX_SCOPED_CANDIDATES: usize = 8_192;

/// When to write the index to disk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SavePolicy {
    /// Write the whole index after every mutation. Correct, and fine for an
    /// index nobody bulk-loads, but indexing a thousand-document folder this
    /// way is a thousand full-corpus writes.
    Immediate,
    /// Mark the index dirty and let a flusher choose the moment — when a bulk
    /// run goes quiet, at a periodic checkpoint, and on shutdown. Safe because
    /// a launch that finds SQLite ahead of the index rebuilds from SQLite.
    Coalesced,
}

/// Metadata stored alongside each vector for search result enrichment.
///
/// Search identity and scope metadata only. Candidate text is fetched from
/// SQLite after ranking so corpus text is never retained in the vector index.
#[derive(Clone, Debug)]
struct VectorMeta {
    chunk_id: String,
    document_id: String,
}

/// Whether the in-memory index has changes the file on disk does not.
#[derive(Default)]
struct DirtyState {
    /// When the index first moved ahead of the file, so a long bulk run can
    /// still be checkpointed instead of never going quiet.
    dirty_since: Option<Instant>,
    /// When it last moved, so a run that has finished can be recognised.
    last_mutation: Option<Instant>,
}

impl DirtyState {
    fn mark(&mut self) {
        let now = Instant::now();
        self.dirty_since.get_or_insert(now);
        self.last_mutation = Some(now);
    }
}

/// How long the index has owed a save, and how long it has been quiet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DirtyFor {
    pub since_first_mutation: std::time::Duration,
    pub since_last_mutation: std::time::Duration,
}

/// Consolidated mutable state protected by a single `RwLock`.
///
/// Keeping both key maps and metadata under one lock prevents:
/// - Desynchronization between id_to_key and key_to_id
/// - TOCTOU races in add_internal (duplicate check → insert is atomic)
struct KeyState {
    /// Forward map: string ID → u64 key
    id_to_key: HashMap<String, u64>,
    /// Reverse map: u64 key → string ID
    key_to_id: HashMap<u64, String>,
    /// Metadata map: string ID → chunk metadata
    metadata: HashMap<String, VectorMeta>,
    /// Reverse scope index so a narrow document filter can visit just its keys.
    document_to_keys: HashMap<String, HashSet<u64>>,
}

/// USearch-backed vector index implementing both `VectorSearchPort` (application layer)
/// and `SearchServiceTrait` (infrastructure layer), unifying the two parallel pipelines.
pub struct USearchVectorIndex {
    /// The USearch HNSW index (thread-safe, persistent)
    index: Index,

    /// All mutable key/metadata state under a single lock (prevents desync and TOCTOU)
    state: RwLock<KeyState>,

    /// Next available key (monotonically increasing)
    next_key: AtomicU64,

    /// Expected embedding dimension, as produced by the embedding model.
    ///
    /// This is what callers hand in and what `dimension()` reports; the
    /// dimension USearch is configured with may be smaller when truncation is
    /// active.
    dimension: usize,

    /// How vectors are stored in USearch. `None` on every existing index.
    compression: VectorIndexCompression,

    /// Full-precision vectors used to rescore over-fetched candidates.
    /// `Some` exactly when `compression.is_active()`.
    rescore: Option<RescoreVectorStore>,

    /// Largest index a query scans exactly rather than walking the graph.
    /// Always [`EXACT_SEARCH_MAX_VECTORS`] outside tests, which lower it to
    /// reach the graph path without building twenty thousand vectors.
    exact_search_max: usize,

    #[cfg(test)]
    exact_scope_scored: AtomicUsize,

    /// Path for persisting the index to disk
    index_path: Option<PathBuf>,

    /// Path for persisting the key map to disk
    keymap_path: Option<PathBuf>,
    /// Serialize disk snapshots (including their shared temporary filename).
    persistence: Mutex<()>,
    /// When mutations reach the disk.
    save_policy: SavePolicy,
    /// Tracks whether a save is owed, and how long the index has been quiet.
    dirty: Mutex<DirtyState>,
}

/// Where a file is staged before it replaces its final name.
///
/// Deterministic rather than randomized, so a crash leaves at most one stale
/// temporary per file and the next open can clear it by name.
fn temp_path_for(path: &Path) -> PathBuf {
    let mut p = path.as_os_str().to_owned();
    p.push(".tmp");
    PathBuf::from(p)
}

/// Write `bytes` so that readers see either the previous file or the whole new
/// one, never a prefix of it.
pub(crate) fn write_file_atomically(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            AppError::FileStorage(format!(
                "Failed to create directory for {}: {e}",
                path.display()
            ))
        })?;
    }
    let temp = temp_path_for(path);
    std::fs::write(&temp, bytes)
        .map_err(|e| AppError::FileStorage(format!("Failed to write {}: {e}", temp.display())))?;
    std::fs::rename(&temp, path).map_err(|e| {
        let _ = std::fs::remove_file(&temp);
        AppError::FileStorage(format!("Failed to replace {}: {e}", path.display()))
    })
}

/// USearch scalar kind for a compression configuration.
fn scalar_kind_for(compression: &VectorIndexCompression) -> ScalarKind {
    match compression.quantization() {
        VectorQuantization::F32 => ScalarKind::F32,
        VectorQuantization::I8 => ScalarKind::I8,
    }
}

impl USearchVectorIndex {
    /// Create a new uncompressed USearch vector index.
    ///
    /// # Arguments
    /// * `dimension` - Expected embedding dimension (e.g., 768)
    /// * `index_path` - Optional path for index persistence
    pub fn new(dimension: usize, index_path: Option<PathBuf>) -> Result<Self> {
        Self::with_compression(dimension, index_path, VectorIndexCompression::None)
    }

    /// Create a new USearch vector index with an explicit storage configuration.
    ///
    /// `VectorIndexCompression::None` reproduces [`Self::new`] exactly. Any
    /// other configuration stores lossy vectors in USearch and keeps
    /// full-precision copies in a side store for exact rescoring.
    pub fn with_compression(
        dimension: usize,
        index_path: Option<PathBuf>,
        compression: VectorIndexCompression,
    ) -> Result<Self> {
        compression.validate(dimension)?;
        let options = IndexOptions {
            dimensions: compression.stored_dimension(dimension),
            metric: MetricKind::Cos,
            quantization: scalar_kind_for(&compression),
            connectivity: HNSW_CONNECTIVITY,
            expansion_add: HNSW_EXPANSION_ADD,
            expansion_search: HNSW_EXPANSION_SEARCH,
            multi: false,
        };

        let index = Index::new(&options).map_err(|e| {
            AppError::InternalError(format!("Failed to create USearch index: {}", e))
        })?;

        let keymap_path = index_path.as_ref().map(|p| {
            let mut keymap = p.clone();
            keymap.set_file_name(format!(
                "{}.keymap.json",
                p.file_stem()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "index".to_string())
            ));
            keymap
        });

        let rescore = compression
            .is_active()
            .then(|| RescoreVectorStore::new(dimension, index_path.as_deref()));

        Ok(Self {
            index,
            state: RwLock::new(KeyState {
                id_to_key: HashMap::new(),
                key_to_id: HashMap::new(),
                metadata: HashMap::new(),
                document_to_keys: HashMap::new(),
            }),
            next_key: AtomicU64::new(1),
            dimension,
            compression,
            rescore,
            exact_search_max: EXACT_SEARCH_MAX_VECTORS,
            #[cfg(test)]
            exact_scope_scored: AtomicUsize::new(0),
            index_path,
            keymap_path,
            persistence: Mutex::new(()),
            save_policy: SavePolicy::Immediate,
            dirty: Mutex::new(DirtyState::default()),
        })
    }

    /// Stop writing the whole index after every mutation; a flusher decides
    /// when instead. See [`SavePolicy`].
    #[must_use]
    pub fn with_coalesced_saves(mut self) -> Self {
        self.save_policy = SavePolicy::Coalesced;
        self
    }

    /// The storage configuration this index was opened with.
    pub fn compression(&self) -> &VectorIndexCompression {
        &self.compression
    }

    /// The graph and vector layout, for the manifest that decides whether this
    /// index can be reused on the next launch.
    pub fn index_config(&self) -> IndexConfig {
        IndexConfig::new(
            self.dimension,
            &self.compression,
            HNSW_CONNECTIVITY,
            HNSW_EXPANSION_ADD,
            HNSW_EXPANSION_SEARCH,
        )
    }

    /// The index holds changes the file on disk does not.
    pub fn is_dirty(&self) -> bool {
        self.dirty.lock().dirty_since.is_some()
    }

    /// How overdue a save is, or `None` when nothing is owed.
    pub fn dirty_for(&self) -> Option<DirtyFor> {
        let state = self.dirty.lock();
        let since_first_mutation = state.dirty_since?.elapsed();
        Some(DirtyFor {
            since_first_mutation,
            since_last_mutation: state
                .last_mutation
                .map_or(since_first_mutation, |at| at.elapsed()),
        })
    }

    /// Record that the in-memory index moved ahead of the file, and persist it
    /// now if that is this index's policy.
    fn note_mutation(&self) -> Result<()> {
        self.dirty.lock().mark();
        match self.save_policy {
            SavePolicy::Immediate => self.save_to_disk(),
            SavePolicy::Coalesced => Ok(()),
        }
    }
}

/// Flush the USearch index to disk on drop.
///
/// Best-effort only. Tauri exits the process from inside its event loop, so in
/// the real app nothing that `AppHandle` owns is ever dropped; the shutdown
/// flush in the search feature's `IndexPersistence` is what actually saves.
/// This still covers tests and any index that is genuinely dropped.
impl Drop for USearchVectorIndex {
    fn drop(&mut self) {
        if self.index_path.is_some() && self.is_dirty() {
            if let Err(e) = self.save_to_disk() {
                // Cannot propagate errors from Drop, so log at error level
                tracing::error!(error = %e, "Failed to flush USearch index to disk during drop");
            } else {
                tracing::debug!("USearch index flushed to disk during drop");
            }
        }
    }
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]
mod tests;

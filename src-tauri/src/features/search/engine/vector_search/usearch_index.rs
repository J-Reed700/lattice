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
use super::manifest::IndexConfig;
use super::rescore_store::RescoreVectorStore;
use crate::application::ports::vector_search_port::VectorIndexEntry;
use crate::application::ports::VectorSearchPort;
use crate::features::search::dto::SearchResultPortDto;
use crate::features::search::engine::service::SearchResult;
use crate::features::search::engine::vector_ops::cosine_similarity_simd;
use crate::features::search::SearchServiceTrait;
use crate::shared::error::AppError;
use crate::shared::result::Result;
use async_trait::async_trait;
use parking_lot::{Mutex, RwLock};
use std::cmp::Ordering as CmpOrdering;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;
use usearch::{Index, IndexOptions, MetricKind, ScalarKind};

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
/// graph. A personal library is usually this small, the scan costs under a
/// microsecond per vector, and it cannot miss; the graph is still built and
/// saved, so crossing the line needs no rebuild.
const EXACT_SEARCH_MAX_VECTORS: usize = 20_000;

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
/// `content` is the one field that is *not* persisted with the key map: it is
/// a second copy of `text_chunks.content`, and writing every chunk's text back
/// to disk on every save is what made saving cost the whole corpus. It is
/// filled in by whoever supplies the vector (indexing, or the startup rebuild)
/// and re-filled from SQLite by `hydrate_content` when an index is loaded
/// without a rebuild.
#[derive(Clone, Debug)]
struct VectorMeta {
    chunk_id: String,
    document_id: String,
    content: String,
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

/// Serializable key map for persistence alongside the USearch index file.
///
/// Deliberately still JSON. Once the chunk text is gone an entry is a handful
/// of short strings, so parsing it is no longer what startup waits on — the
/// content hydration query is — and a format anyone can open in a text editor
/// is worth more than the milliseconds a binary encoding would save.
#[derive(serde::Serialize, serde::Deserialize)]
struct KeyMapData {
    id_to_key: HashMap<String, u64>,
    next_key: u64,
    metadata: HashMap<String, SerializableMeta>,
}

/// The persisted half of [`VectorMeta`]. Chunk text is deliberately absent:
/// SQLite already holds it.
#[derive(serde::Serialize, serde::Deserialize)]
struct SerializableMeta {
    chunk_id: String,
    document_id: String,
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
pub(super) fn write_file_atomically(path: &Path, bytes: &[u8]) -> Result<()> {
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
            }),
            next_key: AtomicU64::new(1),
            dimension,
            compression,
            rescore,
            exact_search_max: EXACT_SEARCH_MAX_VECTORS,
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

    /// The `count` stored vectors nearest to `query`, nearest first, as
    /// parallel key and distance lists.
    ///
    /// A small index is scanned exactly; see [`EXACT_SEARCH_MAX_VECTORS`]. The
    /// scan has no predicate, so a scoped query ranks every vector and keeps the
    /// in-scope ones. Past that size the graph answers, and the scope runs as a
    /// predicate inside the traversal: the filter costs a hash lookup per
    /// visited node instead of a discarded candidate per hit. The predicate
    /// sees only the USearch key, and it must not panic, because USearch calls
    /// it back through a plain `extern "C"` trampoline that unwinding would
    /// cross.
    fn nearest(
        &self,
        state: &KeyState,
        query: &[f32],
        count: usize,
        scope: Option<&HashSet<String>>,
    ) -> Result<(Vec<u64>, Vec<f32>)> {
        let in_scope = |key: u64, scope: &HashSet<String>| {
            state
                .key_to_id
                .get(&key)
                .map(|id| {
                    let doc_id = state
                        .metadata
                        .get(id)
                        .map_or(id.as_str(), |m| m.document_id.as_str());
                    scope.contains(doc_id)
                })
                .unwrap_or(false)
        };
        let failed = |e| AppError::InternalError(format!("USearch search failed: {}", e));

        let index_size = self.index.size();
        if index_size <= self.exact_search_max {
            let Some(scope) = scope else {
                let matches = self.index.exact_search(query, count).map_err(failed)?;
                return Ok((matches.keys, matches.distances));
            };
            let matches = self.index.exact_search(query, index_size).map_err(failed)?;
            return Ok(matches
                .keys
                .into_iter()
                .zip(matches.distances)
                .filter(|(key, _)| in_scope(*key, scope))
                .take(count)
                .unzip());
        }

        let matches = match scope {
            None => self.index.search(query, count),
            Some(scope) => self
                .index
                .filtered_search(query, count, |key| in_scope(key, scope)),
        }
        .map_err(failed)?;
        Ok((matches.keys, matches.distances))
    }

    /// Candidates to ask USearch for when `top_k` results are wanted.
    ///
    /// With compression on, the ANN ordering is computed over truncated and
    /// usually quantized vectors, so the true top-k is a *subset* of a wider
    /// window. Over-fetching by `rescore_factor` and re-ranking exactly is what
    /// buys back the accuracy the smaller vectors gave up.
    fn candidate_window(&self, top_k: usize, index_size: usize) -> usize {
        if !self.compression.is_active() {
            return top_k.min(index_size);
        }
        top_k
            .saturating_mul(self.compression.rescore_factor())
            .min(index_size)
    }

    /// Score actually reported for a candidate.
    ///
    /// With compression off this is just USearch's cosine distance inverted.
    /// With compression on, USearch's distance was measured in the lossy space
    /// and is only a ranking hint, so the authoritative score is the exact
    /// cosine between the full-precision query and the full-precision stored
    /// vector. A candidate whose full vector is missing — an index rebuilt by
    /// an older build, a truncated side store — falls back to the approximate
    /// score rather than disappearing from the result set.
    fn candidate_similarity(&self, key: u64, query: &[f32], approximate_distance: f32) -> f32 {
        let approximate = 1.0 - approximate_distance;
        let Some(store) = self.rescore.as_ref() else {
            return approximate;
        };
        match store.get(key) {
            // The rescore runs `top_k * rescore_factor` times per query over
            // full-width vectors, so it gets the SIMD kernel rather than the
            // scalar one. Both agree to within f32 reassociation noise; see
            // the property test in `vector_ops`.
            Some(full) => cosine_similarity_simd(query, &full),
            None => {
                tracing::debug!(key, "no full vector to rescore against; using ANN score");
                approximate
            }
        }
    }

    /// Order results by descending score. Only meaningful after rescoring —
    /// USearch already returns its own matches sorted.
    fn sort_by_score_desc<T>(results: &mut [T], score: impl Fn(&T) -> f32) {
        results.sort_by(|a, b| {
            score(b)
                .partial_cmp(&score(a))
                .unwrap_or(CmpOrdering::Equal)
        });
    }

    /// Load an existing uncompressed index from disk, or create a new one.
    pub fn open_or_create(dimension: usize, index_path: PathBuf) -> Result<Self> {
        Self::open_or_create_with_compression(dimension, index_path, VectorIndexCompression::None)
    }

    /// Load an existing index from disk, or create a new one if it doesn't exist.
    ///
    /// The caller is responsible for having run
    /// [`ensure_index_layout_match`](super::dimension_metadata::ensure_index_layout_match)
    /// first: an index file written under a different configuration will fail
    /// to load here rather than being silently reinterpreted.
    pub fn open_or_create_with_compression(
        dimension: usize,
        index_path: PathBuf,
        compression: VectorIndexCompression,
    ) -> Result<Self> {
        let instance = Self::with_compression(dimension, Some(index_path.clone()), compression)?;
        instance.discard_stale_temporaries();

        if index_path.exists() {
            let path_str = index_path.to_string_lossy().to_string();
            // `load`, not `view`: `view` maps the file read-only for an
            // instant, allocation-free open, but this index is mutated in
            // place every time a document is indexed, and USearch cannot grow
            // or rewrite a viewed index. The startup cost `view` would save is
            // reading one already-built graph, which is exactly the cost the
            // manifest check exists to make rare.
            instance.index.load(&path_str).map_err(|e| {
                AppError::InternalError(format!("Failed to load USearch index from disk: {}", e))
            })?;

            if let Some(ref keymap_path) = instance.keymap_path {
                if keymap_path.exists() {
                    instance.load_keymap(keymap_path)?;
                } else {
                    tracing::warn!(
                        "USearch index exists but key map is missing — index will be empty until rebuild"
                    );
                }
            }

            tracing::info!(
                path = %index_path.display(),
                size = instance.index.size(),
                "USearch index loaded from disk"
            );
        } else {
            // Capacity is not reserved up front — `ensure_capacity` grows the
            // index on demand from the first insert. Reserving a fixed block
            // here is what produced the old "crashes past N vectors" cliff.
            tracing::info!(
                path = %index_path.display(),
                "Created new USearch index"
            );
        }

        Ok(instance)
    }

    /// Rebuild the index from SQLite embeddings.
    /// Called when the index file is missing/corrupt or after model changes.
    #[must_use = "returns the number of embeddings added"]
    pub fn rebuild_from_embeddings(
        &self,
        embeddings: Vec<(String, Vec<f32>, String, String, String)>,
    ) -> Result<usize> {
        self.clear()?;

        // Reserve the whole batch up front so the per-vector path never has to
        // grow. Scoped so the lock is released before `add_internal` re-takes it.
        let count = embeddings.len();
        if count > 0 {
            let _guard = self.state.write();
            self.ensure_capacity(count)?;
        }

        let mut added = 0usize;
        for (emb_id, vector, content, chunk_id, doc_id) in embeddings {
            if vector.len() != self.dimension {
                tracing::warn!(
                    emb_id,
                    len = vector.len(),
                    expected = self.dimension,
                    "Skipping embedding with incorrect dimension during rebuild"
                );
                continue;
            }

            match self.add_internal(emb_id, vector, Some(content), Some(chunk_id), Some(doc_id)) {
                Ok(()) => added += 1,
                Err(e) => {
                    tracing::warn!(error = %e, "Failed to add embedding during rebuild");
                }
            }
        }

        // Persist to disk
        self.save_to_disk()?;

        tracing::info!(
            added,
            total = count,
            "USearch index rebuilt from embeddings"
        );
        Ok(added)
    }

    /// Guarantees the index has room for `additional` more vectors.
    ///
    /// USearch does not bounds-check `add`: inserting into an index that is at
    /// capacity writes past the end of its node array and segfaults the process
    /// rather than returning an error. Every insertion path must widen the
    /// reservation first. Growth is geometric so bulk insertion doesn't
    /// reallocate per vector.
    ///
    /// Callers must hold the `state` write lock — reserving while another thread
    /// is inside `add` or `search` is not safe.
    fn ensure_capacity(&self, additional: usize) -> Result<()> {
        let required = self.index.size().saturating_add(additional);
        let current = self.index.capacity();
        if required <= current {
            return Ok(());
        }

        let mut target = if current == 0 {
            INITIAL_INDEX_CAPACITY
        } else {
            current
        };
        while target < required {
            target = target.saturating_mul(2);
        }

        self.index.reserve(target).map_err(|e| {
            AppError::InternalError(format!(
                "Failed to reserve USearch capacity ({} → {}): {}",
                current, target, e
            ))
        })?;

        tracing::debug!(from = current, to = target, "Grew USearch index capacity");
        Ok(())
    }

    /// Internal add method that handles key allocation and metadata storage.
    ///
    /// Holds a single write lock for the entire check-allocate-insert sequence,
    /// preventing TOCTOU races and ensuring key map consistency.
    fn add_internal(
        &self,
        id: String,
        embedding: Vec<f32>,
        content: Option<String>,
        chunk_id: Option<String>,
        document_id: Option<String>,
    ) -> Result<()> {
        self.insert_internal(id, embedding, content, chunk_id, document_id, false)
    }

    fn insert_internal(
        &self,
        id: String,
        embedding: Vec<f32>,
        content: Option<String>,
        chunk_id: Option<String>,
        document_id: Option<String>,
        allow_existing: bool,
    ) -> Result<()> {
        if embedding.len() != self.dimension {
            return Err(AppError::InvalidInput(format!(
                "Embedding dimension mismatch: expected {}, got {}",
                self.dimension,
                embedding.len()
            )));
        }

        // Single write lock for the entire check-allocate-insert sequence
        let mut state = self.state.write();

        // Check for duplicate (atomic with insert — no TOCTOU race)
        if state.id_to_key.contains_key(&id) {
            if allow_existing {
                return Ok(());
            }
            return Err(AppError::InvalidInput(format!(
                "Embedding with id '{}' already exists",
                id
            )));
        }

        // Grow before inserting — USearch segfaults instead of erroring when
        // `add` is called on a full index.
        self.ensure_capacity(1)?;

        // Allocate a new u64 key
        let key = self.next_key.fetch_add(1, Ordering::SeqCst);

        // The full-precision copy has to land before the lossy one, so a
        // crash between the two leaves a rescorable vector USearch cannot
        // find rather than a USearch hit with nothing to rescore it against.
        if let Some(store) = self.rescore.as_ref() {
            store.put(key, &embedding)?;
        }

        // Truncate + renormalize (and let USearch quantize) when configured.
        // `project` borrows for the uncompressed path, so this costs nothing
        // when compression is off.
        let stored = self.compression.project(&embedding)?;
        self.index.add(key, stored.as_ref()).map_err(|e| {
            AppError::InternalError(format!("Failed to add embedding to USearch: {}", e))
        })?;

        state.id_to_key.insert(id.clone(), key);
        state.key_to_id.insert(key, id.clone());

        if let (Some(content), Some(chunk_id), Some(doc_id)) = (content, chunk_id, document_id) {
            state.metadata.insert(
                id,
                VectorMeta {
                    chunk_id,
                    document_id: doc_id,
                    content,
                },
            );
        }

        Ok(())
    }

    /// Save the index and key map to disk.
    ///
    /// Both go out through a temporary file and a rename, so a crash mid-save
    /// leaves the previous snapshot intact rather than a half-written graph
    /// USearch would refuse to load. The manifest that vouches for this
    /// snapshot is written by the caller afterwards — see
    /// [`super::manifest`] — because only it knows what SQLite looked like.
    pub fn save_to_disk(&self) -> Result<()> {
        let _snapshot = self.persistence.lock();
        // Hold the same read lock used by searches while saving the vectors
        // and metadata, so additions cannot split the two snapshots.
        let state = self.state.read();
        if let Some(ref path) = self.index_path {
            // Ensure parent directory exists
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).map_err(|e| {
                    AppError::FileStorage(format!("Failed to create index directory: {}", e))
                })?;
            }

            let temp = temp_path_for(path);
            let temp_str = temp.to_string_lossy().to_string();
            self.index.save(&temp_str).map_err(|e| {
                AppError::FileStorage(format!("Failed to save USearch index: {}", e))
            })?;
            std::fs::rename(&temp, path).map_err(|e| {
                let _ = std::fs::remove_file(&temp);
                AppError::FileStorage(format!("Failed to replace USearch index: {}", e))
            })?;

            tracing::debug!(path = %path.display(), "USearch index saved to disk");
        }

        if let Some(ref path) = self.keymap_path {
            self.save_keymap(path, &state)?;
            tracing::debug!(path = %path.display(), "Key map saved to disk");
        }

        *self.dirty.lock() = DirtyState::default();

        Ok(())
    }

    /// Remove temporaries a crashed save left behind.
    ///
    /// They are never read — the real files are only ever replaced by an
    /// atomic rename — but leaving a corpus-sized file lying around is not
    /// something a user should have to clean up by hand.
    fn discard_stale_temporaries(&self) {
        for path in [self.index_path.as_deref(), self.keymap_path.as_deref()]
            .into_iter()
            .flatten()
            .map(temp_path_for)
        {
            if path.exists() {
                match std::fs::remove_file(&path) {
                    Ok(()) => {
                        tracing::info!(path = %path.display(), "Removed a temporary file from an interrupted index save")
                    }
                    Err(e) => {
                        tracing::warn!(path = %path.display(), error = %e, "Could not remove a stale index temporary")
                    }
                }
            }
        }
    }

    /// Save the key map to a JSON file (atomic via temp file + rename).
    fn save_keymap(&self, path: &Path, state: &KeyState) -> Result<()> {
        let data = KeyMapData {
            id_to_key: state.id_to_key.clone(),
            next_key: self.next_key.load(Ordering::SeqCst),
            metadata: state
                .metadata
                .iter()
                .map(|(id, m)| {
                    (
                        id.clone(),
                        SerializableMeta {
                            chunk_id: m.chunk_id.clone(),
                            document_id: m.document_id.clone(),
                        },
                    )
                })
                .collect(),
        };

        let bytes = serde_json::to_vec(&data)
            .map_err(|e| AppError::Serialization(format!("Failed to serialize key map: {}", e)))?;
        write_file_atomically(path, &bytes)
    }

    /// Re-attach chunk text to keys loaded from disk.
    ///
    /// The key map deliberately does not persist content, so an index opened
    /// without a rebuild knows which chunk each vector is but not what it
    /// says. `rows` is `(embedding id, content)` straight out of SQLite.
    /// Returns how many keys were filled. Unknown ids are ignored: SQLite
    /// holds chunks this generation has no vector for.
    pub fn hydrate_content<I>(&self, rows: I) -> usize
    where
        I: IntoIterator<Item = (String, String)>,
    {
        let mut state = self.state.write();
        let mut filled = 0;
        for (id, content) in rows {
            if let Some(meta) = state.metadata.get_mut(&id) {
                meta.content = content;
                filled += 1;
            }
        }
        filled
    }

    /// Load the key map from a JSON file.
    fn load_keymap(&self, path: &Path) -> Result<()> {
        let file = std::fs::File::open(path)
            .map_err(|e| AppError::FileStorage(format!("Failed to open key map file: {}", e)))?;
        let reader = std::io::BufReader::new(file);
        let data: KeyMapData = serde_json::from_reader(reader).map_err(|e| {
            AppError::Deserialization(format!("Failed to deserialize key map: {}", e))
        })?;

        // Rebuild maps under a single lock
        let mut state = self.state.write();

        state.id_to_key = data.id_to_key.clone();
        state.key_to_id = data
            .id_to_key
            .into_iter()
            .map(|(id, key)| (key, id))
            .collect();

        for (id, m) in data.metadata {
            state.metadata.insert(
                id,
                VectorMeta {
                    chunk_id: m.chunk_id,
                    document_id: m.document_id,
                    // Filled by `hydrate_content` from SQLite; see `KeyMapData`.
                    content: String::new(),
                },
            );
        }

        self.next_key.store(data.next_key, Ordering::SeqCst);

        Ok(())
    }
}

/// Flush the USearch index to disk on drop.
///
/// Best-effort only. Tauri exits the process from inside its event loop, so in
/// the real app nothing that `AppHandle` owns is ever dropped; the shutdown
/// flush in [`super::persistence::IndexPersistence`] is what actually saves.
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

/// How far a compressed vector's approximate rank may misrepresent its true
/// cosine. Quantization perturbs each component slightly, so a candidate can
/// rescore a little above or below its neighbours; the scoped search will not
/// abandon a widening pass over a gap smaller than this.
const RESCORE_ORDER_MARGIN: f32 = 0.05;

impl VectorSearchPort for USearchVectorIndex {
    fn search(
        &self,
        query_embedding: &[f32],
        top_k: usize,
        threshold: f32,
    ) -> Result<Vec<SearchResultPortDto>> {
        self.search_scoped(query_embedding, top_k, threshold, None)
    }

    fn search_scoped(
        &self,
        query_embedding: &[f32],
        top_k: usize,
        threshold: f32,
        allowed_document_ids: Option<&HashSet<String>>,
    ) -> Result<Vec<SearchResultPortDto>> {
        if query_embedding.len() != self.dimension {
            return Err(AppError::InvalidInput(format!(
                "Query embedding dimension mismatch: expected {}, got {}",
                self.dimension,
                query_embedding.len()
            )));
        }

        if top_k == 0 {
            return Err(AppError::InvalidInput(
                "top_k must be greater than 0".to_string(),
            ));
        }

        if !(0.0..=1.0).contains(&threshold) {
            return Err(AppError::InvalidInput(format!(
                "Threshold must be in [0.0, 1.0], got {}",
                threshold
            )));
        }

        let state = self.state.read();
        if self.index.size() == 0 {
            return Ok(Vec::new());
        }
        if matches!(allowed_document_ids, Some(scope) if scope.is_empty()) {
            return Ok(Vec::new());
        }

        let index_size = self.index.size();
        let rescoring = self.compression.is_active();
        let mut candidate_k = self.candidate_window(top_k, index_size);
        if allowed_document_ids.is_some() {
            // A predicate keeps out-of-scope neighbours out of the result set
            // but cannot conjure in-scope ones the traversal never reached, so
            // a scoped search still asks for a wider window than an open one.
            let widened_start = top_k.saturating_mul(4).max(32).min(index_size);
            candidate_k = candidate_k.max(widened_start);
        }

        // The stored vectors are truncated/quantized, so the query has to be
        // projected into the same space before USearch can compare them. The
        // untouched `query_embedding` stays the reference for rescoring.
        let projected_query = self.compression.project(query_embedding)?;

        // At most two passes: the sized window, then — only if the scope is so
        // sparse that the first traversal could not fill it — one exhaustive
        // pass. The old code doubled `candidate_k` until it gave up, which on
        // a real session cost nine passes over 29,766 vectors to return the
        // empty list its first pass had already established.
        loop {
            let (keys, distances) = self.nearest(
                &state,
                projected_query.as_ref(),
                candidate_k,
                allowed_document_ids,
            )?;

            let mut results = Vec::new();
            // The best similarity any candidate reached. A second pass is
            // worth doing when the scope is what emptied the window, and
            // pointless when the threshold is: USearch returns neighbours
            // nearest first, so a wider window can only add vectors that score
            // lower than the ones already rejected.
            let mut best_seen: Option<f32> = None;
            for (&key, &distance) in keys.iter().zip(distances.iter()) {
                let Some(id) = state.key_to_id.get(&key) else {
                    continue;
                };
                let (doc_id, chunk_id, content) = if let Some(m) = state.metadata.get(id) {
                    (
                        m.document_id.as_str(),
                        m.chunk_id.as_str(),
                        m.content.as_str(),
                    )
                } else {
                    (id.as_str(), id.as_str(), "")
                };

                // USearch cosine distance = 1.0 - cosine_similarity; rescoring
                // replaces it with the exact full-precision cosine. Threshold
                // is applied to the score we report, not to the lossy one.
                let similarity = self.candidate_similarity(key, query_embedding, distance);
                best_seen = Some(best_seen.map_or(similarity, |best: f32| best.max(similarity)));
                if similarity < threshold {
                    continue;
                }

                results.push(SearchResultPortDto {
                    doc_id: doc_id.to_string(),
                    chunk_id: chunk_id.to_string(),
                    score: similarity,
                    content: content.to_string(),
                });
                // Rescoring reorders the window, so the first `top_k` matches
                // USearch handed back are not necessarily the best `top_k`.
                if !rescoring && results.len() >= top_k {
                    break;
                }
            }

            if rescoring {
                Self::sort_by_score_desc(&mut results, |r| r.score);
            }

            if results.len() >= top_k || candidate_k >= index_size {
                if results.len() > top_k {
                    results.truncate(top_k);
                }
                return Ok(results);
            }

            // When rescoring is active the approximate order USearch returns
            // is not exactly the order of true cosine, so a vector further out
            // can still rescore slightly higher. Give that reordering room:
            // only stop when the nearest candidate misses by more than
            // quantization could explain.
            if let Some(best) = best_seen {
                let margin = if rescoring { RESCORE_ORDER_MARGIN } else { 0.0 };
                if best + margin < threshold {
                    tracing::debug!(
                        candidate_k,
                        best_similarity = best,
                        threshold,
                        "USearch search stopping: nearest candidate is below threshold"
                    );
                    return Ok(results);
                }
            }

            tracing::debug!(
                candidate_k,
                index_size,
                top_k,
                "USearch scoped search falling back to an exhaustive pass"
            );
            candidate_k = index_size;
        }
    }

    fn add_embedding(&self, id: String, embedding: Vec<f32>) -> Result<()> {
        self.add_internal(id, embedding, None, None, None)?;
        self.note_mutation()
    }

    fn add_embedding_with_content(
        &self,
        id: String,
        embedding: Vec<f32>,
        content: String,
        chunk_id: String,
        document_id: String,
    ) -> Result<()> {
        self.add_internal(
            id,
            embedding,
            Some(content),
            Some(chunk_id),
            Some(document_id),
        )?;
        self.note_mutation()
    }

    fn publish_embeddings(&self, entries: Vec<VectorIndexEntry>) -> Result<()> {
        // Validate before changing the runtime index. IDs come from committed
        // SQLite rows; an existing ID is the same immutable chunk on a retry.
        if entries
            .iter()
            .any(|entry| entry.embedding.len() != self.dimension)
        {
            return Err(AppError::InvalidInput(
                "Embedding dimension mismatch during publication".into(),
            ));
        }
        for entry in entries {
            self.insert_internal(
                entry.id,
                entry.embedding,
                Some(entry.content),
                Some(entry.chunk_id),
                Some(entry.document_id),
                true,
            )?;
        }
        self.note_mutation()
    }

    fn remove_embedding(&self, id: &str) -> Result<()> {
        self.remove_embeddings(&[id.to_owned()])
    }

    fn remove_embeddings(&self, ids: &[String]) -> Result<()> {
        let mut state = self.state.write();
        let mut changed = false;
        for id in ids {
            let Some(&key) = state.id_to_key.get(id) else {
                continue;
            };
            self.index.remove(key).map_err(|e| {
                AppError::InternalError(format!("Failed to remove embedding from USearch: {}", e))
            })?;
            if let Some(store) = self.rescore.as_ref() {
                store.remove(key)?;
            }
            state.id_to_key.remove(id);
            state.key_to_id.remove(&key);
            state.metadata.remove(id);
            changed = true;
        }
        drop(state);
        if changed {
            self.note_mutation()?;
        }
        Ok(())
    }

    fn clear(&self) -> Result<()> {
        let mut state = self.state.write();
        self.index.reset().map_err(|e| {
            AppError::InternalError(format!("Failed to reset USearch index: {}", e))
        })?;
        // Keys restart at 1 after a reset, so stale slots would otherwise be
        // reused with another chunk's vector still in them.
        if let Some(store) = self.rescore.as_ref() {
            store.clear()?;
        }
        state.id_to_key.clear();
        state.key_to_id.clear();
        state.metadata.clear();
        self.next_key.store(1, Ordering::SeqCst);
        drop(state);
        // No save here: `clear` is only ever the first half of a rebuild, and
        // writing an empty index between the two halves would hand a crash a
        // file claiming the library has no vectors.
        self.dirty.lock().mark();
        Ok(())
    }

    fn count(&self) -> usize {
        self.state.read().id_to_key.len()
    }

    fn dimension(&self) -> usize {
        self.dimension
    }
}

#[async_trait]
impl SearchServiceTrait for USearchVectorIndex {
    fn search(&self, query_embedding: &[f32], top_k: usize) -> Result<Vec<SearchResult>> {
        let state = self.state.read();
        if self.index.size() == 0 {
            return Ok(Vec::new());
        }

        let start = std::time::Instant::now();

        // Over-fetch when the stored vectors are lossy; the exact top-k is a
        // subset of the wider window, not a prefix of it.
        let candidate_k = self.candidate_window(top_k, self.index.size());
        let projected_query = self.compression.project(query_embedding)?;

        let (keys, distances) =
            self.nearest(&state, projected_query.as_ref(), candidate_k, None)?;

        let mut results: Vec<SearchResult> = keys
            .iter()
            .zip(distances.iter())
            .enumerate()
            .filter_map(|(idx, (&key, &distance))| {
                let similarity = self.candidate_similarity(key, query_embedding, distance);
                state.key_to_id.get(&key).map(|id| {
                    let (resolved_chunk_id, resolved_document_id) =
                        if let Some(metadata) = state.metadata.get(id) {
                            (
                                metadata.chunk_id.clone(),
                                Some(metadata.document_id.clone()),
                            )
                        } else if let Some(stripped_chunk_id) = id.strip_prefix("emb_") {
                            // Backward compatibility: legacy indexes may only store embedding IDs.
                            // The DB convention is emb_<chunk_id>, so recover the chunk ID here.
                            (stripped_chunk_id.to_string(), None)
                        } else {
                            (id.clone(), None)
                        };

                    SearchResult {
                        id: resolved_chunk_id,
                        score: similarity,
                        index: idx,
                        filename: None,
                        mime_type: None,
                        size_bytes: None,
                        created_at: None,
                        content: None,
                        file_id: resolved_document_id.clone(),
                        file_path: None,
                        file_name: None,
                        file_extension: None,
                        file_category: None,
                        is_indexed: None,
                        document_id: resolved_document_id,
                        snippet: None,
                        chunk_index: None,
                        updated_at: None,
                    }
                })
            })
            .collect();

        if self.compression.is_active() {
            Self::sort_by_score_desc(&mut results, |r| r.score);
            results.truncate(top_k);
            // `index` is the caller-visible rank, so it has to follow the
            // rescored order rather than USearch's approximate one.
            for (rank, result) in results.iter_mut().enumerate() {
                result.index = rank;
            }
        }

        let duration = start.elapsed();
        tracing::debug!(
            duration_ms = duration.as_millis(),
            results_found = results.len(),
            index_size = self.index.size(),
            "USearch vector search completed"
        );

        Ok(results)
    }

    async fn search_with_metadata(
        &self,
        query_embedding: &[f32],
        top_k: usize,
    ) -> Result<Vec<SearchResult>> {
        // Metadata enrichment happens at higher layers (SearchEnrichmentService)
        SearchServiceTrait::search(self, query_embedding, top_k)
    }

    fn search_with_threshold(
        &self,
        query_embedding: &[f32],
        top_k: usize,
        threshold: f32,
    ) -> Result<Vec<SearchResult>> {
        let results = SearchServiceTrait::search(self, query_embedding, top_k)?;
        Ok(results
            .into_iter()
            .filter(|r| r.score >= threshold)
            .collect())
    }

    fn batch_search(
        &self,
        query_embeddings: &[Vec<f32>],
        top_k: usize,
    ) -> Result<Vec<Vec<SearchResult>>> {
        query_embeddings
            .iter()
            .map(|query| SearchServiceTrait::search(self, query, top_k))
            .collect()
    }
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]
mod tests {
    use super::*;
    // The scalar reference, so these assertions do not simply repeat whatever
    // the SIMD kernel the rescore now uses happens to compute.
    use crate::features::search::engine::vector_ops::cosine_similarity_naive;

    fn make_test_index(dim: usize) -> USearchVectorIndex {
        USearchVectorIndex::new(dim, None).unwrap_or_else(|e| {
            panic!("Failed to create test index: {}", e);
        })
    }

    /// Inserting more vectors than the initially reserved capacity used to walk
    /// off the end of USearch's node array and take the whole process down with
    /// SIGSEGV. `ensure_capacity` must grow the reservation transparently.
    #[test]
    fn growing_past_initial_capacity_does_not_crash() {
        let dim = 4;
        let index = make_test_index(dim);

        let count = INITIAL_INDEX_CAPACITY + 64;
        for i in 0..count {
            let angle = i as f32;
            index
                .add_embedding(
                    format!("doc{}", i),
                    vec![angle.cos(), angle.sin(), 0.0, 0.0],
                )
                .unwrap_or_else(|e| panic!("add {} failed: {}", i, e));
        }

        assert_eq!(index.index.size(), count);
        assert!(
            index.index.capacity() >= count,
            "capacity {} should have grown to hold {} vectors",
            index.index.capacity(),
            count
        );

        let results = VectorSearchPort::search(&index, &[1.0, 0.0, 0.0, 0.0], 5, 0.0)
            .unwrap_or_else(|e| panic!("search failed: {}", e));
        assert_eq!(results.len(), 5, "search must still work after growth");
    }

    #[test]
    fn test_add_and_search() {
        let dim = 4;
        let index = make_test_index(dim);

        index
            .add_embedding("doc1".into(), vec![1.0, 0.0, 0.0, 0.0])
            .unwrap_or_else(|e| panic!("add failed: {}", e));
        index
            .add_embedding("doc2".into(), vec![0.0, 1.0, 0.0, 0.0])
            .unwrap_or_else(|e| panic!("add failed: {}", e));
        index
            .add_embedding("doc3".into(), vec![0.9, 0.1, 0.0, 0.0])
            .unwrap_or_else(|e| panic!("add failed: {}", e));

        let results = VectorSearchPort::search(&index, &[1.0, 0.0, 0.0, 0.0], 2, 0.0)
            .unwrap_or_else(|e| panic!("search failed: {}", e));

        assert_eq!(results.len(), 2);
        // doc1 should be the closest match
        assert_eq!(results[0].doc_id, "doc1");
    }

    #[test]
    fn test_add_with_content() {
        let dim = 4;
        let index = make_test_index(dim);

        index
            .add_embedding_with_content(
                "emb_1".into(),
                vec![1.0, 0.0, 0.0, 0.0],
                "Hello world".into(),
                "chunk_1".into(),
                "doc_1".into(),
            )
            .unwrap_or_else(|e| panic!("add_with_content failed: {}", e));

        let results = VectorSearchPort::search(&index, &[1.0, 0.0, 0.0, 0.0], 1, 0.0)
            .unwrap_or_else(|e| panic!("search failed: {}", e));

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].doc_id, "doc_1");
        assert_eq!(results[0].chunk_id, "chunk_1");
        assert_eq!(results[0].content, "Hello world");
    }

    #[test]
    fn test_remove() {
        let dim = 4;
        let index = make_test_index(dim);

        index
            .add_embedding("doc1".into(), vec![1.0, 0.0, 0.0, 0.0])
            .unwrap_or_else(|e| panic!("add failed: {}", e));

        assert_eq!(index.count(), 1);

        index
            .remove_embedding("doc1")
            .unwrap_or_else(|e| panic!("remove failed: {}", e));

        // count() now uses key map length which accurately reflects live entries
        assert_eq!(index.count(), 0);
    }

    #[test]
    fn batch_removal_persists_and_preserves_other_documents() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("removal.usearch");
        let index = USearchVectorIndex::new(4, Some(path.clone())).unwrap();
        index
            .publish_embeddings(
                (0..128)
                    .map(|i| VectorIndexEntry {
                        id: format!("emb_{i}"),
                        embedding: vec![1.0, i as f32 / 128.0, 0.0, 0.0],
                        content: format!("Passage {i}"),
                        chunk_id: format!("chunk_{i}"),
                        document_id: if i < 127 { "deleted" } else { "kept" }.into(),
                    })
                    .collect(),
            )
            .unwrap();
        let mut ids: Vec<_> = (0..127).map(|i| format!("emb_{i}")).collect();
        ids.push("missing".into());
        ids.push("emb_0".into());
        index.remove_embeddings(&ids).unwrap();
        index.remove_embeddings(&ids).unwrap();
        let loaded = USearchVectorIndex::open_or_create(4, path).unwrap();
        assert_eq!(loaded.count(), 1);
        let hits = VectorSearchPort::search(&loaded, &[1.0, 0.0, 0.0, 0.0], 10, 0.0).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].doc_id, "kept");
    }

    #[test]
    fn test_duplicate_rejected() {
        let dim = 4;
        let index = make_test_index(dim);

        index
            .add_embedding("doc1".into(), vec![1.0, 0.0, 0.0, 0.0])
            .unwrap_or_else(|e| panic!("add failed: {}", e));

        let result = index.add_embedding("doc1".into(), vec![0.0, 1.0, 0.0, 0.0]);
        assert!(result.is_err());
    }

    #[test]
    fn test_dimension_mismatch() {
        let index = make_test_index(4);

        let result = index.add_embedding("doc1".into(), vec![1.0, 0.0]); // wrong dim
        assert!(result.is_err());
    }

    #[test]
    fn test_clear() {
        let dim = 4;
        let index = make_test_index(dim);

        index
            .add_embedding("doc1".into(), vec![1.0, 0.0, 0.0, 0.0])
            .unwrap_or_else(|e| panic!("add failed: {}", e));
        index
            .add_embedding("doc2".into(), vec![0.0, 1.0, 0.0, 0.0])
            .unwrap_or_else(|e| panic!("add failed: {}", e));

        index
            .clear()
            .unwrap_or_else(|e| panic!("clear failed: {}", e));

        assert_eq!(index.count(), 0);
        assert!(index.state.read().id_to_key.is_empty());
    }

    #[test]
    fn test_persistence() {
        let dim = 4;
        let temp_dir = tempfile::TempDir::new().unwrap_or_else(|e| panic!("tempdir: {}", e));
        let index_path = temp_dir.path().join("test_index.usearch");

        {
            let index = USearchVectorIndex::open_or_create(dim, index_path.clone())
                .unwrap_or_else(|e| panic!("open_or_create failed: {}", e));

            index
                .add_embedding_with_content(
                    "emb_1".into(),
                    vec![1.0, 0.0, 0.0, 0.0],
                    "test content".into(),
                    "chunk_1".into(),
                    "doc_1".into(),
                )
                .unwrap_or_else(|e| panic!("add failed: {}", e));

            // save_to_disk is called by add_embedding_with_content, but also by Drop
        }

        // Reload
        let index2 = USearchVectorIndex::open_or_create(dim, index_path)
            .unwrap_or_else(|e| panic!("reload failed: {}", e));

        assert_eq!(index2.count(), 1);

        let results = VectorSearchPort::search(&index2, &[1.0, 0.0, 0.0, 0.0], 1, 0.0)
            .unwrap_or_else(|e| panic!("search failed: {}", e));

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].chunk_id, "chunk_1");
        assert_eq!(results[0].doc_id, "doc_1");
        // Chunk text is not persisted with the key map — SQLite already has
        // it, and a copy of the whole corpus is what made every save expensive.
        assert_eq!(results[0].content, "");

        assert_eq!(
            index2.hydrate_content([("emb_1".to_string(), "test content".to_string())]),
            1
        );
        let rehydrated = VectorSearchPort::search(&index2, &[1.0, 0.0, 0.0, 0.0], 1, 0.0).unwrap();
        assert_eq!(rehydrated[0].content, "test content");
    }

    #[test]
    fn hydration_ignores_chunks_this_generation_has_no_vector_for() {
        let index = make_test_index(4);
        index
            .add_embedding_with_content(
                "emb_1".into(),
                vec![1.0, 0.0, 0.0, 0.0],
                String::new(),
                "chunk_1".into(),
                "doc_1".into(),
            )
            .unwrap();
        assert_eq!(
            index.hydrate_content([
                ("emb_1".to_string(), "kept".to_string()),
                ("emb_absent".to_string(), "dropped".to_string()),
            ]),
            1
        );
        let hits = VectorSearchPort::search(&index, &[1.0, 0.0, 0.0, 0.0], 1, 0.0).unwrap();
        assert_eq!(hits[0].content, "kept");
    }

    #[test]
    fn batch_publication_is_retryable_and_persists_all_source_metadata() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("batch.usearch");
        let index = USearchVectorIndex::new(4, Some(path.clone())).unwrap();
        let entries = || {
            (0..128)
                .map(|i| VectorIndexEntry {
                    id: format!("emb_{i}"),
                    embedding: vec![1.0, i as f32 / 128.0, 0.0, 0.0],
                    content: format!("Source passage {i}"),
                    chunk_id: format!("chunk_{i}"),
                    document_id: "pdf".into(),
                })
                .collect()
        };
        index.publish_embeddings(entries()).unwrap();
        index.publish_embeddings(entries()).unwrap();
        assert_eq!(index.count(), 128);
        // Open before Drop to prove publication itself flushed the entire batch.
        let loaded = USearchVectorIndex::open_or_create(4, path).unwrap();
        assert_eq!(loaded.count(), 128);
        let hits = VectorSearchPort::search(&loaded, &[1.0, 0.0, 0.0, 0.0], 128, 0.0).unwrap();
        assert_eq!(hits.len(), 128);
        assert!(hits
            .iter()
            .all(|hit| hit.doc_id == "pdf" && hit.chunk_id.starts_with("chunk_")));
    }

    /// The whole point of coalescing: publishing does not touch the disk, and
    /// one save afterwards captures everything that was published.
    #[test]
    fn coalesced_publication_writes_once_instead_of_once_per_document() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("coalesced.usearch");
        let index = USearchVectorIndex::new(4, Some(path.clone()))
            .unwrap()
            .with_coalesced_saves();

        for document in 0..16 {
            index
                .publish_embeddings(
                    (0..4)
                        .map(|chunk| VectorIndexEntry {
                            id: format!("emb_{document}_{chunk}"),
                            embedding: vec![1.0, document as f32 / 16.0, chunk as f32, 0.0],
                            content: String::new(),
                            chunk_id: format!("chunk_{document}_{chunk}"),
                            document_id: format!("doc_{document}"),
                        })
                        .collect(),
                )
                .unwrap();
            assert!(
                !path.exists(),
                "document {document} wrote the whole index to disk"
            );
        }
        assert!(index.is_dirty());

        index.save_to_disk().unwrap();
        assert!(!index.is_dirty());
        let loaded = USearchVectorIndex::open_or_create(4, path).unwrap();
        assert_eq!(
            loaded.count(),
            64,
            "one save must capture every publication"
        );
    }

    /// An index without a flusher — the summaries index, the eval harness, the
    /// tests above — keeps writing on every mutation.
    #[test]
    fn the_default_policy_still_saves_immediately() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("immediate.usearch");
        let index = USearchVectorIndex::new(4, Some(path.clone())).unwrap();
        index
            .add_embedding("a".into(), vec![1.0, 0.0, 0.0, 0.0])
            .unwrap();
        assert!(path.exists());
        assert!(!index.is_dirty());
    }

    #[test]
    fn a_save_replaces_the_index_atomically() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("atomic.usearch");
        let index = USearchVectorIndex::new(4, Some(path.clone()))
            .unwrap()
            .with_coalesced_saves();
        index
            .add_embedding("a".into(), vec![1.0, 0.0, 0.0, 0.0])
            .unwrap();
        index.save_to_disk().unwrap();

        let temp = PathBuf::from({
            let mut p = path.clone().into_os_string();
            p.push(".tmp");
            p
        });
        assert!(
            !temp.exists(),
            "a finished save must not leave its staging file behind"
        );

        // What a crash between `save` and `rename` leaves. It must never be
        // mistaken for the index itself.
        std::fs::write(&temp, b"half a graph").unwrap();
        let reopened = USearchVectorIndex::open_or_create(4, path).unwrap();
        assert_eq!(reopened.count(), 1);
        assert!(!temp.exists(), "a stale staging file should be cleaned up");
    }

    #[test]
    fn test_concurrent_add_no_desync() {
        use std::sync::Arc;
        use std::thread;

        let dim = 4;
        let index = Arc::new(make_test_index(dim));

        let mut handles = Vec::new();
        for i in 0..10 {
            let idx = index.clone();
            handles.push(thread::spawn(move || {
                let id = format!("doc_{}", i);
                let mut embedding = vec![0.0f32; 4];
                embedding[i % 4] = 1.0;
                idx.add_internal(id, embedding, None, None, None)
            }));
        }

        let mut successes = 0;
        for h in handles {
            if h.join().unwrap().is_ok() {
                successes += 1;
            }
        }

        assert_eq!(successes, 10);
        assert_eq!(index.count(), 10);

        let state = index.state.read();
        assert_eq!(state.id_to_key.len(), 10);
        assert_eq!(state.key_to_id.len(), 10);
        for (id, &key) in &state.id_to_key {
            assert_eq!(state.key_to_id.get(&key), Some(id));
        }
    }

    #[test]
    fn test_keymap_path_derivation() {
        let dim = 4;
        let path = PathBuf::from("/data/usearch_index.usearch");
        let index = USearchVectorIndex::new(dim, Some(path)).unwrap();
        assert_eq!(
            index.keymap_path.as_deref(),
            Some(Path::new("/data/usearch_index.keymap.json"))
        );

        // Multi-dotted filename
        let path2 = PathBuf::from("/data/index.v2.usearch");
        let index2 = USearchVectorIndex::new(dim, Some(path2)).unwrap();
        assert_eq!(
            index2.keymap_path.as_deref(),
            Some(Path::new("/data/index.v2.keymap.json"))
        );
    }

    // ---------------------------------------------------------------------
    // Compression
    // ---------------------------------------------------------------------

    fn unit(values: &[f32]) -> Vec<f32> {
        let norm = values.iter().map(|v| v * v).sum::<f32>().sqrt();
        values.iter().map(|v| v / norm).collect()
    }

    /// Deterministic stand-in for a Matryoshka embedding: pseudo-random
    /// directions whose energy decays along the axis, which is the property
    /// truncation relies on.
    fn synthetic_vector(seed: u64, dim: usize, decay: f32) -> Vec<f32> {
        let mut x = seed
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407)
            | 1;
        let mut values = Vec::with_capacity(dim);
        for i in 0..dim {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            let unit_interval = (x >> 40) as f32 / (1u64 << 24) as f32;
            values.push((unit_interval * 2.0 - 1.0) * decay.powi(i as i32));
        }
        unit(&values)
    }

    fn i8_compression(dims: usize) -> VectorIndexCompression {
        VectorIndexCompression::truncated(dims, VectorQuantization::I8)
    }

    fn exact_cosine_ranking(query: &[f32], corpus: &[(String, Vec<f32>)]) -> Vec<String> {
        let mut scored: Vec<(String, f32)> = corpus
            .iter()
            .map(|(id, v)| (id.clone(), cosine_similarity_naive(query, v)))
            .collect();
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(CmpOrdering::Equal));
        scored.into_iter().map(|(id, _)| id).collect()
    }

    #[test]
    fn compression_shrinks_the_usearch_vector_but_not_the_reported_dimension() {
        let index = USearchVectorIndex::with_compression(1024, None, i8_compression(512)).unwrap();
        assert_eq!(
            VectorSearchPort::dimension(&index),
            1024,
            "callers still hand in full embeddings"
        );
        assert_eq!(index.index.dimensions(), 512);
        assert_eq!(index.compression().bytes_per_vector(1024), 512);
    }

    #[test]
    fn a_configuration_wider_than_the_embedding_is_rejected() {
        assert!(USearchVectorIndex::with_compression(384, None, i8_compression(512)).is_err());
        assert!(USearchVectorIndex::with_compression(384, None, i8_compression(0)).is_err());
    }

    /// The whole point of rescoring: the quantized, truncated index ranks `a`
    /// first because their prefixes are collinear, but `b` is the better match
    /// on the full vector. The exact score must win.
    #[test]
    fn rescoring_overrides_the_approximate_order() {
        let index = USearchVectorIndex::with_compression(4, None, i8_compression(2)).unwrap();
        let a = unit(&[1.0, 0.0, 0.0, 0.0]);
        let b = unit(&[0.8, 0.6, 1.0, 0.0]);
        index.add_embedding("a".into(), a.clone()).unwrap();
        index.add_embedding("b".into(), b.clone()).unwrap();

        let query = unit(&[1.0, 0.0, 1.0, 0.0]);

        // Precondition: in the truncated space `a` is the nearer neighbour.
        let projected = index.compression().project(&query).unwrap();
        let approximate = index.index.search(projected.as_ref(), 2).unwrap();
        let approximate_first = approximate.keys.first().copied().unwrap();
        assert_eq!(
            index
                .state
                .read()
                .key_to_id
                .get(&approximate_first)
                .map(String::as_str),
            Some("a"),
            "precondition: the lossy index prefers `a`"
        );

        let results = VectorSearchPort::search(&index, &query, 2, 0.0).unwrap();
        assert_eq!(
            results
                .iter()
                .map(|r| r.doc_id.as_str())
                .collect::<Vec<_>>(),
            vec!["b", "a"],
            "rescoring must reorder the candidate window"
        );

        // …and the reported score is the exact cosine, not the quantized one.
        let exact_b = cosine_similarity_naive(&query, &b);
        let exact_a = cosine_similarity_naive(&query, &a);
        assert!((results[0].score - exact_b).abs() < 1e-5, "{:?}", results);
        assert!((results[1].score - exact_a).abs() < 1e-5, "{:?}", results);
        assert!(exact_b > exact_a);
    }

    /// The contract users actually care about: turning compression on must not
    /// change what comes back.
    #[test]
    fn compressed_search_returns_the_same_top_k_as_an_f32_index() {
        const DIM: usize = 16;
        const CORPUS: u64 = 96;
        const TOP_K: usize = 5;

        let corpus: Vec<(String, Vec<f32>)> = (0..CORPUS)
            .map(|i| (format!("doc{i}"), synthetic_vector(i, DIM, 0.8)))
            .collect();

        let plain = USearchVectorIndex::new(DIM, None).unwrap();
        let compressed =
            USearchVectorIndex::with_compression(DIM, None, i8_compression(8)).unwrap();
        for (id, vector) in &corpus {
            plain.add_embedding(id.clone(), vector.clone()).unwrap();
            compressed
                .add_embedding(id.clone(), vector.clone())
                .unwrap();
        }

        for seed in 1_000..1_010u64 {
            let query = synthetic_vector(seed, DIM, 0.8);

            let expected = exact_cosine_ranking(&query, &corpus);
            let expected: Vec<&str> = expected.iter().take(TOP_K).map(String::as_str).collect();

            let plain_hits = VectorSearchPort::search(&plain, &query, TOP_K, 0.0).unwrap();
            let plain_ids: Vec<&str> = plain_hits.iter().map(|r| r.doc_id.as_str()).collect();
            assert_eq!(plain_ids, expected, "f32 baseline drifted for seed {seed}");

            let compressed_hits =
                VectorSearchPort::search(&compressed, &query, TOP_K, 0.0).unwrap();
            let compressed_ids: Vec<&str> =
                compressed_hits.iter().map(|r| r.doc_id.as_str()).collect();
            assert_eq!(
                compressed_ids, plain_ids,
                "compressed top-{TOP_K} diverged for seed {seed}"
            );

            for (compressed_hit, plain_hit) in compressed_hits.iter().zip(plain_hits.iter()) {
                assert!(
                    (compressed_hit.score - plain_hit.score).abs() < 1e-3,
                    "score {} vs {} for {}",
                    compressed_hit.score,
                    plain_hit.score,
                    compressed_hit.doc_id
                );
            }
        }
    }

    /// Scope post-filtering widens the candidate window in a loop; the rescore
    /// has to run on every pass, not just the first.
    ///
    /// The scope is deliberately the three *worst* matches in the corpus, so
    /// the first candidate window cannot satisfy it and the loop must widen to
    /// exhaustion. Because the scope holds exactly `top_k` documents the
    /// expected answer is fully determined: those three, in exact-cosine order.
    #[test]
    fn scoped_search_rescores_inside_the_widening_loop() {
        const DIM: usize = 16;
        const TOP_K: usize = 3;

        let corpus: Vec<(String, Vec<f32>)> = (0..128u64)
            .map(|i| (format!("chunk{i}"), synthetic_vector(i, DIM, 0.8)))
            .collect();
        let query = synthetic_vector(7_777, DIM, 0.8);

        let index = USearchVectorIndex::with_compression(DIM, None, i8_compression(8)).unwrap();
        for (i, (id, vector)) in corpus.iter().enumerate() {
            index
                .add_embedding_with_content(
                    id.clone(),
                    vector.clone(),
                    format!("passage {i}"),
                    id.clone(),
                    format!("doc{i}"),
                )
                .unwrap();
        }

        // Rank every chunk exactly, then scope to the weakest three that still
        // clear a 0.0 threshold (a negative cosine is filtered out by design).
        let ranked = exact_cosine_ranking(&query, &corpus);
        let positive: Vec<String> = ranked
            .into_iter()
            .filter(|id| {
                corpus
                    .iter()
                    .find(|(candidate, _)| candidate == id)
                    .is_some_and(|(_, v)| cosine_similarity_naive(&query, v) > 0.05)
            })
            .collect();
        let worst: Vec<String> = positive.iter().rev().take(TOP_K).cloned().collect();
        let expected: Vec<String> = worst.iter().rev().cloned().collect();

        let scope: HashSet<String> = worst
            .iter()
            .map(|chunk| {
                let i = chunk.trim_start_matches("chunk");
                format!("doc{i}")
            })
            .collect();

        let hits =
            VectorSearchPort::search_scoped(&index, &query, TOP_K, 0.0, Some(&scope)).unwrap();
        assert_eq!(
            hits.iter().map(|h| h.chunk_id.as_str()).collect::<Vec<_>>(),
            expected.iter().map(String::as_str).collect::<Vec<_>>(),
            "scoped results must come back in exact-cosine order"
        );
        assert!(hits.iter().all(|h| scope.contains(&h.doc_id)));

        // Every score is the exact cosine, which is only possible if the
        // rescore ran on the widened pass that finally found these chunks.
        for hit in &hits {
            let (_, vector) = corpus
                .iter()
                .find(|(id, _)| *id == hit.chunk_id)
                .expect("hit must come from the corpus");
            assert!(
                (hit.score - cosine_similarity_naive(&query, vector)).abs() < 1e-5,
                "{} scored {} approximately",
                hit.chunk_id,
                hit.score
            );
        }
    }

    #[test]
    fn compressed_index_survives_a_round_trip_through_disk() {
        const DIM: usize = 16;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("compressed.usearch");
        let compression = i8_compression(8);

        let corpus: Vec<(String, Vec<f32>)> = (0..32u64)
            .map(|i| (format!("emb_{i}"), synthetic_vector(i, DIM, 0.8)))
            .collect();

        {
            let index = USearchVectorIndex::open_or_create_with_compression(
                DIM,
                path.clone(),
                compression.clone(),
            )
            .unwrap();
            index
                .publish_embeddings(
                    corpus
                        .iter()
                        .enumerate()
                        .map(|(i, (id, vector))| VectorIndexEntry {
                            id: id.clone(),
                            embedding: vector.clone(),
                            content: format!("Passage {i}"),
                            chunk_id: format!("chunk_{i}"),
                            document_id: "book".into(),
                        })
                        .collect(),
                )
                .unwrap();
        }

        assert!(
            super::super::rescore_store::vectors_path_for(&path).exists(),
            "full vectors must be persisted for rescoring after a restart"
        );

        let reloaded =
            USearchVectorIndex::open_or_create_with_compression(DIM, path, compression).unwrap();
        assert_eq!(reloaded.count(), 32);

        let query = synthetic_vector(4_242, DIM, 0.8);
        let expected: Vec<String> = exact_cosine_ranking(&query, &corpus)
            .into_iter()
            .take(3)
            .map(|id| id.replace("emb_", "chunk_"))
            .collect();
        let hits = VectorSearchPort::search(&reloaded, &query, 3, 0.0).unwrap();
        assert_eq!(
            hits.iter().map(|h| h.chunk_id.as_str()).collect::<Vec<_>>(),
            expected.iter().map(String::as_str).collect::<Vec<_>>(),
            "rescoring must still work against the reloaded side store"
        );
        assert!(hits.iter().all(|h| h.chunk_id.starts_with("chunk_")));
    }

    /// A deleted chunk must not be resurrected by a stale full-precision copy.
    #[test]
    fn removal_and_clear_drop_the_full_precision_copies() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("removal.usearch");
        let index = USearchVectorIndex::with_compression(4, Some(path), i8_compression(2)).unwrap();

        index
            .add_embedding("a".into(), unit(&[1.0, 0.0, 0.0, 0.0]))
            .unwrap();
        index
            .add_embedding("b".into(), unit(&[0.8, 0.6, 1.0, 0.0]))
            .unwrap();

        let key_a = *index.state.read().id_to_key.get("a").unwrap();
        let store = index.rescore.as_ref().unwrap();
        assert!(store.get(key_a).is_some());

        index.remove_embedding("a").unwrap();
        assert!(store.get(key_a).is_none(), "stale vector left behind");

        let key_b = *index.state.read().id_to_key.get("b").unwrap();
        index.clear().unwrap();
        assert!(
            store.get(key_b).is_none(),
            "clear must empty the side store"
        );
    }

    /// Losing the side store degrades to approximate scoring rather than
    /// failing or dropping results — an index rebuild restores exactness.
    #[test]
    fn a_missing_full_vector_falls_back_to_the_approximate_score() {
        let index = USearchVectorIndex::with_compression(4, None, i8_compression(4)).unwrap();
        let vector = unit(&[1.0, 0.0, 0.0, 0.0]);
        index.add_embedding("a".into(), vector.clone()).unwrap();

        let key = *index.state.read().id_to_key.get("a").unwrap();
        index.rescore.as_ref().unwrap().remove(key).unwrap();

        let hits = VectorSearchPort::search(&index, &vector, 1, 0.0).unwrap();
        assert_eq!(hits.len(), 1);
        assert!(hits[0].score > 0.99, "score was {}", hits[0].score);
    }

    /// The predicate runs inside the graph traversal now, so the properties
    /// that used to fall out of post-filtering have to be pinned explicitly:
    /// nothing out of scope survives, and a scope with enough members still
    /// yields a full `top_k`.
    #[test]
    fn a_scoped_search_returns_only_allowed_documents_and_still_fills_top_k() {
        scoped_search_fills_top_k_with_the_best_in_scope_chunks(EXACT_SEARCH_MAX_VECTORS);
    }

    /// The same contract past the exact-scan size, where the scope runs as a
    /// predicate inside the graph traversal instead.
    #[test]
    fn a_scoped_graph_search_returns_only_allowed_documents_and_still_fills_top_k() {
        scoped_search_fills_top_k_with_the_best_in_scope_chunks(0);
    }

    fn scoped_search_fills_top_k_with_the_best_in_scope_chunks(exact_search_max: usize) {
        const DIM: usize = 16;
        const TOP_K: usize = 10;
        let corpus = widening_corpus(DIM, 512);
        let mut index = widening_index(DIM, &corpus);
        index.exact_search_max = exact_search_max;
        let query = synthetic_vector(2_024, DIM, 0.8);

        // Every fifth document, so the scope is both large enough to fill
        // `top_k` and selective enough that post-filtering would have had to
        // widen.
        let scope: HashSet<String> = (0..512)
            .filter(|i| i % 5 == 0)
            .map(|i| format!("doc{i}"))
            .collect();

        let hits =
            VectorSearchPort::search_scoped(&index, &query, TOP_K, 0.0, Some(&scope)).unwrap();

        assert_eq!(hits.len(), TOP_K, "a satisfiable scope must fill top_k");
        assert!(
            hits.iter().all(|h| scope.contains(&h.doc_id)),
            "an out-of-scope document survived the predicate: {hits:?}"
        );

        // …and they are the best in-scope chunks, not merely in-scope ones.
        let mut in_scope: Vec<(String, f32)> = corpus
            .iter()
            .filter(|(id, _)| scope.contains(&format!("doc{}", id.trim_start_matches("chunk"))))
            .map(|(id, v)| (id.clone(), cosine_similarity_naive(&query, v)))
            .collect();
        in_scope.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(CmpOrdering::Equal));
        let expected: Vec<&str> = in_scope
            .iter()
            .take(TOP_K)
            .map(|(id, _)| id.as_str())
            .collect();
        assert_eq!(
            hits.iter().map(|h| h.chunk_id.as_str()).collect::<Vec<_>>(),
            expected
        );
    }

    /// A scope of one document out of five hundred: the first traversal will
    /// not find it, and the single exhaustive retry must.
    #[test]
    fn a_scope_of_one_distant_document_is_still_reached() {
        const DIM: usize = 16;
        let corpus = widening_corpus(DIM, 512);
        let index = widening_index(DIM, &corpus);
        let query = synthetic_vector(2_024, DIM, 0.8);

        // The worst-ranked chunk that still clears the bar. A negative cosine
        // is filtered out by the threshold, so scoping to one of those would
        // be testing the threshold rather than the traversal.
        let worst = exact_cosine_ranking(&query, &corpus)
            .into_iter()
            .rfind(|id| {
                corpus
                    .iter()
                    .find(|(candidate, _)| candidate == id)
                    .is_some_and(|(_, v)| cosine_similarity_naive(&query, v) > 0.05)
            })
            .expect("some chunk clears the bar");
        let doc = format!("doc{}", worst.trim_start_matches("chunk"));
        let scope: HashSet<String> = [doc.clone()].into();

        let hits = VectorSearchPort::search_scoped(&index, &query, 5, 0.0, Some(&scope)).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].doc_id, doc);
        assert_eq!(hits[0].chunk_id, worst);
    }

    #[test]
    fn an_empty_scope_returns_nothing_without_searching() {
        let index = widening_index(16, &widening_corpus(16, 32));
        let scope: HashSet<String> = HashSet::new();
        assert!(VectorSearchPort::search_scoped(
            &index,
            &synthetic_vector(1, 16, 0.8),
            5,
            0.0,
            Some(&scope)
        )
        .unwrap()
        .is_empty());
    }

    fn widening_corpus(dim: usize, count: u64) -> Vec<(String, Vec<f32>)> {
        (0..count)
            .map(|i| (format!("chunk{i}"), synthetic_vector(i, dim, 0.8)))
            .collect()
    }

    /// Forced onto the graph path: these tests pin the predicate and the
    /// exhaustive retry, which an exact scan never needs.
    fn widening_index(dim: usize, corpus: &[(String, Vec<f32>)]) -> USearchVectorIndex {
        let mut index = USearchVectorIndex::new(dim, None).unwrap();
        index.exact_search_max = 0;
        for (i, (id, vector)) in corpus.iter().enumerate() {
            index
                .add_embedding_with_content(
                    id.clone(),
                    vector.clone(),
                    format!("passage {i}"),
                    id.clone(),
                    format!("doc{i}"),
                )
                .unwrap();
        }
        index
    }

    /// The logged waste: a scoped search whose scope was satisfiable but whose
    /// threshold nothing could clear widened nine times across the whole index
    /// before returning the empty list its first pass had already established.
    /// The answer must stay empty — this pins the result, while the widening
    /// loop now gives up once the nearest in-scope vector misses the bar.
    #[test]
    fn an_unreachable_threshold_still_returns_nothing() {
        const DIM: usize = 16;
        let corpus = widening_corpus(DIM, 128);
        let index = widening_index(DIM, &corpus);
        let query = synthetic_vector(7_777, DIM, 0.8);
        let scope: HashSet<String> = (0..128).map(|i| format!("doc{i}")).collect();

        let hits =
            VectorSearchPort::search_scoped(&index, &query, 10, 0.999_9, Some(&scope)).unwrap();

        assert!(hits.is_empty());
    }

    /// The property the early stop must not break. Here the scope — not the
    /// threshold — is what empties the first candidate windows, so widening is
    /// the only way to reach the answer and it has to keep going.
    #[test]
    fn a_scope_that_only_distant_documents_satisfy_is_still_reached() {
        const DIM: usize = 16;
        let corpus = widening_corpus(DIM, 128);
        let index = widening_index(DIM, &corpus);
        let query = synthetic_vector(7_777, DIM, 0.8);

        // Scope to the single worst-ranked document that still clears the bar,
        // so every early candidate window is filtered away entirely.
        let ranked = exact_cosine_ranking(&query, &corpus);
        let worst = ranked
            .iter()
            .rev()
            .find(|id| {
                corpus
                    .iter()
                    .find(|(candidate, _)| candidate == *id)
                    .is_some_and(|(_, v)| cosine_similarity_naive(&query, v) > 0.05)
            })
            .expect("some chunk clears the bar")
            .clone();
        let doc = format!("doc{}", worst.trim_start_matches("chunk"));
        let scope: HashSet<String> = [doc.clone()].into();

        let hits = VectorSearchPort::search_scoped(&index, &query, 3, 0.05, Some(&scope)).unwrap();

        assert_eq!(hits.len(), 1, "widening must still reach a distant scope");
        assert_eq!(hits[0].doc_id, doc);
    }
}

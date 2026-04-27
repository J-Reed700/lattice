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

use crate::features::search::dto::SearchResultPortDto;
use crate::application::ports::VectorSearchPort;
use crate::infrastructure::search::service::SearchResult;
use crate::features::search::SearchServiceTrait;
use crate::shared::error::AppError;
use crate::shared::result::Result;
use async_trait::async_trait;
use parking_lot::RwLock;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use usearch::{Index, IndexOptions, MetricKind, ScalarKind};

/// Metadata stored alongside each vector for search result enrichment.
/// Kept in a side map (not in USearch) so search results include content.
#[derive(Clone, Debug)]
struct VectorMeta {
    chunk_id: String,
    document_id: String,
    content: String,
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

    /// Expected embedding dimension
    dimension: usize,

    /// Path for persisting the index to disk
    index_path: Option<PathBuf>,

    /// Path for persisting the key map to disk
    keymap_path: Option<PathBuf>,
}

/// Serializable key map for persistence alongside the USearch index file.
#[derive(serde::Serialize, serde::Deserialize)]
struct KeyMapData {
    id_to_key: HashMap<String, u64>,
    next_key: u64,
    metadata: HashMap<String, SerializableMeta>,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct SerializableMeta {
    chunk_id: String,
    document_id: String,
    content: String,
}

impl USearchVectorIndex {
    /// Create a new USearch vector index.
    ///
    /// # Arguments
    /// * `dimension` - Expected embedding dimension (e.g., 768)
    /// * `index_path` - Optional path for index persistence
    pub fn new(dimension: usize, index_path: Option<PathBuf>) -> Result<Self> {
        let options = IndexOptions {
            dimensions: dimension,
            metric: MetricKind::Cos,
            quantization: ScalarKind::F32,
            connectivity: 16,
            expansion_add: 128,
            expansion_search: 64,
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

        Ok(Self {
            index,
            state: RwLock::new(KeyState {
                id_to_key: HashMap::new(),
                key_to_id: HashMap::new(),
                metadata: HashMap::new(),
            }),
            next_key: AtomicU64::new(1),
            dimension,
            index_path,
            keymap_path,
        })
    }

    /// Load an existing index from disk, or create a new one if it doesn't exist.
    pub fn open_or_create(dimension: usize, index_path: PathBuf) -> Result<Self> {
        let instance = Self::new(dimension, Some(index_path.clone()))?;

        if index_path.exists() {
            // Load the USearch index from disk
            let path_str = index_path.to_string_lossy().to_string();
            instance.index.load(&path_str).map_err(|e| {
                AppError::InternalError(format!("Failed to load USearch index from disk: {}", e))
            })?;

            // Load the key map
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
            // Reserve initial capacity
            instance.index.reserve(10_000).map_err(|e| {
                AppError::InternalError(format!("Failed to reserve USearch capacity: {}", e))
            })?;

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
        // Clear existing data
        self.index.reset().map_err(|e| {
            AppError::InternalError(format!("Failed to reset USearch index: {}", e))
        })?;
        {
            let mut state = self.state.write();
            state.id_to_key.clear();
            state.key_to_id.clear();
            state.metadata.clear();
        }
        self.next_key.store(1, Ordering::SeqCst);

        // Reserve capacity
        let count = embeddings.len();
        if count > 0 {
            self.index.reserve(count).map_err(|e| {
                AppError::InternalError(format!("Failed to reserve USearch capacity: {}", e))
            })?;
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
            return Err(AppError::InvalidInput(format!(
                "Embedding with id '{}' already exists",
                id
            )));
        }

        // Allocate a new u64 key
        let key = self.next_key.fetch_add(1, Ordering::SeqCst);

        // Add to USearch index
        self.index.add(key, &embedding).map_err(|e| {
            AppError::InternalError(format!("Failed to add embedding to USearch: {}", e))
        })?;

        // Update key maps (same lock — cannot desync)
        state.id_to_key.insert(id.clone(), key);
        state.key_to_id.insert(key, id.clone());

        // Store metadata if provided
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
    pub fn save_to_disk(&self) -> Result<()> {
        if let Some(ref path) = self.index_path {
            // Ensure parent directory exists
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).map_err(|e| {
                    AppError::FileStorage(format!("Failed to create index directory: {}", e))
                })?;
            }

            let path_str = path.to_string_lossy().to_string();
            self.index.save(&path_str).map_err(|e| {
                AppError::FileStorage(format!("Failed to save USearch index: {}", e))
            })?;

            tracing::debug!(path = %path.display(), "USearch index saved to disk");
        }

        if let Some(ref path) = self.keymap_path {
            self.save_keymap(path)?;
            tracing::debug!(path = %path.display(), "Key map saved to disk");
        }

        Ok(())
    }

    /// Save the key map to a JSON file (atomic via temp file + rename).
    fn save_keymap(&self, path: &Path) -> Result<()> {
        let state = self.state.read();

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
                            content: m.content.clone(),
                        },
                    )
                })
                .collect(),
        };

        // Atomic write via temp file + rename
        let temp_path = path.with_extension("tmp");
        let file = std::fs::File::create(&temp_path).map_err(|e| {
            AppError::FileStorage(format!("Failed to create temp key map file: {}", e))
        })?;
        let writer = std::io::BufWriter::new(file);
        serde_json::to_writer(writer, &data)
            .map_err(|e| AppError::Serialization(format!("Failed to serialize key map: {}", e)))?;
        std::fs::rename(&temp_path, path)
            .map_err(|e| AppError::FileStorage(format!("Failed to rename key map file: {}", e)))?;

        Ok(())
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
                    content: m.content,
                },
            );
        }

        self.next_key.store(data.next_key, Ordering::SeqCst);

        Ok(())
    }
}

/// Flush the USearch index to disk on drop.
///
/// This ensures that any embeddings added during the application's lifetime
/// are persisted even if `save_to_disk()` wasn't explicitly called.
impl Drop for USearchVectorIndex {
    fn drop(&mut self) {
        if self.index_path.is_some() {
            if let Err(e) = self.save_to_disk() {
                // Cannot propagate errors from Drop, so log at error level
                tracing::error!(error = %e, "Failed to flush USearch index to disk during drop");
            } else {
                tracing::debug!("USearch index flushed to disk during drop");
            }
        }
    }
}

// =============================================================================
// VectorSearchPort implementation (application layer)
// =============================================================================

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

        if self.index.size() == 0 {
            return Ok(Vec::new());
        }
        if matches!(allowed_document_ids, Some(scope) if scope.is_empty()) {
            return Ok(Vec::new());
        }

        let index_size = self.index.size();
        let mut candidate_k = top_k.min(index_size);
        if allowed_document_ids.is_some() {
            let widened_start = top_k.saturating_mul(4).max(32).min(index_size);
            candidate_k = candidate_k.max(widened_start);
        }

        loop {
            let matches = self
                .index
                .search(query_embedding, candidate_k)
                .map_err(|e| AppError::InternalError(format!("USearch search failed: {}", e)))?;

            let state = self.state.read();
            let mut results = Vec::new();
            for (&key, &distance) in matches.keys.iter().zip(matches.distances.iter()) {
                // USearch cosine distance = 1.0 - cosine_similarity
                let similarity = 1.0 - distance;
                if similarity < threshold {
                    continue;
                }

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
                if let Some(scope) = allowed_document_ids {
                    if !scope.contains(doc_id) {
                        continue;
                    }
                }

                results.push(SearchResultPortDto {
                    doc_id: doc_id.to_string(),
                    chunk_id: chunk_id.to_string(),
                    score: similarity,
                    content: content.to_string(),
                });
                if results.len() >= top_k {
                    break;
                }
            }

            let exhausted = candidate_k >= index_size;
            if results.len() >= top_k || exhausted {
                if results.len() > top_k {
                    results.truncate(top_k);
                }
                return Ok(results);
            }

            let next_candidate_k = candidate_k.saturating_mul(2).min(index_size);
            if next_candidate_k == candidate_k {
                return Ok(results);
            }
            tracing::debug!(
                current_candidate_k = candidate_k,
                next_candidate_k = next_candidate_k,
                top_k = top_k,
                "USearch scoped search widening candidate window"
            );
            candidate_k = next_candidate_k;
        }
    }

    fn add_embedding(&self, id: String, embedding: Vec<f32>) -> Result<()> {
        self.add_internal(id, embedding, None, None, None)?;
        // Persist after each mutation to prevent data loss
        self.save_to_disk()
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
        // Persist after each mutation to prevent data loss
        self.save_to_disk()
    }

    fn remove_embedding(&self, id: &str) -> Result<()> {
        let key = {
            let state = self.state.read();
            match state.id_to_key.get(id) {
                Some(&k) => k,
                None => return Ok(()), // Not found is a no-op
            }
        };

        // Remove from USearch
        self.index.remove(key).map_err(|e| {
            AppError::InternalError(format!("Failed to remove embedding from USearch: {}", e))
        })?;

        // Remove from key maps and metadata under a single lock
        {
            let mut state = self.state.write();
            state.id_to_key.remove(id);
            state.key_to_id.remove(&key);
            state.metadata.remove(id);
        }

        // Persist after mutation
        self.save_to_disk()
    }

    fn clear(&self) -> Result<()> {
        self.index.reset().map_err(|e| {
            AppError::InternalError(format!("Failed to reset USearch index: {}", e))
        })?;
        {
            let mut state = self.state.write();
            state.id_to_key.clear();
            state.key_to_id.clear();
            state.metadata.clear();
        }
        self.next_key.store(1, Ordering::SeqCst);
        Ok(())
    }

    fn count(&self) -> usize {
        // Use key map length for accuracy (USearch size() may include tombstones)
        self.state.read().id_to_key.len()
    }

    fn dimension(&self) -> usize {
        self.dimension
    }
}

// =============================================================================
// SearchServiceTrait implementation (infrastructure layer)
// Unifies the two parallel pipelines into one.
// =============================================================================

#[async_trait]
impl SearchServiceTrait for USearchVectorIndex {
    fn search(&self, query_embedding: &[f32], top_k: usize) -> Result<Vec<SearchResult>> {
        if self.index.size() == 0 {
            return Ok(Vec::new());
        }

        let start = std::time::Instant::now();

        let matches = self
            .index
            .search(query_embedding, top_k)
            .map_err(|e| AppError::InternalError(format!("USearch search failed: {}", e)))?;

        let state = self.state.read();

        let results: Vec<SearchResult> = matches
            .keys
            .iter()
            .zip(matches.distances.iter())
            .enumerate()
            .filter_map(|(idx, (&key, &distance))| {
                let similarity = 1.0 - distance;
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

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_index(dim: usize) -> USearchVectorIndex {
        USearchVectorIndex::new(dim, None).unwrap_or_else(|e| {
            panic!("Failed to create test index: {}", e);
        })
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

        // Create and populate
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
        assert_eq!(results[0].content, "test content");
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

        // Verify maps are consistent
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
}

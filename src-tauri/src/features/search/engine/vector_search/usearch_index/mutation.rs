//! Inserting vectors, growing capacity and rebuilding the index in batches.
use super::{USearchVectorIndex, VectorMeta, INITIAL_INDEX_CAPACITY};
use crate::application::ports::VectorSearchPort;
use crate::shared::error::{AppError, Result};
use std::sync::atomic::Ordering;

impl USearchVectorIndex {
    /// Rebuild the index from SQLite embeddings.
    /// Called when the index file is missing/corrupt or after model changes.
    #[must_use = "returns the number of embeddings added"]
    pub fn rebuild_from_embeddings(
        &self,
        embeddings: Vec<(String, Vec<f32>, String, String, String)>,
    ) -> Result<usize> {
        self.begin_rebuild()?;
        let added = self.add_rebuild_batch(embeddings)?;
        self.finish_rebuild()?;
        Ok(added)
    }

    /// Start a rebuild before streaming bounded pages from SQLite.
    pub fn begin_rebuild(&self) -> Result<()> {
        self.clear()
    }

    /// Insert one bounded restore page without saving the whole graph.
    pub fn add_rebuild_batch(
        &self,
        embeddings: Vec<(String, Vec<f32>, String, String, String)>,
    ) -> Result<usize> {
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

        Ok(added)
    }

    /// Persist the completed streamed rebuild once, after all rows validate.
    pub fn finish_rebuild(&self) -> Result<()> {
        self.save_to_disk()
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
    pub(super) fn add_internal(
        &self,
        id: String,
        embedding: Vec<f32>,
        content: Option<String>,
        chunk_id: Option<String>,
        document_id: Option<String>,
    ) -> Result<()> {
        self.insert_internal(id, embedding, content, chunk_id, document_id, false)
    }

    pub(super) fn insert_internal(
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

        if let (Some(_content), Some(chunk_id), Some(doc_id)) = (content, chunk_id, document_id) {
            state
                .document_to_keys
                .entry(doc_id.clone())
                .or_default()
                .insert(key);
            state.metadata.insert(
                id,
                VectorMeta {
                    chunk_id,
                    document_id: doc_id,
                },
            );
        } else {
            // Legacy bare-vector callers identify the document with the vector
            // id. Keep their scope lookup indexed as well.
            state.document_to_keys.entry(id).or_default().insert(key);
        }

        Ok(())
    }
}

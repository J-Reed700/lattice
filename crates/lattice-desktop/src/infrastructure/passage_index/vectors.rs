//! The vectors file beside a passage store, keyed by embedding identity.
//!
//! One `vectors-<identity>.usearch` per directory, made by whichever
//! embedding model is active; a vector's key is its passage id and its
//! document is the passage's document key, so a search confined to some
//! documents is scoped inside USearch. A different identity than the one the
//! store records drops every vectors file and marks every document for
//! embedding again, and so does a vectors file that is missing or will not
//! load. Passages stay: only the vectors depend on the model.

use super::store::{PassageStore, META_IDENTITY};
use crate::infrastructure::vector_index::USearchVectorIndex;
use lattice_core::application::ports::vector_search_port::{VectorIndexEntry, VectorSearchPort};
use lattice_core::shared::{AppError, Result};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::{info, warn};

pub(super) const PREFIX: &str = "vectors-";

/// The vectors file for one embedding identity in `dir`.
pub fn vectors_path(dir: &Path, identity: &str) -> PathBuf {
    dir.join(format!(
        "{PREFIX}{}.usearch",
        identity.replace([':', '/'], "-")
    ))
}

/// Removes every vectors file (and its key map and manifest) in `dir`.
fn remove_vector_files(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.filter_map(|entry| entry.ok()) {
        if entry.file_name().to_string_lossy().starts_with(PREFIX) {
            let path = entry.path();
            if let Err(error) = lattice_core::shared::fs::removal::remove_file(&path) {
                tracing::warn!(path = %path.display(), %error, "Passage index: could not remove a vectors file");
            }
        }
    }
}

/// The open vectors file of one index. Cheap to clone: clones share it.
#[derive(Clone)]
pub struct Vectors {
    index: Arc<USearchVectorIndex>,
}

impl Vectors {
    /// The vectors file for `identity` beside `store`, after the identity
    /// rule: anything made by another model, or unreadable, is dropped and
    /// every document with passages is marked for embedding.
    pub async fn open(store: &PassageStore, identity: &str, dimension: usize) -> Result<Self> {
        let dir = store.dir().to_path_buf();
        let stored = store.meta(META_IDENTITY).await?;
        let path = vectors_path(&dir, identity);
        let keymap = path.with_extension("keymap.json");
        if stored.as_deref() != Some(identity) || !path.exists() || !keymap.exists() {
            if stored.is_some() {
                info!(
                    dir = %dir.display(),
                    from = stored.as_deref().unwrap_or_default(),
                    to = %identity,
                    "Passage index: embedding model changed; embedding again"
                );
            }
            remove_vector_files(&dir);
            store.reset_embedded().await?;
            store.set_meta(META_IDENTITY, identity).await?;
        }
        let load_path = path.clone();
        let loaded = tokio::task::spawn_blocking(move || {
            USearchVectorIndex::open_or_create(dimension, load_path)
        })
        .await
        .map_err(|error| AppError::InternalError(format!("Loading vectors failed: {error}")))?;
        let index = match loaded {
            Ok(index) => index,
            Err(error) => {
                warn!(%error, dir = %dir.display(), "Passage index: vectors file unreadable; embedding again");
                remove_vector_files(&dir);
                store.reset_embedded().await?;
                USearchVectorIndex::open_or_create(dimension, path)?
            }
        };
        Ok(Self {
            index: Arc::new(index.with_coalesced_saves()),
        })
    }

    /// The vectors as the search port the hybrid search's dense branch
    /// queries.
    pub fn port(&self) -> Arc<dyn VectorSearchPort> {
        Arc::clone(&self.index) as Arc<dyn VectorSearchPort>
    }

    /// Publishes `(passage id, document key, embedding)` triples to the
    /// in-memory index; they reach the file with the next [`Self::save`].
    pub fn add(&self, vectors: Vec<(i64, String, Vec<f32>)>) -> Result<()> {
        let entries = vectors
            .into_iter()
            .map(|(id, document, embedding)| VectorIndexEntry {
                id: id.to_string(),
                embedding,
                content: String::new(),
                chunk_id: id.to_string(),
                document_id: document,
            })
            .collect();
        self.index.publish_embeddings(entries)
    }

    /// Takes the vectors of these passages out of the in-memory index. Until
    /// the next save a stale vector on disk is harmless: a search keeps only
    /// hits whose passage row still exists.
    pub fn remove(&self, ids: &[i64]) -> Result<()> {
        if ids.is_empty() {
            return Ok(());
        }
        let ids: Vec<String> = ids.iter().map(i64::to_string).collect();
        self.index.remove_embeddings(&ids)
    }

    /// Writes the vectors file when anything changed since the last save.
    pub async fn save(&self) -> Result<()> {
        let index = Arc::clone(&self.index);
        tokio::task::spawn_blocking(move || {
            if index.is_dirty() {
                index.save_to_disk()
            } else {
                Ok(())
            }
        })
        .await
        .map_err(|error| AppError::InternalError(format!("Saving vectors failed: {error}")))?
    }

    pub fn count(&self) -> usize {
        self.index.count()
    }
}

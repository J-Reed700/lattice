//! Index directories in use: a build, a search, a watcher or all three
//! share one open store and one loaded vectors file, and take turns writing.
//!
//! [`LiveIndexes`] hands each directory out once and counts its users; the
//! last to let go closes the store's pool. It is a value its owner keeps (a
//! feature's index manager), never global state.

use super::store::PassageStore;
use super::vectors::Vectors;
use async_trait::async_trait;
use lattice_core::application::ports::EmbeddingPort;
use lattice_core::shared::Result;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Where an index gets its embedder. The app resolves it through the
/// container, so the model is loaded once and every index and the chat path
/// share it.
#[async_trait]
pub trait EmbedderSource: Send + Sync {
    /// The active embedding model, loaded; `None` when there is none.
    async fn embedder(&self) -> Option<Arc<dyn EmbeddingPort>>;
}

/// What a search or a build needs once the embedder has loaded.
#[derive(Clone)]
pub struct Engine {
    pub vectors: Vectors,
    pub embedder: Arc<dyn EmbeddingPort>,
}

/// An index directory in use. Its users share the pool and the vectors.
pub struct LiveIndex {
    pub dir: PathBuf,
    pub store: Arc<PassageStore>,
    engine: RwLock<Option<Engine>>,
    /// Held by whatever writes: a build for its whole run, a watcher for one
    /// update.
    pub writing: tokio::sync::Mutex<()>,
    /// For an owner whose rows may change while it embeds (a course's): held
    /// while pending passages are embedded, so two runs never embed the same
    /// passages, while `writing` is held only for the row changes. An owner
    /// that holds `writing` for a whole run needs neither apart.
    pub embedding: tokio::sync::Mutex<()>,
}

impl LiveIndex {
    pub fn new(dir: PathBuf, store: Arc<PassageStore>) -> Self {
        Self {
            dir,
            store,
            engine: RwLock::new(None),
            writing: tokio::sync::Mutex::new(()),
            embedding: tokio::sync::Mutex::new(()),
        }
    }

    /// The vectors and embedder last prepared, if any.
    pub fn engine(&self) -> Option<Engine> {
        self.engine.read().clone()
    }

    /// The vectors for `embedder`, loaded once while the index is in use and
    /// again when the model changes. Call while holding `writing`: a model
    /// change rewrites the store's embedded flags.
    pub async fn prepare(&self, embedder: Arc<dyn EmbeddingPort>) -> Result<Engine> {
        if let Some(engine) = self
            .engine()
            .filter(|engine| engine.embedder.model_identity() == embedder.model_identity())
        {
            return Ok(engine);
        }
        let vectors = Vectors::open(
            &self.store,
            &embedder.model_identity(),
            embedder.dimension(),
        )
        .await?;
        let engine = Engine { vectors, embedder };
        *self.engine.write() = Some(engine.clone());
        Ok(engine)
    }
}

/// An index directory in use, and by how many.
struct LiveEntry {
    index: Arc<LiveIndex>,
    users: usize,
}

/// The directories in use, each opened once however many use it.
#[derive(Default)]
pub struct LiveIndexes {
    live: tokio::sync::Mutex<HashMap<PathBuf, LiveEntry>>,
}

impl LiveIndexes {
    /// Opens the index in `dir` for one more user, sharing it with any that
    /// already has it.
    pub async fn acquire(&self, dir: &Path) -> Result<Arc<LiveIndex>> {
        let mut live = self.live.lock().await;
        if let Some(entry) = live.get_mut(dir) {
            entry.users += 1;
            return Ok(Arc::clone(&entry.index));
        }
        let store = Arc::new(PassageStore::open(dir).await?);
        let index = Arc::new(LiveIndex::new(dir.to_path_buf(), store));
        live.insert(
            dir.to_path_buf(),
            LiveEntry {
                index: Arc::clone(&index),
                users: 1,
            },
        );
        Ok(index)
    }

    /// Gives back one user's hold; the last closes the store's pool.
    pub async fn release(&self, index: &LiveIndex) {
        let closing = {
            let mut live = self.live.lock().await;
            match live.get_mut(&index.dir) {
                Some(entry) if entry.users > 1 => {
                    entry.users -= 1;
                    None
                }
                Some(_) => live.remove(&index.dir),
                None => None,
            }
        };
        if let Some(entry) = closing {
            entry.index.store.close().await;
        }
    }
}

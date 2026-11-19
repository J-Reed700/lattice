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

    /// The active model's identity, without loading it where the source can
    /// tell (a status read must not load a model); `None` when there is no
    /// active model. A source that cannot tell answers from the loaded model.
    async fn identity(&self) -> Option<String> {
        self.embedder()
            .await
            .map(|embedder| embedder.model_identity())
    }
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

type ClosingSlot = tokio::sync::watch::Receiver<bool>;

/// Keeps a directory marked as closing until its store and vectors drop.
///
/// Dropping this registration clears the directory from the closing map and
/// wakes any [`LiveIndexes::acquire`] or [`LiveIndexes::evict_and_close`] waiting
/// on it.
pub struct CloseRegistration {
    dir: PathBuf,
    closing: Arc<parking_lot::Mutex<HashMap<PathBuf, ClosingSlot>>>,
    tx: Option<tokio::sync::watch::Sender<bool>>,
}

impl Drop for CloseRegistration {
    fn drop(&mut self) {
        {
            let mut map = self.closing.lock();
            if let Some(current) = map.get(&self.dir) {
                if self
                    .tx
                    .as_ref()
                    .is_some_and(|tx| current.same_channel(&tx.subscribe()))
                {
                    map.remove(&self.dir);
                }
            }
        }
        if let Some(tx) = self.tx.take() {
            let _ = tx.send(true);
        }
    }
}

/// An RAII hold on a [`LiveIndex`] obtained from [`LiveIndexes`].
///
/// Calling [`Self::release`] awaits closing the store's pool when this was the
/// last user. Dropping without calling `release` (for example on an early `?`
/// or a cancelled future) still decrements the refcount and evicts the entry
/// synchronously from [`LiveIndexes`].
pub struct LiveLease<'a> {
    live: &'a LiveIndexes,
    index: Option<Arc<LiveIndex>>,
}

impl<'a> LiveLease<'a> {
    pub fn new(live: &'a LiveIndexes, index: Arc<LiveIndex>) -> Self {
        Self {
            live,
            index: Some(index),
        }
    }

    pub async fn release(mut self) {
        if let Some(index) = self.index.take() {
            self.live.release(&index).await;
        }
    }

    pub fn disarm(mut self) {
        self.index = None;
    }
}

impl Drop for LiveLease<'_> {
    fn drop(&mut self) {
        if let Some(index) = self.index.take() {
            let _ = self.live.release_sync(&index);
        }
    }
}

/// The directories in use, each opened once however many use it.
#[derive(Default)]
pub struct LiveIndexes {
    live: parking_lot::Mutex<HashMap<PathBuf, LiveEntry>>,
    closing: Arc<parking_lot::Mutex<HashMap<PathBuf, ClosingSlot>>>,
    opening: tokio::sync::Mutex<()>,
}

impl LiveIndexes {
    /// Opens the index in `dir` for one more user, sharing it with any that
    /// already has it.
    pub async fn acquire(&self, dir: &Path) -> Result<Arc<LiveIndex>> {
        {
            let mut live = self.live.lock();
            if let Some(entry) = live.get_mut(dir) {
                entry.users += 1;
                return Ok(Arc::clone(&entry.index));
            }
        }
        let _opening = self.opening.lock().await;
        {
            let mut live = self.live.lock();
            if let Some(entry) = live.get_mut(dir) {
                entry.users += 1;
                return Ok(Arc::clone(&entry.index));
            }
        }
        self.wait_closing(dir).await;
        let store = Arc::new(PassageStore::open(dir).await?);
        let index = Arc::new(LiveIndex::new(dir.to_path_buf(), store));
        self.live.lock().insert(
            dir.to_path_buf(),
            LiveEntry {
                index: Arc::clone(&index),
                users: 1,
            },
        );
        Ok(index)
    }

    /// Wraps an acquired [`LiveIndex`] in an RAII lease guard.
    pub fn lease(&self, index: Arc<LiveIndex>) -> LiveLease<'_> {
        LiveLease::new(self, index)
    }

    /// Gives back one user's hold synchronously; when `users` hits zero, evicts
    /// the entry from the live map and returns its [`Arc<LiveIndex>`].
    pub fn release_sync(&self, index: &LiveIndex) -> Option<Arc<LiveIndex>> {
        let mut live = self.live.lock();
        match live.get_mut(&index.dir) {
            Some(entry) if entry.users > 1 => {
                entry.users -= 1;
                None
            }
            Some(_) => live.remove(&index.dir).map(|entry| entry.index),
            None => None,
        }
    }

    /// Gives back one user's hold synchronously, registering any final close so
    /// [`Self::evict_and_close`] and [`Self::acquire`] wait until the store
    /// finishes closing.
    pub fn release_for_close(
        &self,
        index: &LiveIndex,
    ) -> Option<(Arc<LiveIndex>, CloseRegistration)> {
        let mut live = self.live.lock();
        let evicted = match live.get_mut(&index.dir) {
            Some(entry) if entry.users > 1 => {
                entry.users -= 1;
                return None;
            }
            Some(_) => live.remove(&index.dir).map(|entry| entry.index)?,
            None => return None,
        };
        let (tx, rx) = tokio::sync::watch::channel(false);
        self.closing.lock().insert(index.dir.clone(), rx);
        Some((
            evicted,
            CloseRegistration {
                dir: index.dir.clone(),
                closing: Arc::clone(&self.closing),
                tx: Some(tx),
            },
        ))
    }

    /// Gives back one user's hold; the last closes the store's pool.
    pub async fn release(&self, index: &LiveIndex) {
        if let Some((closing, _reg)) = self.release_for_close(index) {
            closing.store.close().await;
            drop(closing);
        }
    }

    /// Evicts `dir` from the live map (closing its store if present) and waits
    /// for any in-flight background close of `dir` to finish.
    pub async fn evict_and_close(&self, dir: &Path) {
        let evicted = self.live.lock().remove(dir).map(|entry| entry.index);
        if let Some(index) = evicted {
            index.store.close().await;
            drop(index);
        }
        self.wait_closing(dir).await;
    }

    /// Drains all open index stores, closes their pools, and waits for any
    /// in-flight background closes to complete.
    pub async fn close_all(&self) {
        let entries: Vec<Arc<LiveIndex>> = self
            .live
            .lock()
            .drain()
            .map(|(_, entry)| entry.index)
            .collect();
        for index in entries {
            index.store.close().await;
            drop(index);
        }
        let closing_dirs: Vec<PathBuf> = self.closing.lock().keys().cloned().collect();
        for dir in closing_dirs {
            self.wait_closing(&dir).await;
        }
    }

    async fn wait_closing(&self, dir: &Path) {
        loop {
            let rx = self.closing.lock().get(dir).cloned();
            let Some(mut rx) = rx else {
                return;
            };
            if *rx.borrow() {
                return;
            }
            if rx.changed().await.is_err() {
                return;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn release_sync_evicts_entry_when_last_user_releases() {
        let temp = tempfile::tempdir().unwrap();
        let live = LiveIndexes::default();
        let first = live.acquire(temp.path()).await.unwrap();
        let second = live.acquire(temp.path()).await.unwrap();

        assert!(live.release_sync(&first).is_none());
        assert!(live.live.lock().contains_key(temp.path()));

        let closing = live
            .release_sync(&second)
            .expect("last user must evict entry synchronously");
        assert!(!live.live.lock().contains_key(temp.path()));
        closing.store.close().await;
    }

    #[tokio::test]
    async fn lease_drop_releases_entry_without_leaking() {
        let temp = tempfile::tempdir().unwrap();
        let live = LiveIndexes::default();
        {
            let index = live.acquire(temp.path()).await.unwrap();
            let _lease = live.lease(index);
            assert!(live.live.lock().contains_key(temp.path()));
        }
        assert!(!live.live.lock().contains_key(temp.path()));
    }

    #[tokio::test]
    async fn evict_and_close_closes_pool_and_waits_for_in_flight_close() {
        let temp = tempfile::tempdir().unwrap();
        let live = Arc::new(LiveIndexes::default());
        let index = live.acquire(temp.path()).await.unwrap();
        let (closing, reg) = live.release_for_close(&index).unwrap();

        let waiter = tokio::spawn({
            let live = Arc::clone(&live);
            let dir = temp.path().to_path_buf();
            async move {
                live.evict_and_close(&dir).await;
            }
        });
        tokio::task::yield_now().await;
        assert!(!waiter.is_finished());

        closing.store.close().await;
        drop(closing);
        drop(reg);
        waiter.await.unwrap();
        assert!(index.store.meta("anything").await.is_err());
    }

    #[tokio::test]
    async fn close_all_closes_multiple_open_stores() {
        let first_dir = tempfile::tempdir().unwrap();
        let second_dir = tempfile::tempdir().unwrap();
        let live = LiveIndexes::default();
        let first = live.acquire(first_dir.path()).await.unwrap();
        let second = live.acquire(second_dir.path()).await.unwrap();

        live.close_all().await;

        assert!(live.live.lock().is_empty());
        assert!(first.store.meta("anything").await.is_err());
        assert!(second.store.meta("anything").await.is_err());
    }
}

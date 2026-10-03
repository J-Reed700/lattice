//! The open folder's index: opening, closing, rebuilding, forgetting, and
//! the rules for which folders get an index at all.
//!
//! One folder is open at a time. Opening another closes the previous one's
//! watcher, task and SQLite pool; leaving the Explorer page does not.
//!
//! - The filesystem root, the home folder and any ancestor of home are
//!   refused: the Explorer still opens them, the index does not. So is a
//!   folder holding Lattice's data, whose index would watch its own writes.
//! - A folder with an index of its own uses it. One without reuses the
//!   deepest enclosing folder's index, if any: the walk and the watcher cover
//!   only the sub-tree, and search keeps to its prefix. An enclosing index
//!   that was too large holds nothing, so it is never reused.
//! - Nothing deletes an index unless the user asks: "Delete index" and
//!   "Remove" in the folders list, or a rebuild.
//! - The vectors file is keyed by the embedding identity. A different model
//!   drops the vectors and embeds again; chunks and hashes stay.

use super::dto::{
    FolderIndexState, FolderIndexStatusDto, FolderIndexSummaryDto, FolderIndexSummaryState,
};
use super::run::{Indexer, Limits, Outcome, StatusCell, StatusEmitter};
use super::search::FolderSearch;
use super::store::{self, Counts, FolderStore, IndexEntry, TooLarge};
use super::watcher::{self, Change};
use crate::application::ports::vector_search_port::VectorSearchPort;
use crate::application::ports::EmbeddingPort;
use crate::features::explorer::scope::Scope;
use crate::features::search::engine::vector_search::USearchVectorIndex;
use crate::shared::{AppError, Result};
use async_trait::async_trait;
use parking_lot::RwLock;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use std::time::Duration;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

/// Watcher batches past this many paths are cheaper as one full pass.
const RESCAN_PATHS: usize = 2_000;

/// Where the folder index gets its embedder. The app resolves it through the
/// container, so loading happens once and the chat path shares the model.
#[async_trait]
pub trait EmbedderSource: Send + Sync {
    /// The active embedding model, loaded; `None` when there is none.
    async fn embedder(&self) -> Option<Arc<dyn EmbeddingPort>>;
}

#[derive(Debug, Clone)]
pub struct ManagerConfig {
    /// `<data_dir>/folder-index`.
    pub base_dir: PathBuf,
    /// Canonical home folder; it and its ancestors are never indexed.
    pub home: Option<PathBuf>,
    pub limits: Limits,
    pub debounce: Duration,
}

impl ManagerConfig {
    pub fn for_data_dir(data_dir: &Path) -> Self {
        // Canonical, so a root that holds the data directory is recognised
        // whatever symlinks lead to it.
        let data_dir = std::fs::canonicalize(data_dir).unwrap_or_else(|_| data_dir.to_path_buf());
        Self {
            base_dir: data_dir.join("folder-index"),
            home: dirs::home_dir().and_then(|home| std::fs::canonicalize(home).ok()),
            limits: Limits::default(),
            debounce: Duration::from_millis(1_500),
        }
    }
}

/// What a search needs once the embedder has loaded.
#[derive(Clone)]
struct Engine {
    vectors: Arc<USearchVectorIndex>,
    embedder: Arc<dyn EmbeddingPort>,
}

struct OpenFolder {
    root: PathBuf,
    /// The index directory; `None` for a refused folder.
    dir: Option<PathBuf>,
    prefix: String,
    status: Arc<StatusCell>,
    store: Option<Arc<FolderStore>>,
    engine: Arc<RwLock<Option<Engine>>>,
    cancel: CancellationToken,
    task: Option<JoinHandle<()>>,
}

impl OpenFolder {
    /// Stops the task (it saves what it has embedded first) and closes the
    /// pool.
    async fn shut(mut self) {
        self.cancel.cancel();
        if let Some(task) = self.task.take() {
            let _ = task.await;
        }
        if let Some(store) = &self.store {
            store.close().await;
        }
    }
}

pub struct FolderIndexManager {
    config: ManagerConfig,
    embedders: Arc<dyn EmbedderSource>,
    emit: StatusEmitter,
    open: tokio::sync::Mutex<Option<OpenFolder>>,
}

/// The `path` of `inner` below `outer`, `/`-separated; `""` when equal.
fn relative_below(outer: &Path, inner: &Path) -> Option<String> {
    let rest = inner.strip_prefix(outer).ok()?;
    Some(
        rest.components()
            .map(|part| part.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/"),
    )
}

fn root_string(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn remove_dir(dir: &Path) {
    match std::fs::remove_dir_all(dir) {
        Ok(()) => info!(dir = %dir.display(), "Removed a folder index"),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => warn!(dir = %dir.display(), %error, "Could not remove a folder index"),
    }
}

/// The words for a walk that found more files than the cap.
fn too_large_message(found: TooLarge, cap: usize) -> String {
    format!(
        "{}{} files to index; the limit is {}. Search uses text matching.",
        if found.capped { "More than " } else { "" },
        group_digits(found.count),
        group_digits(cap)
    )
}

/// A closed index's state from its counts: work left is a pause; nothing at
/// all is no index, unless a full run finished over an empty folder.
fn settled_state(counts: &Counts, complete: bool) -> FolderIndexSummaryState {
    if counts.files_indexed < counts.files_total || counts.passages_embedded < counts.passages_total
    {
        FolderIndexSummaryState::Partial
    } else if counts.files_total == 0 && !complete {
        FolderIndexSummaryState::NotIndexed
    } else {
        FolderIndexSummaryState::Indexed
    }
}

/// The open folder's live status as the folders list shows it.
fn live_summary(status: &FolderIndexStatusDto, own_dir: Option<&Path>) -> FolderIndexSummaryDto {
    let counts = Counts {
        files_total: status.files_total,
        files_indexed: status.files_indexed,
        passages_total: status.passages_total,
        passages_embedded: status.passages_embedded,
    };
    let state = match status.state {
        FolderIndexState::Scanning | FolderIndexState::Indexing => {
            FolderIndexSummaryState::Indexing
        }
        FolderIndexState::Ready => FolderIndexSummaryState::Indexed,
        FolderIndexState::TooLarge => FolderIndexSummaryState::TooLarge,
        FolderIndexState::Refused => FolderIndexSummaryState::Refused,
        FolderIndexState::Error => FolderIndexSummaryState::Error,
        // No model to embed with: what is on disk is all there is.
        FolderIndexState::Unavailable => settled_state(&counts, true),
    };
    FolderIndexSummaryDto {
        state,
        files_total: counts.files_total,
        files_indexed: counts.files_indexed,
        passages_total: counts.passages_total,
        passages_embedded: counts.passages_embedded,
        bytes: own_dir.map_or(0, store::dir_bytes),
        index_root: (status.index_root != status.root).then(|| status.index_root.clone()),
        eta_seconds: status.eta_seconds,
        message: status.message.clone(),
    }
}

/// Removes every vectors file (and its key map and manifest) in `dir`.
fn remove_vector_files(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.filter_map(|entry| entry.ok()) {
        if entry.file_name().to_string_lossy().starts_with("vectors-") {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

impl FolderIndexManager {
    pub fn new(
        config: ManagerConfig,
        embedders: Arc<dyn EmbedderSource>,
        emit: StatusEmitter,
    ) -> Self {
        Self {
            config,
            embedders,
            emit,
            open: tokio::sync::Mutex::new(None),
        }
    }

    pub fn config(&self) -> &ManagerConfig {
        &self.config
    }

    /// The filesystem root, the home folder, or a folder that holds it.
    fn refusal(&self, root: &Path) -> Option<&'static str> {
        if root.parent().is_none() {
            return Some("Lattice does not index the whole disk. Pick a project folder.");
        }
        // An index inside the folder it indexes would watch its own writes.
        let base = &self.config.base_dir;
        if base.starts_with(root) || root.starts_with(base) {
            return Some(
                "Lattice does not index a folder that holds its own data. Pick a project folder.",
            );
        }
        let home = self.config.home.as_deref()?;
        if home == root {
            Some("Lattice does not index your home folder. Pick a project folder inside it.")
        } else if home.starts_with(root) {
            Some("Lattice does not index a folder that holds your home folder. Pick a project folder.")
        } else {
            None
        }
    }

    /// The index a folder uses, as `(index root, directory)`: its own when it
    /// has one; else the deepest enclosing folder's that holds anything;
    /// else its own, to be made.
    fn choose_index(&self, root: &Path, indexes: &[IndexEntry]) -> (PathBuf, PathBuf) {
        let own = store::index_dir(&self.config.base_dir, root);
        if indexes.iter().any(|entry| entry.dir == own) {
            return (root.to_path_buf(), own);
        }
        let parent = indexes
            .iter()
            .filter(|entry| !entry.too_large)
            .filter_map(|entry| {
                let parent = entry.root.as_deref()?;
                (parent != root && root.starts_with(parent) && parent.is_dir())
                    .then(|| (parent.to_path_buf(), entry.dir.clone()))
            })
            .max_by_key(|(parent, _)| parent.components().count());
        parent.unwrap_or_else(|| (root.to_path_buf(), own))
    }

    /// Every index directory on disk with the root its meta records: the
    /// folders list reads it once per listing.
    pub async fn index_dirs(&self) -> Vec<IndexEntry> {
        store::list_indexes(&self.config.base_dir).await
    }

    /// Opens `root`'s index and starts (or resumes) indexing it, closing any
    /// other open folder first. Opening the folder that is already open
    /// changes nothing.
    pub async fn open(&self, root: &str) -> Result<FolderIndexStatusDto> {
        let scope = Scope::open(root)?;
        let mut slot = self.open.lock().await;
        if let Some(current) = slot.as_ref() {
            // Opening a failed folder again is the retry; any other state of
            // the same folder is left running.
            let retry = matches!(
                current.status.get().state,
                FolderIndexState::Error | FolderIndexState::Unavailable
            );
            if current.root == scope.root() && !retry {
                return Ok(current.status.get());
            }
        }
        if let Some(previous) = slot.take() {
            previous.shut().await;
        }
        self.open_into(&mut slot, scope, false).await
    }

    async fn open_into(
        &self,
        slot: &mut Option<OpenFolder>,
        scope: Scope,
        wipe: bool,
    ) -> Result<FolderIndexStatusDto> {
        let root = scope.root().to_path_buf();
        let root_text = root_string(&root);

        if let Some(reason) = self.refusal(&root) {
            let status = Arc::new(StatusCell::new(
                FolderIndexStatusDto::new(&root_text, &root_text, FolderIndexState::Refused)
                    .with_message(reason),
                Arc::clone(&self.emit),
            ));
            status.announce();
            let snapshot = status.get();
            *slot = Some(OpenFolder {
                root,
                dir: None,
                prefix: String::new(),
                status,
                store: None,
                engine: Arc::new(RwLock::new(None)),
                cancel: CancellationToken::new(),
                task: None,
            });
            return Ok(snapshot);
        }

        let indexes = store::list_indexes(&self.config.base_dir).await;
        let (index_root, dir) = self.choose_index(&root, &indexes);
        let prefix = relative_below(&index_root, &root).unwrap_or_default();
        if wipe && prefix.is_empty() {
            remove_dir(&dir);
        }
        let store = Arc::new(FolderStore::open(&dir).await?);
        if store.meta(store::META_ROOT).await?.is_none() {
            store
                .set_meta(store::META_ROOT, &root_string(&index_root))
                .await?;
        }
        store
            .set_meta(
                store::META_LAST_OPENED,
                &store::next_open_stamp().to_string(),
            )
            .await?;
        let counts = store.counts(&prefix).await?;
        let mut initial = FolderIndexStatusDto::new(
            &root_text,
            &root_string(&index_root),
            FolderIndexState::Scanning,
        );
        initial.files_total = counts.files_total;
        initial.files_indexed = counts.files_indexed;
        initial.passages_total = counts.passages_total;
        initial.passages_embedded = counts.passages_embedded;
        let status = Arc::new(StatusCell::new(initial, Arc::clone(&self.emit)));
        status.announce();

        let cancel = crate::shared::background::cancellation_token().child_token();
        let engine = Arc::new(RwLock::new(None));
        let index_scope = Scope::open(&root_string(&index_root))?;
        let job = FolderJob {
            embedders: Arc::clone(&self.embedders),
            store: Arc::clone(&store),
            dir: dir.clone(),
            scope: index_scope,
            walk_root: root.clone(),
            prefix: prefix.clone(),
            status: Arc::clone(&status),
            engine: Arc::clone(&engine),
            cancel: cancel.clone(),
            limits: self.config.limits,
            debounce: self.config.debounce,
            wipe_prefix: wipe && !prefix.is_empty(),
        };
        let task = crate::shared::background::spawn(job.run());
        let snapshot = status.get();
        info!(root = %root.display(), index_root = %index_root.display(), "Folder index opened");
        *slot = Some(OpenFolder {
            root,
            dir: Some(dir),
            prefix,
            status,
            store: Some(store),
            engine,
            cancel,
            task,
        });
        Ok(snapshot)
    }

    /// Closes the open folder's index, if any.
    pub async fn close(&self) {
        if let Some(previous) = self.open.lock().await.take() {
            previous.shut().await;
        }
    }

    /// The live status for the open folder; for another, what its index on
    /// disk holds.
    pub async fn status(&self, root: &str) -> Result<FolderIndexStatusDto> {
        let scope = Scope::open(root)?;
        let root = scope.root().to_path_buf();
        if let Some(current) = self.open.lock().await.as_ref() {
            if current.root == root {
                return Ok(current.status.get());
            }
        }
        let root_text = root_string(&root);
        if let Some(reason) = self.refusal(&root) {
            return Ok(FolderIndexStatusDto::new(
                &root_text,
                &root_text,
                FolderIndexState::Refused,
            )
            .with_message(reason));
        }
        let indexes = store::list_indexes(&self.config.base_dir).await;
        let (index_root, dir) = self.choose_index(&root, &indexes);
        if !dir.join(store::DB_FILE).exists() {
            return Err(AppError::NotFound(format!(
                "{} has no folder index yet.",
                root.display()
            )));
        }
        let prefix = relative_below(&index_root, &root).unwrap_or_default();
        let store = FolderStore::open(&dir).await?;
        let counts = store.counts(&prefix).await;
        store.close().await;
        let counts = counts?;
        let mut status = FolderIndexStatusDto::new(
            &root_text,
            &root_string(&index_root),
            FolderIndexState::Ready,
        );
        status.files_total = counts.files_total;
        status.files_indexed = counts.files_indexed;
        status.passages_total = counts.passages_total;
        status.passages_embedded = counts.passages_embedded;
        Ok(status)
    }

    /// What each of `roots`' index holds, for the folders list: the live
    /// status for the open folder, what is on disk for the rest. `indexes`
    /// is the directory listing, read once for the lot.
    pub async fn summaries(
        &self,
        roots: &[PathBuf],
        indexes: &[IndexEntry],
    ) -> Vec<FolderIndexSummaryDto> {
        let live = {
            let slot = self.open.lock().await;
            slot.as_ref().map(|current| {
                let own = current.dir.clone().filter(|_| current.prefix.is_empty());
                (current.root.clone(), own, current.status.get())
            })
        };
        let mut summaries = Vec::with_capacity(roots.len());
        for root in roots {
            summaries.push(match &live {
                Some((open, own, status)) if open == root => live_summary(status, own.as_deref()),
                _ => self.summary_on_disk(root, indexes).await,
            });
        }
        summaries
    }

    /// A closed folder's index, read from disk without opening it.
    async fn summary_on_disk(&self, root: &Path, indexes: &[IndexEntry]) -> FolderIndexSummaryDto {
        let mut summary = FolderIndexSummaryDto::new(FolderIndexSummaryState::NotIndexed);
        if let Some(reason) = self.refusal(root) {
            summary.state = FolderIndexSummaryState::Refused;
            summary.message = Some(reason.to_string());
            return summary;
        }
        let (index_root, dir) = self.choose_index(root, indexes);
        let prefix = relative_below(&index_root, root).unwrap_or_default();
        if prefix.is_empty() {
            summary.bytes = store::dir_bytes(&dir);
        } else {
            summary.index_root = Some(root_string(&index_root));
        }
        match store::inspect(&dir, &prefix).await {
            Ok(None) => {}
            Ok(Some(snapshot)) => match snapshot.too_large.filter(|_| prefix.is_empty()) {
                Some(found) => {
                    summary.state = FolderIndexSummaryState::TooLarge;
                    summary.files_total = u32::try_from(found.count).unwrap_or(u32::MAX);
                    summary.message = Some(too_large_message(found, self.config.limits.max_files));
                }
                None => {
                    let counts = snapshot.counts;
                    summary.state = settled_state(&counts, snapshot.complete);
                    summary.files_total = counts.files_total;
                    summary.files_indexed = counts.files_indexed;
                    summary.passages_total = counts.passages_total;
                    summary.passages_embedded = counts.passages_embedded;
                }
            },
            Err(error) => {
                summary.state = FolderIndexSummaryState::Error;
                summary.message = Some(error.to_string());
            }
        }
        summary
    }

    /// Wipes `root`'s index and starts it over. A sub-folder that reuses a
    /// parent's index wipes only its own part of it.
    pub async fn rebuild(&self, root: &str) -> Result<FolderIndexStatusDto> {
        let scope = Scope::open(root)?;
        let mut slot = self.open.lock().await;
        if let Some(previous) = slot.take() {
            previous.shut().await;
        }
        self.open_into(&mut slot, scope, true).await
    }

    /// Deletes `root`'s own index directory, closing it first if it is open.
    /// A folder that reuses an enclosing folder's index has none of its own,
    /// and the enclosing one is left alone. The folder may be gone from disk
    /// by now, so a root that no longer resolves is taken as given.
    pub async fn delete_index(&self, root: &str) -> Result<()> {
        let root = Scope::open(root)
            .map(|scope| scope.root().to_path_buf())
            .unwrap_or_else(|_| PathBuf::from(root.trim()));
        let dir = store::index_dir(&self.config.base_dir, &root);
        let mut slot = self.open.lock().await;
        let open_here = slot.as_ref().is_some_and(|current| {
            current.root == root || current.dir.as_deref() == Some(dir.as_path())
        });
        if open_here {
            if let Some(current) = slot.take() {
                current.shut().await;
            }
        }
        remove_dir(&dir);
        Ok(())
    }

    /// Search over the open folder, when it is `root` and has anything
    /// indexed. `None` otherwise: refused, too large, no model, or empty.
    pub async fn search_for(&self, root: &Path) -> Option<Arc<FolderSearch>> {
        let search = {
            let slot = self.open.lock().await;
            let current = slot.as_ref()?;
            if current.root != root {
                return None;
            }
            if matches!(
                current.status.get().state,
                FolderIndexState::TooLarge
                    | FolderIndexState::Refused
                    | FolderIndexState::Unavailable
            ) {
                return None;
            }
            let engine = current.engine.read().clone()?;
            FolderSearch::new(
                Arc::clone(current.store.as_ref()?),
                engine.vectors,
                engine.embedder,
                current.prefix.clone(),
                Arc::clone(&current.status),
            )
        };
        search.has_chunks().await.then(|| Arc::new(search))
    }

    /// Waits for the open folder's first run to settle. For tests, which
    /// otherwise race the background task.
    #[cfg(test)]
    pub async fn settled(&self) -> FolderIndexStatusDto {
        let deadline = std::time::Instant::now() + Duration::from_secs(30);
        loop {
            let status = {
                let slot = self.open.lock().await;
                slot.as_ref().map(|current| current.status.get())
            };
            match status {
                Some(status)
                    if !matches!(
                        status.state,
                        FolderIndexState::Scanning | FolderIndexState::Indexing
                    ) =>
                {
                    return status
                }
                Some(status) if std::time::Instant::now() > deadline => return status,
                Some(_) => tokio::time::sleep(Duration::from_millis(10)).await,
                None => {
                    return FolderIndexStatusDto::new("", "", FolderIndexState::Error)
                        .with_message("nothing open")
                }
            }
        }
    }
}

/// Everything the background task of one open folder owns.
struct FolderJob {
    embedders: Arc<dyn EmbedderSource>,
    store: Arc<FolderStore>,
    dir: PathBuf,
    /// The index root; walks start here so its ignore rules apply.
    scope: Scope,
    /// The folder the user picked: what is walked and watched.
    walk_root: PathBuf,
    prefix: String,
    status: Arc<StatusCell>,
    engine: Arc<RwLock<Option<Engine>>>,
    cancel: CancellationToken,
    limits: Limits,
    debounce: Duration,
    /// A rebuild of a sub-folder inside a reused index.
    wipe_prefix: bool,
}

impl FolderJob {
    fn fail(&self, error: &AppError) {
        warn!(%error, root = %self.walk_root.display(), "Folder index failed");
        self.status.update(|status| {
            status.state = FolderIndexState::Error;
            status.message = Some(error.to_string());
            status.passages_per_second = None;
            status.eta_seconds = None;
        });
    }

    async fn run(self) {
        let embedder = tokio::select! {
            biased;
            _ = self.cancel.cancelled() => return,
            embedder = self.embedders.embedder() => embedder,
        };
        let Some(embedder) = embedder else {
            self.status.update(|status| {
                status.state = FolderIndexState::Unavailable;
                status.message = Some(
                    "No embedding model is active, so the folder is searched by text only.".into(),
                );
            });
            return;
        };
        let vectors = match self.prepare_vectors(embedder.as_ref()).await {
            Ok(vectors) => vectors,
            Err(error) => return self.fail(&error),
        };
        *self.engine.write() = Some(Engine {
            vectors: Arc::clone(&vectors),
            embedder: Arc::clone(&embedder),
        });
        let indexer = Indexer {
            store: Arc::clone(&self.store),
            vectors,
            embedder,
            scope: self.scope.clone(),
            prefix: self.prefix.clone(),
            status: Arc::clone(&self.status),
            cancel: self.cancel.clone(),
            limits: self.limits,
        };
        if self.wipe_prefix {
            let wiped = async {
                let removed = self.store.remove_under(&self.prefix).await?;
                indexer
                    .vectors
                    .remove_embeddings(&removed.iter().map(i64::to_string).collect::<Vec<_>>())
            };
            if let Err(error) = wiped.await {
                return self.fail(&error);
            }
        }

        // Watching starts before the walk, so a save made during a long
        // first run is picked up after it rather than missed.
        let watching = match watcher::watch(&self.walk_root, self.debounce) {
            Ok(watching) => Some(watching),
            Err(error) => {
                warn!(%error, "Folder index runs without a watcher");
                None
            }
        };

        if !self.settle(indexer.run().await) {
            return;
        }
        let Some((_watcher, mut changes)) = watching else {
            return;
        };
        loop {
            let first = tokio::select! {
                biased;
                _ = self.cancel.cancelled() => return,
                change = changes.recv() => match change {
                    Some(change) => change,
                    None => return,
                },
            };
            let mut paths = HashSet::new();
            let mut rescan = false;
            let mut take = |change: Change| match change {
                Change::Paths(found) => paths.extend(found),
                Change::Rescan => rescan = true,
            };
            take(first);
            while let Ok(change) = changes.try_recv() {
                take(change);
            }
            let outcome = if rescan || paths.len() > RESCAN_PATHS {
                indexer.run().await
            } else {
                let relative: Vec<String> = paths
                    .iter()
                    .filter_map(|path| self.watched_relative(path))
                    .collect();
                if relative.is_empty() {
                    continue;
                }
                indexer.sync_paths(relative).await
            };
            if !self.settle(outcome) {
                return;
            }
        }
    }

    /// Records how a run ended. `false` when the task should stop.
    fn settle(&self, outcome: Result<Outcome>) -> bool {
        match outcome {
            Ok(Outcome::Done) => {
                self.status.update(|status| {
                    status.state = FolderIndexState::Ready;
                    status.message = None;
                    status.passages_per_second = None;
                    status.eta_seconds = None;
                });
                true
            }
            Ok(Outcome::Cancelled) => {
                let status = self.status.get();
                info!(
                    root = %self.walk_root.display(),
                    embedded = status.passages_embedded,
                    total = status.passages_total,
                    "Folder index paused"
                );
                false
            }
            Ok(Outcome::TooLarge { count, capped }) => {
                let message = too_large_message(TooLarge { count, capped }, self.limits.max_files);
                self.status.update(|status| {
                    status.state = FolderIndexState::TooLarge;
                    status.files_total = u32::try_from(count).unwrap_or(u32::MAX);
                    status.files_indexed = 0;
                    status.message = Some(message);
                });
                false
            }
            Err(error) => {
                self.fail(&error);
                false
            }
        }
    }

    /// A watcher path as the index names it, or `None` outside the walked
    /// folder.
    fn watched_relative(&self, path: &Path) -> Option<String> {
        if !path.starts_with(&self.walk_root) {
            return None;
        }
        let relative = self.scope.relative_of(path)?;
        (!relative.is_empty()).then_some(relative)
    }

    /// The vectors file for this embedder. A different identity than the one
    /// recorded drops every vectors file and marks every file for embedding;
    /// so does a vectors file that is missing or will not load.
    async fn prepare_vectors(
        &self,
        embedder: &dyn EmbeddingPort,
    ) -> Result<Arc<USearchVectorIndex>> {
        let identity = embedder.model_identity();
        let stored = self.store.meta(store::META_IDENTITY).await?;
        let path = store::vectors_path(&self.dir, &identity);
        let keymap = path.with_extension("keymap.json");
        if stored.as_deref() != Some(identity.as_str()) || !path.exists() || !keymap.exists() {
            if stored.is_some() {
                info!(
                    from = stored.as_deref().unwrap_or_default(),
                    to = %identity,
                    "Folder index: embedding model changed; embedding again"
                );
            }
            remove_vector_files(&self.dir);
            self.store.reset_embedded().await?;
            self.store.set_meta(store::META_IDENTITY, &identity).await?;
        }
        let dimension = embedder.dimension();
        let load_path = path.clone();
        let loaded = tokio::task::spawn_blocking(move || {
            USearchVectorIndex::open_or_create(dimension, load_path)
        })
        .await
        .map_err(|error| {
            AppError::InternalError(format!("Loading folder vectors failed: {error}"))
        })?;
        let index = match loaded {
            Ok(index) => index,
            Err(error) => {
                warn!(%error, "Folder index: vectors file unreadable; embedding again");
                remove_vector_files(&self.dir);
                self.store.reset_embedded().await?;
                USearchVectorIndex::open_or_create(dimension, path)?
            }
        };
        Ok(Arc::new(index.with_coalesced_saves()))
    }
}

/// `20000` as `20,000`.
pub fn group_digits(value: usize) -> String {
    let digits = value.to_string();
    let mut out = String::new();
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            out.push(',');
        }
        out.push(digit);
    }
    out
}

static MANAGER: OnceLock<Arc<FolderIndexManager>> = OnceLock::new();

/// The app's manager, installed by the first index command.
pub fn global() -> Option<Arc<FolderIndexManager>> {
    MANAGER.get().cloned()
}

/// Installs the app's manager unless one already is; returns whichever won.
pub fn install(manager: impl FnOnce() -> FolderIndexManager) -> Arc<FolderIndexManager> {
    Arc::clone(MANAGER.get_or_init(|| Arc::new(manager())))
}

/// Search over `root` when it is the open folder and has an index.
pub async fn search_for_root(root: &Path) -> Option<Arc<FolderSearch>> {
    global()?.search_for(root).await
}

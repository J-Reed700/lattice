//! The open folder: opening, rebuilding and closing it, deleting its index,
//! the watcher that keeps its index current, and search over it.

use super::{relative_below, root_string, FolderIndexManager, OpenFolder, OpenView};
use crate::features::explorer::index::build;
use crate::features::explorer::index::dto::{FolderIndexState, FolderIndexStatusDto};
use crate::features::explorer::index::run::{Indexer, Limits, Outcome, StatusCell};
use crate::features::explorer::index::search::FolderSearch;
use crate::features::explorer::index::store;
use crate::features::explorer::index::watcher::{self, Change, FolderWatcher};
use crate::features::explorer::scope::Scope;
use crate::infrastructure::passage_index::{EmbedTurns, LiveIndex, PassageTextSearch, Priority};
use lattice_core::shared::{runtime::jobs::JobStatus, Result};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Weak};
#[cfg(test)]
use std::time::Duration;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

/// Watcher batches past this many paths are cheaper as one full pass.
const RESCAN_PATHS: usize = 2_000;

impl FolderIndexManager {
    /// Opens `root`'s index and queues a build of it, ahead of every other,
    /// closing any other open folder first. Opening the folder that is
    /// already open changes nothing, unless its index failed: that is the
    /// retry.
    pub async fn open(&self, root: &str) -> Result<FolderIndexStatusDto> {
        let scope = Scope::open(root)?;
        let mut open = self.open.lock().await;
        if let Some(current) = open.as_ref() {
            let retry = matches!(
                current.status.get().state,
                FolderIndexState::Error | FolderIndexState::Unavailable
            );
            if current.root == scope.root() && !retry {
                return Ok(current.status.get());
            }
        }
        if let Some(previous) = open.take() {
            self.close_folder(previous).await;
        }
        let folder = self.open_folder(scope, false).await?;
        let snapshot = folder.status.get();
        info!(root = %folder.root.display(), index_root = %snapshot.index_root, "Folder index opened");
        *open = Some(folder);
        Ok(snapshot)
    }

    async fn open_folder(&self, scope: Scope, rebuild: bool) -> Result<OpenFolder> {
        let root = scope.root().to_path_buf();
        let root_text = root_string(&root);
        if let Some(reason) = self.refusal(&root) {
            let status = Arc::new(StatusCell::new(
                FolderIndexStatusDto::new(&root_text, &root_text, FolderIndexState::Refused)
                    .with_message(reason),
            ));
            self.show(&root, &status);
            return Ok(OpenFolder {
                index_root: root.clone(),
                root,
                dir: None,
                prefix: String::new(),
                status,
                index: None,
                _claim: None,
                cancel: CancellationToken::new(),
                watching: None,
            });
        }

        let indexes = store::list_indexes(&self.config.base_dir).await;
        let (index_root, dir) = self.choose_index(&root, &indexes);
        let prefix = relative_below(&index_root, &root).unwrap_or_default();
        if rebuild && prefix.is_empty() {
            self.forget_index(&index_root, &dir).await?;
        }
        let index = self.acquire(&index_root, &dir).await?;
        let opened = async {
            // The folders list orders by this.
            index
                .store
                .set_meta(
                    store::META_LAST_OPENED,
                    &self.open_stamps.next().to_string(),
                )
                .await?;
            index.store.counts(&prefix).await
        }
        .await;
        let counts = match opened {
            Ok(counts) => counts,
            Err(error) => {
                self.release(&index).await;
                return Err(error);
            }
        };
        let status = Arc::new(StatusCell::new(build::initial_status(
            &root,
            &index_root,
            FolderIndexState::Scanning,
            counts,
        )));
        if let Some(sink) = self.folder_changes.lock().clone() {
            status.forward(sink);
        }
        self.show(&root, &status);

        // Watching starts before the build, so a save made during a long
        // build is picked up after it rather than missed.
        let cancel = self.background.token().child_token();
        let watching = match watcher::watch(&root, self.config.debounce) {
            Ok((watcher, changes)) => {
                let task = WatchTask {
                    manager: Weak::clone(&self.me),
                    index: Arc::clone(&index),
                    scope: Scope::open(&root_string(&index_root))?,
                    walk_root: root.clone(),
                    prefix: prefix.clone(),
                    status: Arc::clone(&status),
                    cancel: cancel.clone(),
                    limits: self.config.limits,
                    turns: Arc::clone(&self.turns),
                };
                self.background.spawn(task.run(watcher, changes))
            }
            Err(error) => {
                warn!(%error, "Folder index runs without a watcher");
                None
            }
        };
        let folder = OpenFolder {
            root: root.clone(),
            index_root: index_root.clone(),
            _claim: Some(self.turns.claim(&dir, Priority::Open)),
            dir: Some(dir),
            prefix: prefix.clone(),
            status: Arc::clone(&status),
            index: Some(index),
            cancel,
            watching,
        };
        match self
            .ensure_build(&root, &index_root, rebuild && !prefix.is_empty())
            .await
        {
            Ok(job) => {
                // A build already running has the newest word on its progress.
                if job.status == JobStatus::Running {
                    if let Some(activity) = build::activity_status(&job) {
                        status.replace(activity);
                    }
                }
                Ok(folder)
            }
            Err(error) => {
                self.close_folder(folder).await;
                Err(error)
            }
        }
    }

    fn show(&self, root: &Path, status: &Arc<StatusCell>) {
        *self.view.lock() = Some(OpenView {
            root: root.to_path_buf(),
            status: Arc::clone(status),
        });
    }

    /// Stops watching a folder and lets go of its index. Its build goes on.
    async fn close_folder(&self, mut folder: OpenFolder) {
        folder.cancel.cancel();
        if let Some(task) = folder.watching.take() {
            let _ = task.await;
        }
        {
            let mut view = self.view.lock();
            if view.as_ref().is_some_and(|view| view.root == folder.root) {
                *view = None;
            }
        }
        *self.first.lock() = None;
        if let Some(index) = folder.index.take() {
            self.release(&index).await;
        }
    }

    /// Closes the open folder's index, if any: its watcher and its hold on
    /// the index. A build of it goes on in the background.
    pub async fn close(&self) {
        if let Some(previous) = self.open.lock().await.take() {
            self.close_folder(previous).await;
        }
    }

    /// Wipes `root`'s index and builds it again. A sub-folder that reuses a
    /// parent's index wipes only its own part of it.
    pub async fn rebuild(&self, root: &str) -> Result<FolderIndexStatusDto> {
        let scope = Scope::open(root)?;
        let mut open = self.open.lock().await;
        if let Some(previous) = open.take() {
            self.close_folder(previous).await;
        }
        let folder = self.open_folder(scope, true).await?;
        let snapshot = folder.status.get();
        *open = Some(folder);
        Ok(snapshot)
    }

    /// Deletes `root`'s own index directory, closing it first if it is open
    /// and stopping its builds. A folder that reuses an enclosing folder's
    /// index has none of its own, and the enclosing one is left alone. The
    /// folder may be gone from disk by now, so a root that no longer resolves
    /// is taken as given.
    pub async fn delete_index(&self, root: &str) -> Result<()> {
        let root = Scope::open(root)
            .map(|scope| scope.root().to_path_buf())
            .unwrap_or_else(|_| PathBuf::from(root.trim()));
        let dir = store::index_dir(&self.config.base_dir, &root);
        let mut open = self.open.lock().await;
        if open.as_ref().is_some_and(|current| {
            current.root == root || current.dir.as_deref() == Some(dir.as_path())
        }) {
            if let Some(current) = open.take() {
                self.close_folder(current).await;
            }
        }
        self.forget_index(&root, &dir).await
    }

    /// Search over the open folder, when it is `root` and has anything
    /// indexed. `None` otherwise: refused, too large, no model, or empty.
    pub async fn search_for(&self, root: &Path) -> Option<Arc<FolderSearch>> {
        let search = {
            let open = self.open.lock().await;
            let current = open.as_ref()?;
            if current.root != root {
                return None;
            }
            // A folder past the file bound is indexed up to it, so its index
            // is searched for what it holds.
            if matches!(
                current.status.get().state,
                FolderIndexState::Refused | FolderIndexState::Unavailable
            ) {
                return None;
            }
            let index = current.index.as_ref()?;
            let engine = index.engine()?;
            let hybrid = self.searches().hybrid(
                engine.embedder,
                engine.vectors.port(),
                Arc::new(PassageTextSearch::new(Arc::clone(&index.store))),
            );
            FolderSearch::new(
                Arc::clone(&index.store),
                hybrid,
                current.prefix.clone(),
                Arc::clone(&current.status),
            )
        };
        search.has_chunks().await.then(|| Arc::new(search))
    }

    /// Waits for the open folder's build to settle. For tests, which
    /// otherwise race the job.
    #[cfg(test)]
    pub async fn settled(&self) -> FolderIndexStatusDto {
        let deadline = std::time::Instant::now() + Duration::from_secs(30);
        loop {
            let Some(view) = self.view() else {
                return FolderIndexStatusDto::new("", "", FolderIndexState::Error)
                    .with_message("nothing open");
            };
            let status = view.status.get();
            let first = self.first.lock().clone();
            let building = match first {
                Some(id) => self
                    .jobs
                    .store()
                    .get(&id)
                    .await
                    .is_ok_and(|job| !job.status.is_finished()),
                None => false,
            };
            let moving = matches!(
                status.state,
                FolderIndexState::Scanning | FolderIndexState::Indexing
            );
            if (!building && !moving) || std::time::Instant::now() > deadline {
                return status;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }
}

/// Keeps the open folder's index current while it stays open.
struct WatchTask {
    manager: Weak<FolderIndexManager>,
    index: Arc<LiveIndex>,
    /// The index root; paths are named relative to it.
    scope: Scope,
    /// The folder the user picked: what is watched.
    walk_root: PathBuf,
    prefix: String,
    status: Arc<StatusCell>,
    cancel: CancellationToken,
    limits: Limits,
    turns: Arc<EmbedTurns>,
}

impl WatchTask {
    async fn run(self, _watcher: FolderWatcher, mut changes: mpsc::UnboundedReceiver<Change>) {
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
            if rescan || paths.len() > RESCAN_PATHS {
                if let Some(manager) = self.manager.upgrade() {
                    // Closing the folder waits for this task, holding the
                    // lock a rescan needs.
                    tokio::select! {
                        biased;
                        _ = self.cancel.cancelled() => return,
                        _ = manager.rescan(&self.walk_root) => {}
                    }
                }
                continue;
            }
            let relative: Vec<String> = paths
                .iter()
                .filter_map(|path| self.watched_relative(path))
                .collect();
            if relative.is_empty() {
                continue;
            }
            // A build over this index holds the turn to write; these paths
            // are synced once it is done.
            let writing = tokio::select! {
                biased;
                _ = self.cancel.cancelled() => return,
                writing = self.index.writing.lock() => writing,
            };
            // Nothing has prepared the vectors yet: the build that will
            // covers these paths too.
            let Some(engine) = self.index.engine() else {
                continue;
            };
            let indexer = Indexer {
                store: Arc::clone(&self.index.store),
                vectors: engine.vectors,
                embedder: engine.embedder,
                scope: self.scope.clone(),
                prefix: self.prefix.clone(),
                status: Arc::clone(&self.status),
                cancel: self.cancel.clone(),
                limits: self.limits,
                turns: Arc::clone(&self.turns),
            };
            let outcome = indexer.sync_paths(relative).await;
            drop(writing);
            match outcome {
                Ok(Outcome::Done) => self.status.update(|status| {
                    status.state = FolderIndexState::Ready;
                    status.message = None;
                    status.passages_per_second = None;
                    status.eta_seconds = None;
                }),
                Ok(Outcome::Cancelled) => return,
                Ok(Outcome::TooLarge { .. }) => {}
                Err(error) => {
                    warn!(%error, root = %self.walk_root.display(), "Folder index update failed");
                    self.status.update(|status| {
                        status.state = FolderIndexState::Error;
                        status.message = Some(error.to_string());
                    });
                    return;
                }
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
}

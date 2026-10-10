//! Folder indexes: which folders get one, the folder open in the Explorer,
//! and the build jobs that walk and embed them.
//!
//! - A build is a job on the app's job runtime ([`super::build`]), with the
//!   index root as its subject, so two builds never write one index at once.
//!   A folder has at most one live build. Builds run one at a time; the
//!   folder being opened goes first, and the other builds give up their turn
//!   and resume after it. Closing or leaving a folder leaves its build
//!   running, and a build stopped by a restart resumes at the next start,
//!   while its folder still exists.
//! - One folder is open at a time. It is watched: small changes update its
//!   index in place, once any build over that index has finished, and a
//!   change too large for that queues a build.
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

use super::build::{self, BuildRequest, LiveIndex};
use super::dto::{
    FolderIndexState, FolderIndexStatusDto, FolderIndexSummaryDto, FolderIndexSummaryState,
};
use super::run::{Indexer, Limits, Outcome, StatusCell, StatusSink};
use super::search::FolderSearch;
use super::store::{self, Counts, FolderStore, IndexEntry, TooLarge};
use super::watcher::{self, Change, FolderWatcher};
use crate::application::ports::EmbeddingPort;
use crate::features::explorer::scope::Scope;
use crate::shared::{
    runtime::jobs::{JobRecord, JobRuntime, JobStatus, NewJob},
    AppError, Result,
};
use async_trait::async_trait;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Weak};
use std::time::Duration;
use tokio::sync::mpsc;
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

/// The open folder as builds and searches see it, readable without waiting
/// for an open or a close in progress.
#[derive(Clone)]
pub(super) struct OpenView {
    pub root: PathBuf,
    pub status: Arc<StatusCell>,
}

struct OpenFolder {
    root: PathBuf,
    index_root: PathBuf,
    /// The index directory; `None` for a refused folder.
    dir: Option<PathBuf>,
    prefix: String,
    status: Arc<StatusCell>,
    index: Option<Arc<LiveIndex>>,
    /// Stops the watcher task.
    cancel: CancellationToken,
    watching: Option<JoinHandle<()>>,
}

/// An index directory in use, and by how many: the open folder and builds.
struct LiveEntry {
    index: Arc<LiveIndex>,
    users: usize,
}

pub struct FolderIndexManager {
    config: ManagerConfig,
    pub(super) embedders: Arc<dyn EmbedderSource>,
    pub(super) jobs: Arc<JobRuntime>,
    open: tokio::sync::Mutex<Option<OpenFolder>>,
    view: Arc<parking_lot::Mutex<Option<OpenView>>>,
    /// The open folder's build, which runs before any other.
    first: parking_lot::Mutex<Option<String>>,
    live: tokio::sync::Mutex<HashMap<PathBuf, LiveEntry>>,
    /// Where the open folder's status goes when the watcher updates it.
    folder_changes: parking_lot::Mutex<Option<StatusSink>>,
    me: Weak<FolderIndexManager>,
}

/// The `path` of `inner` below `outer`, `/`-separated; `""` when equal.
pub(super) fn relative_below(outer: &Path, inner: &Path) -> Option<String> {
    let rest = inner.strip_prefix(outer).ok()?;
    Some(
        rest.components()
            .map(|part| part.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/"),
    )
}

pub(super) fn root_string(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

pub(super) fn remove_dir(dir: &Path) {
    match std::fs::remove_dir_all(dir) {
        Ok(()) => info!(dir = %dir.display(), "Removed a folder index"),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => warn!(dir = %dir.display(), %error, "Could not remove a folder index"),
    }
}

/// The words for a walk that found more files than the cap.
pub(super) fn too_large_message(found: TooLarge, cap: usize) -> String {
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

impl FolderIndexManager {
    pub fn new(
        config: ManagerConfig,
        embedders: Arc<dyn EmbedderSource>,
        jobs: Arc<JobRuntime>,
    ) -> Arc<Self> {
        Arc::new_cyclic(|me| Self {
            config,
            embedders,
            jobs,
            open: tokio::sync::Mutex::new(None),
            view: Arc::default(),
            first: parking_lot::Mutex::new(None),
            live: tokio::sync::Mutex::new(HashMap::new()),
            folder_changes: parking_lot::Mutex::new(None),
            me: me.clone(),
        })
    }

    /// Sends the open folder's status to `sink` whenever the watcher's
    /// updates move it. A build reports through its job instead; these
    /// updates are too small and too frequent to be jobs of their own.
    pub fn on_folder_changed(&self, sink: StatusSink) {
        *self.folder_changes.lock() = Some(sink);
    }

    /// Registers the build job kind. Builds the last process left running
    /// resume, after their folder is checked to still exist.
    pub async fn register(self: &Arc<Self>) -> Result<()> {
        self.jobs
            .register(
                build::FOLDER_INDEX,
                Arc::new(build::BuildJob::new(Arc::downgrade(self))),
                build::build_job_config(),
            )
            .await
    }

    pub fn config(&self) -> &ManagerConfig {
        &self.config
    }

    pub(super) fn view(&self) -> Option<OpenView> {
        self.view.lock().clone()
    }

    pub(super) fn view_handle(&self) -> Arc<parking_lot::Mutex<Option<OpenView>>> {
        Arc::clone(&self.view)
    }

    /// The build that runs before any other, when it is not `id`.
    pub(super) fn first_other_than(&self, id: &str) -> Option<String> {
        self.first.lock().clone().filter(|first| first != id)
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

    pub(super) fn index_dir_of(&self, index_root: &Path) -> PathBuf {
        store::index_dir(&self.config.base_dir, index_root)
    }

    /// Every index directory on disk with the root its meta records: the
    /// folders list reads it once per listing.
    pub async fn index_dirs(&self) -> Vec<IndexEntry> {
        store::list_indexes(&self.config.base_dir).await
    }

    /// Opens the index in `dir` for one more user, sharing it with the open
    /// folder or a build that already has it.
    pub(super) async fn acquire(&self, index_root: &Path, dir: &Path) -> Result<Arc<LiveIndex>> {
        let mut live = self.live.lock().await;
        if let Some(entry) = live.get_mut(dir) {
            entry.users += 1;
            return Ok(Arc::clone(&entry.index));
        }
        let store = Arc::new(FolderStore::open(dir).await?);
        if store.meta(store::META_ROOT).await?.is_none() {
            store
                .set_meta(store::META_ROOT, &root_string(index_root))
                .await?;
        }
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

    /// Gives back one user's hold; the last closes the index's pool.
    pub(super) async fn release(&self, index: &LiveIndex) {
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

    /// The live builds of the folder index, oldest first.
    async fn live_builds(&self) -> Result<Vec<JobRecord>> {
        self.jobs.store().live(build::FOLDER_INDEX).await
    }

    /// Queues a build of `root` unless it already has a live one, and puts it
    /// first: every other live build gives up its turn. Finished builds of the
    /// same index are forgotten, so the job history stays one per index.
    async fn ensure_build(
        &self,
        root: &Path,
        index_root: &Path,
        wipe_prefix: bool,
    ) -> Result<JobRecord> {
        let subject = root_string(index_root);
        let request = BuildRequest {
            root: root_string(root),
            index_root: subject.clone(),
            wipe_prefix,
        };
        let builds = self
            .jobs
            .store()
            .list_for_subject(build::FOLDER_INDEX, &subject)
            .await?;
        let live = builds.iter().find(|job| {
            !job.status.is_finished()
                && BuildRequest::of(job).is_some_and(|asked| asked.root == request.root)
        });
        let job = match live {
            Some(job) if !wipe_prefix => job.clone(),
            live => {
                if let Some(job) = live {
                    self.stop_build(&job.id).await;
                }
                for old in builds.iter().filter(|job| job.status.is_finished()) {
                    if let Err(error) = self.jobs.store().delete(&old.id).await {
                        warn!(%error, job_id = %old.id, "Could not forget a finished folder build");
                    }
                }
                let requested = serde_json::to_value(&request)?;
                self.jobs
                    .submit(&NewJob {
                        kind: build::FOLDER_INDEX.into(),
                        subject_id: Some(subject),
                        operation_id: uuid::Uuid::new_v4().to_string(),
                        payload_hash: request.root.clone(),
                        requested,
                        progress_total: 100,
                        message: "Queued".into(),
                    })
                    .await?
            }
        };
        *self.first.lock() = Some(job.id.clone());
        self.jobs.dispatch(&job.id);
        for other in self.live_builds().await? {
            if other.id != job.id {
                self.jobs.requeue(&other.id);
            }
        }
        Ok(job)
    }

    /// Cancels a build and waits for its worker to let go of the index.
    async fn stop_build(&self, id: &str) {
        if let Err(error) = self.jobs.cancel(id).await {
            warn!(%error, job_id = id, "Could not cancel a folder build");
        }
        self.jobs.finished(id).await;
    }

    /// Cancels every build over the index rooted at `index_root`, forgets
    /// them, and removes its directory. Nothing may hold the index after.
    async fn forget_index(&self, index_root: &Path, dir: &Path) -> Result<()> {
        let builds = self
            .jobs
            .store()
            .list_for_subject(build::FOLDER_INDEX, &root_string(index_root))
            .await?;
        for job in &builds {
            if !job.status.is_finished() {
                self.stop_build(&job.id).await;
            }
            if let Err(error) = self.jobs.store().delete(&job.id).await {
                warn!(%error, job_id = %job.id, "Could not forget a folder build");
            }
        }
        remove_dir(dir);
        Ok(())
    }

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
                    &store::next_open_stamp().to_string(),
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
        let cancel = crate::shared::runtime::background::cancellation_token().child_token();
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
                };
                crate::shared::runtime::background::spawn(task.run(watcher, changes))
            }
            Err(error) => {
                warn!(%error, "Folder index runs without a watcher");
                None
            }
        };
        let folder = OpenFolder {
            root: root.clone(),
            index_root: index_root.clone(),
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

    /// Queues a full build of the open folder, when it is still `root`.
    async fn rescan(&self, root: &Path) {
        let index_root = {
            let open = self.open.lock().await;
            match open.as_ref() {
                Some(current) if current.root == root && current.dir.is_some() => {
                    current.index_root.clone()
                }
                _ => return,
            }
        };
        if let Err(error) = self.ensure_build(root, &index_root, false).await {
            warn!(%error, root = %root.display(), "Could not queue a folder rescan");
        }
    }

    /// The running build of `root`, with the status it last saved.
    async fn running_status(&self, root: &Path) -> Option<FolderIndexStatusDto> {
        let text = root_string(root);
        let builds = self.live_builds().await.ok()?;
        builds
            .iter()
            .filter(|job| job.status == JobStatus::Running)
            .filter(|job| BuildRequest::of(job).is_some_and(|asked| asked.root == text))
            .find_map(build::activity_status)
    }

    /// The live status for the open folder or a running build; for another,
    /// what its index on disk holds.
    pub async fn status(&self, root: &str) -> Result<FolderIndexStatusDto> {
        let scope = Scope::open(root)?;
        let root = scope.root().to_path_buf();
        if let Some(view) = self.view().filter(|view| view.root == root) {
            return Ok(view.status.get());
        }
        if let Some(status) = self.running_status(&root).await {
            return Ok(status);
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
        Ok(build::initial_status(
            &root,
            &index_root,
            FolderIndexState::Ready,
            counts?,
        ))
    }

    /// What each of `roots`' index holds, for the folders list: the live
    /// status for the open folder and running builds, what is on disk for
    /// the rest. `indexes` is the directory listing, read once for the lot.
    pub async fn summaries(
        &self,
        roots: &[PathBuf],
        indexes: &[IndexEntry],
    ) -> Vec<FolderIndexSummaryDto> {
        let view = self.view();
        let running: Vec<(PathBuf, FolderIndexStatusDto)> = match self.live_builds().await {
            Ok(builds) => builds
                .iter()
                .filter(|job| job.status == JobStatus::Running)
                .filter_map(|job| {
                    Some((
                        PathBuf::from(BuildRequest::of(job)?.root),
                        build::activity_status(job)?,
                    ))
                })
                .collect(),
            Err(error) => {
                warn!(%error, "Could not read running folder builds");
                Vec::new()
            }
        };
        let mut summaries = Vec::with_capacity(roots.len());
        for root in roots {
            let live = match &view {
                Some(view) if &view.root == root => Some(view.status.get()),
                _ => running
                    .iter()
                    .find(|(running, _)| running == root)
                    .map(|(_, status)| status.clone()),
            };
            summaries.push(match live {
                Some(status) => {
                    let own = (status.index_root == status.root)
                        .then(|| store::index_dir(&self.config.base_dir, root));
                    live_summary(&status, own.as_deref())
                }
                None => self.summary_on_disk(root, indexes).await,
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
            if matches!(
                current.status.get().state,
                FolderIndexState::TooLarge
                    | FolderIndexState::Refused
                    | FolderIndexState::Unavailable
            ) {
                return None;
            }
            let index = current.index.as_ref()?;
            let engine = index.engine()?;
            FolderSearch::new(
                Arc::clone(&index.store),
                engine.vectors,
                engine.embedder,
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

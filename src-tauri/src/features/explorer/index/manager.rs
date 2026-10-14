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
use super::run::{Limits, StatusCell, StatusSink};
use super::store::{self, FolderStore, IndexEntry, OpenStamps, TooLarge};
use crate::application::ports::EmbeddingPort;
use crate::shared::{
    runtime::jobs::{JobRecord, JobRuntime, NewJob},
    Result,
};
use async_trait::async_trait;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Weak};
use std::time::Duration;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

mod folder;
mod summary;

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
    /// Stamps each opened folder, which the folders list orders by.
    open_stamps: OpenStamps,
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
            open_stamps: OpenStamps::default(),
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

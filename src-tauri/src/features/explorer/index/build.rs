//! A folder index build as a job: walk the folder, bring the rows in line
//! with it, and embed what is new. The job's progress is the share of
//! passages embedded; its activity is the whole status, rate and time left
//! included, which is what the renderer shows.

use super::dto::{FolderIndexState, FolderIndexStatusDto};
use super::manager::{relative_below, root_string, too_large_message, FolderIndexManager};
use super::run::{Indexer, Outcome, StatusCell};
use super::store::{self, Counts, FolderStore, TooLarge};
use crate::application::ports::vector_search_port::VectorSearchPort;
use crate::application::ports::EmbeddingPort;
use crate::features::explorer::scope::Scope;
use crate::features::search::engine::vector_search::USearchVectorIndex;
use crate::shared::{
    runtime::jobs::{JobContext, JobHandler, JobKindConfig, JobOutcome, JobRecord, RecoveryPolicy},
    AppError, Result,
};
use async_trait::async_trait;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Weak};
use std::time::{Duration, Instant};
use tokio::sync::watch;
use tracing::{info, warn};

pub const FOLDER_INDEX: &str = "explorer.folder_index";

/// The saved progress moves at most this often while counts move; a change of
/// state is saved at once.
const PROGRESS_INTERVAL: Duration = Duration::from_secs(1);

/// What a build was asked to do.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct BuildRequest {
    /// The folder walked.
    pub root: String,
    /// The folder whose index it writes: `root`, or an enclosing folder whose
    /// index `root` reuses.
    pub index_root: String,
    /// A rebuild of a sub-folder inside a reused index: its part is wiped
    /// first.
    #[serde(default)]
    pub wipe_prefix: bool,
}

impl BuildRequest {
    pub fn of(job: &JobRecord) -> Option<Self> {
        serde_json::from_value(job.requested.clone()).ok()
    }
}

/// Builds take turns, so the folder being looked at is embedded first rather
/// than sharing the model with every other folder; two builds of one index
/// never overlap. A build resumes after a restart: the index on disk is its
/// checkpoint.
pub(super) fn build_job_config() -> JobKindConfig {
    JobKindConfig::new(RecoveryPolicy::Requeue)
        .concurrency(1)
        .exclusive_per_subject()
}

/// The status a build last saved.
pub(super) fn activity_status(job: &JobRecord) -> Option<FolderIndexStatusDto> {
    serde_json::from_value(job.activity.clone()?).ok()
}

pub(super) fn initial_status(
    root: &Path,
    index_root: &Path,
    state: FolderIndexState,
    counts: Counts,
) -> FolderIndexStatusDto {
    let mut status = FolderIndexStatusDto::new(&root_string(root), &root_string(index_root), state);
    status.files_total = counts.files_total;
    status.files_indexed = counts.files_indexed;
    status.passages_total = counts.passages_total;
    status.passages_embedded = counts.passages_embedded;
    status
}

/// What a search or a build needs once the embedder has loaded.
#[derive(Clone)]
pub(super) struct Engine {
    pub vectors: Arc<USearchVectorIndex>,
    pub embedder: Arc<dyn EmbeddingPort>,
}

/// An index directory in use by the open folder, a build, or both. They share
/// its pool and its vectors, and take turns writing.
pub(super) struct LiveIndex {
    pub dir: PathBuf,
    pub store: Arc<FolderStore>,
    engine: RwLock<Option<Engine>>,
    /// Held by whatever writes: a build for its whole run, the open folder's
    /// watcher for one update.
    pub writing: tokio::sync::Mutex<()>,
}

impl LiveIndex {
    pub fn new(dir: PathBuf, store: Arc<FolderStore>) -> Self {
        Self {
            dir,
            store,
            engine: RwLock::new(None),
            writing: tokio::sync::Mutex::new(()),
        }
    }

    pub fn engine(&self) -> Option<Engine> {
        self.engine.read().clone()
    }

    /// The vectors for `embedder`, loaded once while the index is in use and
    /// again when the model changes. Call while holding `writing`.
    async fn prepare(&self, embedder: Arc<dyn EmbeddingPort>) -> Result<Engine> {
        if let Some(engine) = self
            .engine()
            .filter(|engine| engine.embedder.model_identity() == embedder.model_identity())
        {
            return Ok(engine);
        }
        let vectors = prepare_vectors(&self.store, &self.dir, embedder.as_ref()).await?;
        let engine = Engine { vectors, embedder };
        *self.engine.write() = Some(engine.clone());
        Ok(engine)
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

/// The vectors file for this embedder. A different identity than the one
/// recorded drops every vectors file and marks every file for embedding; so
/// does a vectors file that is missing or will not load.
async fn prepare_vectors(
    store: &FolderStore,
    dir: &Path,
    embedder: &dyn EmbeddingPort,
) -> Result<Arc<USearchVectorIndex>> {
    let identity = embedder.model_identity();
    let stored = store.meta(store::META_IDENTITY).await?;
    let path = store::vectors_path(dir, &identity);
    let keymap = path.with_extension("keymap.json");
    if stored.as_deref() != Some(identity.as_str()) || !path.exists() || !keymap.exists() {
        if stored.is_some() {
            info!(
                from = stored.as_deref().unwrap_or_default(),
                to = %identity,
                "Folder index: embedding model changed; embedding again"
            );
        }
        remove_vector_files(dir);
        store.reset_embedded().await?;
        store.set_meta(store::META_IDENTITY, &identity).await?;
    }
    let dimension = embedder.dimension();
    let load_path = path.clone();
    let loaded = tokio::task::spawn_blocking(move || {
        USearchVectorIndex::open_or_create(dimension, load_path)
    })
    .await
    .map_err(|error| AppError::InternalError(format!("Loading folder vectors failed: {error}")))?;
    let index = match loaded {
        Ok(index) => index,
        Err(error) => {
            warn!(%error, "Folder index: vectors file unreadable; embedding again");
            remove_vector_files(dir);
            store.reset_embedded().await?;
            USearchVectorIndex::open_or_create(dimension, path)?
        }
    };
    Ok(Arc::new(index.with_coalesced_saves()))
}

/// The words a build's saved progress carries.
fn progress_message(state: FolderIndexState) -> &'static str {
    match state {
        FolderIndexState::Scanning => "Scanning the folder",
        FolderIndexState::Indexing => "Embedding passages",
        FolderIndexState::Ready => "Ready",
        FolderIndexState::TooLarge => "Too large to index",
        FolderIndexState::Refused => "Not indexed",
        FolderIndexState::Unavailable => "No embedding model",
        FolderIndexState::Error => "Failed",
    }
}

/// Saves a build's status as its job's progress and activity.
struct Progress<'a> {
    context: &'a JobContext,
    /// Saved progress only moves forward; a scan that finds more passages
    /// lowers the share until embedding catches up.
    high: u32,
    last: Option<(Instant, FolderIndexState)>,
}

impl<'a> Progress<'a> {
    fn new(context: &'a JobContext) -> Self {
        Self {
            context,
            high: context.job().progress_current,
            last: None,
        }
    }

    async fn save(&mut self, status: &FolderIndexStatusDto, force: bool) -> Result<()> {
        let due = force
            || self.last.is_none_or(|(at, state)| {
                state != status.state || at.elapsed() >= PROGRESS_INTERVAL
            });
        if !due {
            return Ok(());
        }
        self.last = Some((Instant::now(), status.state));
        self.high = self.high.max(status.percent()).min(100);
        let activity = serde_json::to_value(status)?;
        self.context
            .progress(self.high, progress_message(status.state), Some(&activity))
            .await?;
        Ok(())
    }
}

pub(super) struct BuildJob {
    manager: Weak<FolderIndexManager>,
}

impl BuildJob {
    pub fn new(manager: Weak<FolderIndexManager>) -> Self {
        Self { manager }
    }
}

#[async_trait]
impl JobHandler for BuildJob {
    async fn run(&self, context: &JobContext) -> Result<JobOutcome> {
        let Some(manager) = self.manager.upgrade() else {
            return Ok(JobOutcome::Stopped);
        };
        let request = BuildRequest::of(context.job())
            .ok_or_else(|| AppError::InvalidData("A folder build without its folder".into()))?;
        // The open folder's build goes first; this one queues again behind it.
        if let Some(first) = manager.first_other_than(context.id()) {
            if manager
                .jobs
                .store()
                .get(&first)
                .await
                .is_ok_and(|job| !job.status.is_finished())
            {
                manager.jobs.requeue(context.id());
                return Ok(JobOutcome::Stopped);
            }
        }
        let root = PathBuf::from(&request.root);
        if !root.is_dir() {
            return Ok(JobOutcome::Failed {
                code: "folder_missing".into(),
                message: format!("{} no longer exists.", root.display()),
            });
        }
        let index_root = PathBuf::from(&request.index_root);
        let dir = manager.index_dir_of(&index_root);
        let index = manager.acquire(&index_root, &dir).await?;
        let build = Build {
            manager: &manager,
            context,
            request: &request,
            root: &root,
            index_root: &index_root,
            index: &index,
        };
        let outcome = build.run().await;
        manager.release(&index).await;
        outcome
    }
}

/// One attempt of a build.
struct Build<'a> {
    manager: &'a FolderIndexManager,
    context: &'a JobContext,
    request: &'a BuildRequest,
    root: &'a Path,
    index_root: &'a Path,
    index: &'a LiveIndex,
}

impl Build<'_> {
    async fn run(&self) -> Result<JobOutcome> {
        let prefix = relative_below(self.index_root, self.root).unwrap_or_default();
        let counts = self.index.store.counts(&prefix).await?;
        let status = Arc::new(StatusCell::new(initial_status(
            self.root,
            self.index_root,
            FolderIndexState::Scanning,
            counts,
        )));
        let (sink, mut updates) = watch::channel(status.get());
        let view = self.manager.view_handle();
        let root = self.root.to_path_buf();
        status.attach(Arc::new(move |snapshot: &FolderIndexStatusDto| {
            sink.send_replace(snapshot.clone());
            // The open folder shows its build's progress.
            if let Some(open) = view.lock().as_ref().filter(|open| open.root == root) {
                open.status.replace(snapshot.clone());
            }
        }));
        let mut progress = Progress::new(self.context);
        let outcome = self
            .embed(&prefix, &status, &mut updates, &mut progress)
            .await;
        status.detach();
        outcome
    }

    async fn embed(
        &self,
        prefix: &str,
        status: &Arc<StatusCell>,
        updates: &mut watch::Receiver<FolderIndexStatusDto>,
        progress: &mut Progress<'_>,
    ) -> Result<JobOutcome> {
        let context = self.context;
        let Some(_writing) = context.until_cancelled(self.index.writing.lock()).await else {
            return Ok(JobOutcome::Stopped);
        };
        let Some(embedder) = context
            .until_cancelled(self.manager.embedders.embedder())
            .await
        else {
            return Ok(JobOutcome::Stopped);
        };
        let Some(embedder) = embedder else {
            let message = "No embedding model is active, so the folder is searched by text only.";
            status.update(|status| {
                status.state = FolderIndexState::Unavailable;
                status.message = Some(message.into());
            });
            progress.save(&status.get(), true).await?;
            return Ok(JobOutcome::Failed {
                code: "unavailable".into(),
                message: message.into(),
            });
        };
        let prepared = async {
            let engine = self.index.prepare(embedder).await?;
            if self.request.wipe_prefix && !prefix.is_empty() {
                let removed = self.index.store.remove_under(prefix).await?;
                engine
                    .vectors
                    .remove_embeddings(&removed.iter().map(i64::to_string).collect::<Vec<_>>())?;
            }
            Ok::<_, AppError>(engine)
        }
        .await;
        let engine = match prepared {
            Ok(engine) => engine,
            Err(error) => return self.fail(status, progress, error).await,
        };
        let indexer = Indexer {
            store: Arc::clone(&self.index.store),
            vectors: engine.vectors,
            embedder: engine.embedder,
            scope: Scope::open(&root_string(self.index_root))?,
            prefix: prefix.to_string(),
            status: Arc::clone(status),
            cancel: context.cancellation().clone(),
            limits: self.manager.config().limits,
        };
        let run = indexer.run();
        tokio::pin!(run);
        let outcome = loop {
            tokio::select! {
                outcome = &mut run => break outcome,
                changed = updates.changed() => match changed {
                    Ok(()) => {
                        let snapshot = updates.borrow_and_update().clone();
                        progress.save(&snapshot, false).await?;
                    }
                    Err(_) => break (&mut run).await,
                },
            }
        };
        match outcome {
            Ok(Outcome::Done) => {
                status.update(|status| {
                    status.state = FolderIndexState::Ready;
                    status.message = None;
                    status.passages_per_second = None;
                    status.eta_seconds = None;
                });
                progress.save(&status.get(), true).await?;
                Ok(JobOutcome::Completed {
                    result_ref: root_string(&self.index.dir),
                    message: progress_message(FolderIndexState::Ready).into(),
                })
            }
            Ok(Outcome::TooLarge { count, capped }) => {
                let message = too_large_message(
                    TooLarge { count, capped },
                    self.manager.config().limits.max_files,
                );
                status.update(|status| {
                    status.state = FolderIndexState::TooLarge;
                    status.files_total = u32::try_from(count).unwrap_or(u32::MAX);
                    status.files_indexed = 0;
                    status.message = Some(message);
                });
                progress.save(&status.get(), true).await?;
                Ok(JobOutcome::Completed {
                    result_ref: root_string(&self.index.dir),
                    message: progress_message(FolderIndexState::TooLarge).into(),
                })
            }
            Ok(Outcome::Cancelled) => {
                let status = status.get();
                info!(
                    root = %self.root.display(),
                    embedded = status.passages_embedded,
                    total = status.passages_total,
                    "Folder index paused"
                );
                Ok(JobOutcome::Stopped)
            }
            Err(error) => self.fail(status, progress, error).await,
        }
    }

    async fn fail(
        &self,
        status: &StatusCell,
        progress: &mut Progress<'_>,
        error: AppError,
    ) -> Result<JobOutcome> {
        warn!(%error, root = %self.root.display(), "Folder index failed");
        status.update(|status| {
            status.state = FolderIndexState::Error;
            status.message = Some(error.to_string());
            status.passages_per_second = None;
            status.eta_seconds = None;
        });
        progress.save(&status.get(), true).await?;
        Err(error)
    }
}

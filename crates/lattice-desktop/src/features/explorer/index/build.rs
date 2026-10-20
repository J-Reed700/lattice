//! A folder index build as a job: walk the folder, bring the rows in line
//! with it, and embed what is new. The job's progress is the share of
//! passages embedded; its activity is the whole status, rate and time left
//! included, which is what the renderer shows.
//!
//! The job kind's configuration, the throttled progress saver and the live
//! index a build writes are the passage index's
//! (`crate::infrastructure::passage_index`); what a build is asked to do, the
//! open folder going first and the folder's own states stay here.

use super::dto::{FolderIndexState, FolderIndexStatusDto};
use super::manager::{relative_below, root_string, too_large_message, FolderIndexManager};
use super::run::{Indexer, Outcome, StatusCell};
use super::store::TooLarge;
use crate::features::explorer::scope::Scope;
use crate::infrastructure::passage_index::job::ProgressSaver;
use crate::infrastructure::passage_index::{Counts, LiveIndex};
use async_trait::async_trait;
use lattice_core::shared::{
    runtime::jobs::{JobContext, JobHandler, JobOutcome, JobRecord},
    AppError, Result,
};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Weak};
use tokio::sync::watch;
use tracing::{info, warn};

pub const FOLDER_INDEX: &str = "explorer.folder_index";

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
    status.files_total = counts.documents_total;
    status.files_indexed = counts.documents_embedded;
    status.passages_total = counts.passages_total;
    status.passages_embedded = counts.passages_embedded;
    status
}

/// The words a build's saved progress carries.
fn progress_message(state: FolderIndexState) -> &'static str {
    match state {
        FolderIndexState::Scanning => "Scanning the folder",
        FolderIndexState::Indexing => "Embedding passages",
        FolderIndexState::Ready => "Ready",
        FolderIndexState::TooLarge => "Partly indexed",
        FolderIndexState::Refused => "Not indexed",
        FolderIndexState::Unavailable => "No embedding model",
        FolderIndexState::Error => "Failed",
    }
}

/// Saves a build's status as its job's progress and activity.
struct Progress<'a> {
    saver: ProgressSaver<'a, FolderIndexState>,
}

impl<'a> Progress<'a> {
    fn new(context: &'a JobContext) -> Self {
        Self {
            saver: ProgressSaver::new(context),
        }
    }

    async fn save(&mut self, status: &FolderIndexStatusDto, force: bool) -> Result<()> {
        let activity = serde_json::to_value(status)?;
        self.saver
            .save(
                status.state,
                status.percent(),
                progress_message(status.state),
                &activity,
                force,
            )
            .await
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
                let keys = self.index.store.keys_under(prefix).await?;
                let removed = self.index.store.remove(&keys).await?;
                engine.vectors.remove(&removed)?;
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
            Ok(Outcome::TooLarge { count }) => {
                let message =
                    too_large_message(TooLarge { count }, self.manager.config().limits.max_files);
                status.update(|status| {
                    status.state = FolderIndexState::TooLarge;
                    status.files_total = u32::try_from(count).unwrap_or(u32::MAX);
                    status.message = Some(message);
                    status.passages_per_second = None;
                    status.eta_seconds = None;
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

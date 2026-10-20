//! Every course's index: where each lives, who holds it, the background
//! builds that keep it current, and its removal with its course.
//!
//! - Builds are jobs of kind `learning.course_index` on the app's job
//!   runtime, one at a time and never two of one course, separate from
//!   Explorer's folder builds so a course never queues behind a large folder.
//!   A build stopped by a restart resumes at the next start.
//! - A capture, adoption, reimport or deletion of a source queues a build,
//!   named by the course's set of active versions so repeats coalesce, unless
//!   a reader holds the course: its next `ensure_current` picks the change up.
//! - Row changes serialise on the index's `writing` lock, held only while
//!   rows change; embedding serialises on its `embedding` lock. A lesson job
//!   that finds a build embedding waits for it, which is the same work; a
//!   user's command takes `writing` alone and never waits for embedding.
//! - At startup, after the sweep, every course whose index has not recorded
//!   its current revision as embedded is queued, with the same coalescing
//!   build name as a capture's.
//! - The embedding model is the app's, shared with chat and Explorer through
//!   the composition root's `EmbedderSource`; the search is the library's
//!   orchestrator, built by the composition root's `HybridSearchFactory`.

use super::job::CourseIndexJob;
use super::search::{CourseSearch, IndexView};
use super::sync::{self, CourseProgress};
use super::{IndexMode, COURSE_INDEX};
use crate::features::learning::source_library::LearningSourceLibraryRepository;
use crate::infrastructure::passage_index::job::build_job_config;
use crate::infrastructure::passage_index::{registry, EmbedderSource, LiveIndex, LiveIndexes};
use lattice_core::application::ports::hybrid_search::HybridSearchFactory;
use lattice_core::shared::runtime::background::BackgroundTasks;
use lattice_core::shared::runtime::jobs::{JobRuntime, NewJob};
use lattice_core::shared::{AppError, Result};
use sqlx::SqlitePool;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

pub struct CourseIndexes {
    /// `<data_dir>/course-index`.
    base_dir: PathBuf,
    pool: SqlitePool,
    embedders: Arc<dyn EmbedderSource>,
    searches: Arc<dyn HybridSearchFactory>,
    jobs: Arc<JobRuntime>,
    /// Index directories open for readers and builds.
    live: LiveIndexes,
    /// How many readers hold each course.
    holders: parking_lot::Mutex<HashMap<String, usize>>,
    /// Where a reader's release of its index runs once the reader is gone.
    background: Arc<BackgroundTasks>,
}

impl CourseIndexes {
    pub fn new(
        base_dir: PathBuf,
        pool: SqlitePool,
        embedders: Arc<dyn EmbedderSource>,
        searches: Arc<dyn HybridSearchFactory>,
        jobs: Arc<JobRuntime>,
        background: Arc<BackgroundTasks>,
    ) -> Arc<Self> {
        Arc::new(Self {
            base_dir,
            pool,
            embedders,
            searches,
            jobs,
            live: LiveIndexes::default(),
            holders: parking_lot::Mutex::new(HashMap::new()),
            background,
        })
    }

    /// Registers the build job kind; builds the last process left running
    /// resume.
    pub async fn register(self: &Arc<Self>) -> Result<()> {
        self.jobs
            .register(
                COURSE_INDEX,
                Arc::new(CourseIndexJob::new(Arc::downgrade(self))),
                build_job_config(),
            )
            .await
    }

    fn repository(&self) -> LearningSourceLibraryRepository {
        LearningSourceLibraryRepository::new(self.pool.clone())
    }

    /// The directory of `program`'s index. A program id is an opaque UUID,
    /// so it names the directory as it is; anything else is refused, so no
    /// id can point outside the base.
    pub fn dir_of(&self, program: &str) -> Result<PathBuf> {
        uuid::Uuid::parse_str(program)
            .map_err(|_| AppError::InvalidInput("Invalid program ID".into()))?;
        Ok(self.base_dir.join(program))
    }

    /// `program`'s index for a reader, open until the reader drops it. Its
    /// view searches by text until the reader's first `ensure_current`.
    pub async fn search(
        self: &Arc<Self>,
        program: &str,
        cancel: CancellationToken,
    ) -> Result<CourseSearch> {
        let index = self.live.acquire(&self.dir_of(program)?).await?;
        let view = IndexView::of(&index.store, IndexMode::Lexical, None, &*self.searches);
        Ok(CourseSearch::new(
            Arc::clone(self),
            program.to_string(),
            index,
            cancel,
            view,
        ))
    }

    /// Brings `program`'s index in line with the course and embeds what is
    /// pending: the one step every build and reader runs.
    pub async fn ensure_current(
        &self,
        program: &str,
        cancel: &CancellationToken,
        progress: &mut dyn CourseProgress,
    ) -> Result<IndexMode> {
        let index = self.live.acquire(&self.dir_of(program)?).await?;
        let outcome = self.bring_current(&index, program, cancel, progress).await;
        self.live.release(&index).await;
        outcome.map(|view| view.mode)
    }

    /// One full run over an open index: rows brought in line, then every
    /// pending passage embedded. A run that finds another embedding (a
    /// background build) waits for it, which is doing the same work.
    pub(super) async fn bring_current(
        &self,
        index: &LiveIndex,
        program: &str,
        cancel: &CancellationToken,
        progress: &mut dyn CourseProgress,
    ) -> Result<IndexView> {
        let repo = self.repository();
        let Some(embedder) = self.embedders.embedder().await else {
            let _writing = locked(&index.writing, cancel).await?;
            let versions = sync::sync_rows(&repo, program, index, None, progress).await?;
            let mode = sync::settle(&index.store, None, program, &versions).await?;
            return Ok(IndexView::of(&index.store, mode, None, &*self.searches));
        };
        let _embedding = match index.embedding.try_lock() {
            Ok(embedding) => embedding,
            Err(_) => {
                progress.waiting();
                locked(&index.embedding, cancel).await?
            }
        };
        let (engine, versions) = {
            let _writing = locked(&index.writing, cancel).await?;
            // Prepared first: a new model drops the old vectors, so removals
            // apply to the vectors searches read.
            let engine = index.prepare(embedder).await?;
            let versions = sync::sync_rows(&repo, program, index, Some(&engine), progress).await?;
            (engine, versions)
        };
        sync::embed(&index.store, &engine, &versions, program, cancel, progress).await?;
        let mode = sync::settle(&index.store, Some(&engine), program, &versions).await?;
        Ok(IndexView::of(
            &index.store,
            mode,
            Some(engine),
            &*self.searches,
        ))
    }

    /// The text-only step, for a user's command: rows brought in line (new
    /// versions chunked, retired ones dropped), nothing embedded. What is
    /// already embedded is searched by meaning; a build is queued for the
    /// rest, and until it runs the mode says `Partial`. Never waits for
    /// embedding.
    pub(super) async fn bring_text_current(
        &self,
        index: &LiveIndex,
        program: &str,
    ) -> Result<IndexView> {
        let embedder = self.embedders.embedder().await;
        let (engine, versions) = {
            let _writing = index.writing.lock().await;
            // The vectors are loaded (or, for a new model, dropped) only while
            // no run is embedding; otherwise this reads the ones that run fills.
            let engine = match (embedder, index.embedding.try_lock()) {
                (Some(embedder), Ok(_embedding)) => Some(index.prepare(embedder).await?),
                (Some(_), Err(_)) => index.engine(),
                (None, _) => None,
            };
            let versions = sync::sync_rows(
                &self.repository(),
                program,
                index,
                engine.as_ref(),
                &mut sync::Silent,
            )
            .await?;
            (engine, versions)
        };
        let mode = sync::settle(&index.store, engine.as_ref(), program, &versions).await?;
        if matches!(mode, IndexMode::Partial { .. }) {
            if let Err(error) = self.queue_build(program, Rerun::Yes).await {
                warn!(%error, program, "Could not queue a course index build");
            }
        }
        Ok(IndexView::of(&index.store, mode, engine, &*self.searches))
    }

    /// A reader took `program`.
    pub(super) fn hold(&self, program: &str) {
        *self.holders.lock().entry(program.to_string()).or_default() += 1;
    }

    /// A reader let `program` go: its count drops now, and its hold on the
    /// open directory as soon as the runtime takes the release.
    pub(super) fn let_go(self: &Arc<Self>, program: &str, index: &Arc<LiveIndex>) {
        {
            let mut holders = self.holders.lock();
            if let Some(count) = holders.get_mut(program) {
                *count = count.saturating_sub(1);
                if *count == 0 {
                    holders.remove(program);
                }
            }
        }
        // Outside a runtime there is nothing to release into; the store's
        // pool closes when its last holder drops.
        if tokio::runtime::Handle::try_current().is_err() {
            return;
        }
        let courses = Arc::clone(self);
        let index = Arc::clone(index);
        self.background
            .spawn(async move { courses.live.release(&index).await });
    }

    fn held(&self, program: &str) -> bool {
        self.holders.lock().contains_key(program)
    }

    /// `program`'s sources changed: queue a build of its index, unless a
    /// reader holds it and will bring it current itself. Never fails its
    /// caller: the next reader brings the index current anyway.
    pub async fn refresh(&self, program: &str) {
        if self.held(program) {
            return;
        }
        if let Err(error) = self.queue_build(program, Rerun::No).await {
            warn!(%error, program, "Could not queue a course index build");
        }
    }

    /// Queues a build of `program` at its current revision.
    async fn queue_build(&self, program: &str, rerun: Rerun) -> Result<()> {
        self.dir_of(program)?;
        let versions = self.repository().active_versions(program).await?;
        self.queue_revision(program, sync::revision(&versions), rerun)
            .await
    }

    /// Queues a build of `program` named by `revision`, so asking again for
    /// the same revision is the same job. Finished builds of other revisions
    /// are forgotten, keeping one per course in the history; a finished build
    /// of this revision is forgotten too when the caller knows the index is
    /// still not current, so the build runs again.
    async fn queue_revision(&self, program: &str, revision: String, rerun: Rerun) -> Result<()> {
        let operation_id = format!("course-index:{program}:{revision}");
        for old in self
            .jobs
            .store()
            .list_for_subject(COURSE_INDEX, program)
            .await?
        {
            let forget = old.operation_id != operation_id || rerun == Rerun::Yes;
            if old.status.is_finished() && forget {
                if let Err(error) = self.jobs.store().delete(&old.id).await {
                    warn!(%error, job_id = %old.id, "Could not forget a course index build");
                }
            }
        }
        self.jobs
            .submit(&NewJob {
                kind: COURSE_INDEX.into(),
                subject_id: Some(program.to_string()),
                operation_id,
                payload_hash: revision,
                requested: serde_json::json!({ "programId": program }),
                progress_total: 100,
                message: "Queued".into(),
            })
            .await?;
        Ok(())
    }

    /// Queues a build for every course whose index is not current: a course
    /// with sources whose index has not recorded its revision as embedded.
    /// A current course queues nothing. Without an embedding model nothing
    /// is queued: a build would add nothing a reader's own step does not.
    /// Returns how many builds were queued; they run one at a time.
    pub async fn backfill(&self) -> Result<usize> {
        let recorded: HashMap<String, String> = registry::list(&self.base_dir)
            .await
            .into_iter()
            .filter_map(|entry| {
                let program = entry.dir.file_name()?.to_str()?.to_string();
                Some((program, entry.meta.get(sync::META_REVISION)?.clone()))
            })
            .collect();
        let programs: Vec<String> = sqlx::query_scalar("SELECT id FROM learning_programs")
            .fetch_all(&self.pool)
            .await
            .map_err(|error| AppError::Database(error.to_string()))?;
        let repo = self.repository();
        let mut stale = Vec::new();
        for program in programs {
            let versions = repo.active_versions(&program).await?;
            if versions.is_empty() {
                continue;
            }
            let revision = sync::revision(&versions);
            if recorded.get(&program) != Some(&revision) {
                stale.push((program, revision));
            }
        }
        if stale.is_empty() || self.embedders.embedder().await.is_none() {
            return Ok(0);
        }
        let mut queued = 0;
        for (program, revision) in stale {
            match self.queue_revision(&program, revision, Rerun::Yes).await {
                Ok(()) => queued += 1,
                Err(error) => warn!(%error, program, "Could not queue a course index build"),
            }
        }
        info!(
            queued,
            "Queued course index builds for courses not yet current"
        );
        Ok(queued)
    }

    /// The course is gone: its builds stop and are forgotten, and its
    /// directory is removed.
    pub async fn delete(&self, program: &str) {
        let Ok(dir) = self.dir_of(program) else {
            return;
        };
        match self
            .jobs
            .store()
            .list_for_subject(COURSE_INDEX, program)
            .await
        {
            Ok(builds) => {
                for job in builds {
                    if !job.status.is_finished() {
                        if let Err(error) = self.jobs.cancel(&job.id).await {
                            warn!(%error, job_id = %job.id, "Could not cancel a course index build");
                        }
                        self.jobs.finished(&job.id).await;
                    }
                    if let Err(error) = self.jobs.store().delete(&job.id).await {
                        warn!(%error, job_id = %job.id, "Could not forget a course index build");
                    }
                }
            }
            Err(error) => warn!(%error, program, "Could not list a course's index builds"),
        }
        remove_dir(&dir);
    }

    /// Removes `program`'s index directory and nothing else: for a build that
    /// finds its course gone, which must not wait for itself to stop.
    pub(super) fn remove_index(&self, program: &str) {
        if let Ok(dir) = self.dir_of(program) {
            remove_dir(&dir);
        }
    }

    /// Removes the index of every course that no longer exists: a course
    /// deleted while the app was not running, or whose directory a delete
    /// could not remove. Only directories named by a program id are touched.
    pub async fn sweep(&self) -> Result<()> {
        let Ok(entries) = std::fs::read_dir(&self.base_dir) else {
            return Ok(());
        };
        let programs: HashSet<String> = sqlx::query_scalar("SELECT id FROM learning_programs")
            .fetch_all(&self.pool)
            .await
            .map_err(|error| AppError::Database(error.to_string()))?
            .into_iter()
            .collect();
        for entry in entries.filter_map(|entry| entry.ok()) {
            let name = entry.file_name().to_string_lossy().into_owned();
            let is_index = entry.file_type().is_ok_and(|kind| kind.is_dir())
                && uuid::Uuid::parse_str(&name).is_ok();
            if is_index && !programs.contains(&name) {
                remove_dir(&entry.path());
            }
        }
        Ok(())
    }

    /// Whether `program` still exists.
    pub(super) async fn exists(&self, program: &str) -> Result<bool> {
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM learning_programs WHERE id=?")
            .bind(program)
            .fetch_one(&self.pool)
            .await
            .map(|count| count > 0)
            .map_err(|error| AppError::Database(error.to_string()))
    }

    /// Brings `program`'s index current with no one watching: for a test, or
    /// a caller that only needs the mode.
    #[cfg(test)]
    pub(in crate::features::learning) async fn ensure_current_quietly(
        &self,
        program: &str,
    ) -> Result<IndexMode> {
        self.ensure_current(program, &CancellationToken::new(), &mut sync::Silent)
            .await
    }
}

/// Whether a build may run again for a revision it already finished.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Rerun {
    /// No: the same revision is the same build (a capture asked again).
    No,
    /// Yes: the index is known not to be current at this revision.
    Yes,
}

/// `lock`, unless `cancel` fires first.
async fn locked<'a>(
    lock: &'a tokio::sync::Mutex<()>,
    cancel: &CancellationToken,
) -> Result<tokio::sync::MutexGuard<'a, ()>> {
    tokio::select! {
        guard = lock.lock() => Ok(guard),
        () = cancel.cancelled() => Err(AppError::InvalidState(
            "Stopped while waiting for the course's reference index".into(),
        )),
    }
}

/// Removes an index directory; one already gone is fine.
fn remove_dir(dir: &Path) {
    match std::fs::remove_dir_all(dir) {
        Ok(()) => info!(dir = %dir.display(), "Removed a course index"),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => warn!(dir = %dir.display(), %error, "Could not remove a course index"),
    }
}

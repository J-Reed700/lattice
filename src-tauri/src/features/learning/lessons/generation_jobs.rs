//! Learning's lesson-preparation handler for the job runtime. The runtime owns
//! delivery, cancellation, per-course exclusivity and shutdown; this handler
//! prepares, verifies and publishes the requested lessons.
use crate::features::learning::{
    curriculum_repository::{lesson_failure, LearningCurriculumRepository},
    dto::{LearningPreparation, LearningProgramStatus},
    repository::LearningRepository,
    service,
};
use crate::{
    application::ports::LLMPort,
    shared::{
        error::{AppError, Result},
        runtime::jobs::{JobContext, JobFailure, JobHandler, JobOutcome, JobRecord},
    },
};
use futures::future::BoxFuture;
use sqlx::SqlitePool;
use std::sync::Arc;

type ModelLoader = Arc<dyn Fn() -> BoxFuture<'static, Result<Arc<dyn LLMPort>>> + Send + Sync>;
type SourceRefresh = Arc<dyn Fn(String) -> BoxFuture<'static, Result<()>> + Send + Sync>;

/// The library search for document references, once the embedding model
/// it queries with is loaded; `None` leaves every reference to keywords.
type LibraryLoader = Arc<
    dyn Fn() -> BoxFuture<'static, Option<Arc<dyn crate::application::ports::LibraryPassagesPort>>>
        + Send
        + Sync,
>;

#[derive(Clone)]
pub(in crate::features::learning) struct LessonGenerationWorker {
    pub pool: SqlitePool,
    pub load_llm: ModelLoader,
    pub load_library: LibraryLoader,
    pub refresh_sources: SourceRefresh,
    pub research_web: Option<Arc<dyn crate::features::web::WebServiceTrait>>,
}

#[async_trait::async_trait]
impl JobHandler for LessonGenerationWorker {
    async fn run(&self, context: &JobContext) -> Result<JobOutcome> {
        let repo = LearningCurriculumRepository::new(self.pool.clone());
        // The worker is independent of the webview. Also tell macOS this
        // user-requested work must continue when the window loses focus.
        let _activity = crate::shared::runtime::user_activity::UserActivity::begin(
            "Preparing learning material",
        );
        // Preparation may perform network/model work. Dropping it closes the
        // request immediately. Publication stays outside the cancellable part,
        // so its transaction commits or rolls back whole before shutdown
        // releases the database.
        match context
            .until_cancelled(crate::features::learning::lesson_progress::run(
                context,
                self.prepare(&repo, context),
            ))
            .await
        {
            Some(Ok((revision, staged))) if !context.is_cancelled() => {
                repo.publish_prepared_lessons(context.id(), revision, &staged)
                    .await?;
                Ok(JobOutcome::Committed)
            }
            Some(Err(error)) => Err(error),
            _ => Ok(JobOutcome::Stopped),
        }
    }

    fn failure(&self, job: &JobRecord, error: &AppError) -> JobFailure {
        lesson_failure(job, error)
    }
}

impl LessonGenerationWorker {
    async fn prepare(
        &self,
        repo: &LearningCurriculumRepository,
        context: &JobContext,
    ) -> Result<(
        i64,
        Vec<(
            String,
            crate::features::learning::dto::PreparedLearningLesson,
        )>,
    )> {
        use crate::features::learning::curriculum::LearningGenerationJobKind;
        let job_id = context.id();
        let job = repo.job(job_id).await?;
        if job.kind != LearningGenerationJobKind::LessonPreparation {
            return Err(crate::shared::error::AppError::InvalidInput(
                "This worker currently accepts bounded lesson-preparation jobs only.".into(),
            ));
        }
        let requested_ids = context
            .job()
            .requested
            .get("lessonIds")
            .and_then(serde_json::Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(serde_json::Value::as_str)
                    .map(str::to_string)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let program_repo = LearningRepository::new(self.pool.clone());
        let mut program = program_repo.get(&job.program_id).await?;
        if !repo.job_content_is_current(job_id).await? {
            return Err(crate::shared::error::AppError::InvalidState(
                "Course content changed before generation resumed. Start preparation again for the updated course.".into(),
            ));
        }
        if program.summary.status != LearningProgramStatus::Active {
            return Err(crate::shared::error::AppError::InvalidState(
                "Only active programs can prepare lessons.".into(),
            ));
        }
        // Publication needs an accepted snapshot even when the renderer never
        // opened the plan view (including recovered background-only work).
        // This snapshots the already accepted outline; it approves no content.
        repo.seed_accepted(&job.program_id).await?;
        (self.refresh_sources)(job.program_id.clone()).await?;
        let had_sources = program_repo.has_source_history(&job.program_id).await?;
        program.sources = program_repo.verification_sources(&job.program_id).await?;
        if had_sources && program.sources.is_empty() {
            return Err(crate::shared::error::AppError::InvalidState(
                "No active source snapshots are available for lesson preparation.".into(),
            ));
        }
        let mut candidates = program
            .modules
            .iter()
            .flat_map(|m| m.lessons.iter())
            .filter(|lesson| {
                lesson.preparation == LearningPreparation::Outline && !lesson.completed
            })
            .collect::<Vec<_>>();
        if !requested_ids.is_empty() {
            if requested_ids.len() > 3 {
                return Err(crate::shared::error::AppError::InvalidInput(
                    "A preparation job may target at most three upcoming lessons.".into(),
                ));
            }
            candidates.retain(|lesson| requested_ids.contains(&lesson.id));
            if candidates.len() != requested_ids.len() {
                return Err(crate::shared::error::AppError::InvalidInput(
                    "A requested lesson is not an upcoming outline in this program.".into(),
                ));
            }
        } else {
            candidates.truncate(job.progress_total as usize);
        }
        if candidates.is_empty() || candidates.len() != job.progress_total as usize {
            return Err(crate::shared::error::AppError::InvalidInput(
                "The job must target one to three outline lessons.".into(),
            ));
        }
        if program.sources.is_empty() {
            return Err(AppError::InvalidInput("Add reference material in Sources before preparing a lesson. The MVP needs saved evidence to check its teaching.".into()));
        }
        let workspace =
            crate::features::learning::source_library::LearningSourceLibraryRepository::new(
                self.pool.clone(),
            )
            .workspace(&job.program_id)
            .await?;
        if workspace
            .sources
            .iter()
            .any(|s| s.active_version.as_ref().is_some_and(|v| v.truncated))
        {
            return Err(AppError::InvalidInput("A reference is incomplete or was saved as an old excerpt. Replace it in Sources with complete text before preparing lessons.".into()));
        }
        crate::features::learning::lesson_progress::stage(
            "Getting the reference search model ready",
        );
        let library = (self.load_library)().await;
        crate::features::learning::lesson_progress::stage(
            "Matching saved references to the library",
        );
        let mut references =
            crate::features::learning::reference_collection::ReferenceCollection::load(
                &self.pool,
                &job.program_id,
                &program.sources,
                library.as_deref(),
            )
            .await?;
        crate::features::learning::lesson_progress::stage("Getting the lesson model ready");
        let llm = (self.load_llm)().await?;
        crate::features::learning::lesson_progress::model(llm.model_name());
        // Own the targets so source snapshots can refresh between lessons.
        let candidates: Vec<_> = candidates.into_iter().cloned().collect();
        let mut staged = Vec::new();
        let mut completed_count = job.progress_completed;
        loop {
            staged.clear();
            for (index, lesson) in candidates.iter().enumerate() {
                crate::features::learning::lesson_progress::lesson(&lesson.title);
                let sources = program_repo.verification_sources(&job.program_id).await?;
                references = references
                    .reload(&self.pool, &job.program_id, &sources)
                    .await?;
                program.sources = sources;
                let saved = repo
                    .lesson_checkpoint(job_id, &lesson.id, "prepared-v1")
                    .await?
                    .and_then(|value| {
                        serde_json::from_value::<
                            crate::features::learning::dto::PreparedLearningLesson,
                        >(value)
                        .ok()
                    })
                    .filter(|prepared| {
                        prepared.verification.as_ref().is_some_and(|report| {
                            report.reusable(&lesson.id, prepared, &program.sources, llm.as_ref())
                        })
                    });
                let generated = if let Some(saved) = saved {
                    crate::features::learning::lesson_progress::stage(
                        "Resuming from a verified lesson checkpoint",
                    );
                    saved
                } else {
                    // Keep the large multi-stage future on the heap. In debug
                    // builds, nesting it inline through task-local scopes can
                    // exhaust a runtime worker's stack before the network call.
                    let preparation = Box::pin(
                        crate::features::learning::generation::prepare_lesson_with_references(
                            llm.as_ref(),
                            &program,
                            lesson,
                            &references,
                        ),
                    );
                    crate::features::learning::lesson_drafts::run(
                        repo,
                        job_id,
                        &lesson.id,
                        crate::features::learning::content_verification::research::run(
                            self.pool.clone(),
                            job.program_id.clone(),
                            format!("{}: {}", program.summary.goal, lesson.title),
                            self.research_web.clone(),
                            preparation,
                        ),
                    )
                    .await?
                };
                service::validate_prepared(&generated, &program)?;
                repo.save_lesson_checkpoint(
                    job_id,
                    &lesson.id,
                    "prepared-v1",
                    serde_json::to_value(&generated)?,
                )
                .await?;
                crate::features::learning::lesson_progress::checkpoint_saved();
                staged.push((lesson.id.clone(), generated));
                completed_count = completed_count.max(index as u32 + 1);
                crate::features::learning::lesson_progress::completed(completed_count);
                crate::features::learning::lesson_progress::stage("Saving verified lesson");
                if !context
                    .progress(
                        completed_count,
                        &format!("Prepared {} of {} lessons", index + 1, job.progress_total),
                        None,
                    )
                    .await?
                {
                    return Err(AppError::InvalidState(
                        "Generation job is no longer running".into(),
                    ));
                }
            }
            // Research for a later lesson may add evidence relevant to an earlier
            // one. Revisit only invalidated checkpoints before atomic publication.
            let sources = program_repo.verification_sources(&job.program_id).await?;
            if staged.iter().all(|(id, prepared)| {
                prepared
                    .verification
                    .as_ref()
                    .is_some_and(|report| report.reusable(id, prepared, &sources, llm.as_ref()))
            }) {
                break;
            }
        }
        crate::features::learning::lesson_progress::phase(
            crate::features::learning::lesson_progress::Phase::Publishing,
            "Publishing verified lesson material",
        );
        Ok((program.summary.revision, staged))
    }
}

/// Runs one delivery of a saved job through a runtime of its own, as the app
/// would after a start.
#[cfg(test)]
async fn run_attempt(worker: LessonGenerationWorker, job_id: &str) -> Result<()> {
    use crate::{
        features::learning::curriculum_repository::{lesson_job_config, LESSON_PREPARATION},
        shared::runtime::jobs::JobRuntime,
    };
    let runtime = JobRuntime::new(worker.pool.clone());
    runtime
        .register(LESSON_PREPARATION, Arc::new(worker), lesson_job_config())
        .await?;
    runtime.dispatch(job_id);
    runtime.finished(job_id).await;
    runtime.close();
    runtime.drain().await;
    Ok(())
}

#[cfg(test)]
mod repository_tests {
    use super::*;
    use crate::features::learning::{
        curriculum::{LearningGenerationJobKind, LearningGenerationJobStatus},
        curriculum_repository::{lesson_job_config, LESSON_PREPARATION},
        dto::AcceptLearningProgramRequestDto,
        plan_dto::{LearningGenerationJobActionRequestDto, StartLearningGenerationJobRequestDto},
    };
    use crate::shared::runtime::jobs::{JobRuntime, JobStore, RecoveryPolicy};

    /// The old one-job-at-a-time semaphore did two things: it limited model
    /// use, which is now the inference scheduler's, and it kept two
    /// preparations of one course off the same sources and snapshot, which the
    /// runtime's per-subject lock still does — for that course only.
    #[test]
    fn lesson_jobs_run_one_per_course_and_resume_after_a_restart() {
        let config = lesson_job_config();
        assert!(config.exclusive_per_subject);
        assert_eq!(config.recovery, RecoveryPolicy::Requeue);
        assert_eq!(config.concurrency, tokio::sync::Semaphore::MAX_PERMITS);
    }

    pub(super) async fn queued_job(
        pool: &SqlitePool,
    ) -> (
        LearningCurriculumRepository,
        crate::features::learning::curriculum::LearningGenerationJob,
    ) {
        let program_repo = LearningRepository::new(pool.clone());
        let program = crate::features::learning::tests::fixture();
        let references: Vec<_> = program
            .sources
            .iter()
            .map(
                |source| crate::features::learning::sources::InitialReference {
                    source_id: source.id.clone(),
                    origin: source.id.clone(),
                    captured: crate::features::learning::source_library::CapturedLearningSource {
                        title: source.title.clone(),
                        publisher: None,
                        requested_url: None,
                        resolved_url: None,
                        text: source.excerpt.clone(),
                        truncated: false,
                        extraction_version: "fixture_complete".into(),
                    },
                },
            )
            .collect();
        program_repo
            .create_with_references(&program, &references)
            .await
            .unwrap();
        program_repo
            .accept(&AcceptLearningProgramRequestDto {
                program_id: program.summary.id.clone(),
                expected_revision: 0,
                title: program.summary.title.clone(),
            })
            .await
            .unwrap();
        let program = program_repo.get(&program.summary.id).await.unwrap();
        let repo = LearningCurriculumRepository::new(pool.clone());
        let job = repo
            .start_lesson_job(&StartLearningGenerationJobRequestDto {
                operation_id: uuid::Uuid::new_v4().to_string(),
                program_id: program.summary.id,
                expected_revision: program.summary.revision,
                kind: LearningGenerationJobKind::LessonPreparation,
                request_json: serde_json::json!({"lessonIds":[program.modules[0].lessons[0].id]})
                    .to_string(),
                progress_total: 1,
            })
            .await
            .unwrap();
        (repo, job)
    }

    // Block model loading, cancel it while in flight, and verify the worker
    // finishes its durable terminal transition without waiting for inference.
    #[tokio::test]
    async fn shutdown_and_user_cancellation_stop_running_work() {
        for user_cancel in [false, true] {
            let pool = crate::features::learning::tests::pool().await.unwrap();
            let (repo, job) = queued_job(&pool).await;
            let count: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM learning_curriculum_revisions WHERE program_id=?",
            )
            .bind(&job.program_id)
            .fetch_one(&pool)
            .await
            .unwrap();
            assert_eq!(count, 0, "No renderer plan request should be needed");
            let entered = Arc::new(tokio::sync::Notify::new());
            let notification = entered.clone();
            let worker = LessonGenerationWorker {
                research_web: None,
                load_library: Arc::new(|| Box::pin(async { None })),
                pool: pool.clone(),
                refresh_sources: Arc::new(|_| Box::pin(async { Ok(()) })),
                load_llm: Arc::new(move || {
                    let entered = notification.clone();
                    Box::pin(async move {
                        entered.notify_one();
                        std::future::pending().await
                    })
                }),
            };
            let runtime = JobRuntime::new(pool.clone());
            runtime
                .register(LESSON_PREPARATION, Arc::new(worker), lesson_job_config())
                .await
                .unwrap();
            runtime.dispatch(&job.id);
            tokio::time::timeout(std::time::Duration::from_secs(5), entered.notified())
                .await
                .unwrap();
            let count: i64 = sqlx::query_scalar("SELECT count(*) FROM learning_curriculum_revisions WHERE program_id=? AND status='accepted'")
                .bind(&job.program_id).fetch_one(&pool).await.unwrap();
            assert_eq!(count, 1, "Initialize the accepted outline before inference");
            assert!(repo.job_content_is_current(&job.id).await.unwrap());
            if user_cancel {
                repo.cancel_job(&LearningGenerationJobActionRequestDto {
                    operation_id: uuid::Uuid::new_v4().to_string(),
                    program_id: job.program_id.clone(),
                    job_id: job.id.clone(),
                    expected_revision: i64::from(job.base_revision_number),
                })
                .await
                .unwrap();
                runtime.cancelled(&job.id).await;
            } else {
                runtime.close();
            }
            tokio::time::timeout(std::time::Duration::from_secs(5), runtime.finished(&job.id))
                .await
                .unwrap();
            runtime.close();
            runtime.drain().await;
            let interrupted = repo.job(&job.id).await.unwrap();
            assert_eq!(
                interrupted.status,
                if user_cancel {
                    LearningGenerationJobStatus::Cancelled
                } else {
                    LearningGenerationJobStatus::Pending
                }
            );
            assert_eq!(interrupted.finished_at.is_some(), user_cancel);
            assert!(interrupted.result_id.is_none());
            let store = JobStore::new(pool.clone());
            store
                .recover(LESSON_PREPARATION, RecoveryPolicy::Requeue)
                .await
                .unwrap();
            assert_eq!(repo.job(&job.id).await.unwrap().status, interrupted.status);
            assert_eq!(
                store
                    .due(LESSON_PREPARATION)
                    .await
                    .unwrap()
                    .iter()
                    .any(|pending| pending.id == job.id),
                !user_cancel
            );
            let (_, pending) = queued_job(&pool).await;
            store.interrupt(&pending.id).await.unwrap();
            assert_eq!(
                repo.job(&pending.id).await.unwrap().status,
                LearningGenerationJobStatus::Pending
            );
        }
    }

    #[tokio::test]
    async fn shutdown_finalization_does_not_overwrite_user_cancellation() {
        let pool = crate::features::learning::tests::pool().await.unwrap();
        let (repo, job) = queued_job(&pool).await;
        let store = JobStore::new(pool.clone());
        assert!(store.claim(&job.id).await.unwrap().is_some());
        repo.cancel_job(&LearningGenerationJobActionRequestDto {
            operation_id: uuid::Uuid::new_v4().to_string(),
            program_id: job.program_id.clone(),
            job_id: job.id.clone(),
            expected_revision: i64::from(job.base_revision_number),
        })
        .await
        .unwrap();
        store.suspend(&job.id).await.unwrap();
        store
            .fail(&job.id, "generation_failed", "late inference failure")
            .await
            .unwrap();
        let cancelled = repo.job(&job.id).await.unwrap();
        assert_eq!(cancelled.status, LearningGenerationJobStatus::Cancelled);
        assert!(store.claim(&job.id).await.unwrap().is_none());
        assert!(cancelled.result_id.is_none());
    }
}

#[cfg(test)]
#[path = "live_lesson_tests.rs"]
mod live_lesson_tests;

#[cfg(test)]
#[path = "generation_recovery_tests.rs"]
mod recovery_tests;

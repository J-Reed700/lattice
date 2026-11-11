//! Learning's lesson-preparation handler for the job runtime. The runtime owns
//! delivery, cancellation, per-course exclusivity and shutdown; this handler
//! prepares, verifies and publishes the requested lessons.
use crate::{
    content_verification::Rounds,
    curriculum_repository::{lesson_failure, LearningCurriculumRepository},
    dto::{LearningPreparation, LearningProgramStatus},
    model_requirements,
    repository::LearningRepository,
    service,
};
use futures::future::BoxFuture;
use lattice_core::shared::hash::sha256_hex;
use lattice_core::{
    application::ports::llm_port::PinnedModel,
    shared::{
        error::{AppError, Result},
        persistence::Database,
        runtime::jobs::{JobContext, JobFailure, JobHandler, JobOutcome, JobRecord},
    },
};
#[cfg(test)]
use sqlx::SqlitePool;
use std::sync::Arc;

/// The Learning model, pinned for the whole job.
type ModelLoader = Arc<dyn Fn() -> BoxFuture<'static, Result<PinnedModel>> + Send + Sync>;
type SourceRefresh = Arc<dyn Fn(String) -> BoxFuture<'static, Result<()>> + Send + Sync>;

/// `llm` pinned as the Learning model, for workers a test builds.
#[cfg(any(test, feature = "test-support"))]
pub fn pinned_for_test(llm: Arc<dyn lattice_core::application::ports::LLMPort>) -> PinnedModel {
    use lattice_core::application::ports::llm_port::{ModelOrigin, ResolvedModel};
    use lattice_core::domain::models::model_ref::ModelRef;
    PinnedModel {
        resolved: ResolvedModel {
            model: ModelRef::Local {
                model_id: llm.model_name().to_string(),
            },
            origin: ModelOrigin::LearningRole,
        },
        llm,
        instance_id: 0,
        degraded: None,
    }
}

/// A verified lesson with the digest of the course plan it was written from,
/// saved together so a retry never pairs a lesson with a plan it was not
/// written from.
#[derive(serde::Serialize, serde::Deserialize)]
struct PreparedCheckpoint {
    plan: String,
    lesson: crate::dto::PreparedLearningLesson,
}

fn plan_digest(plan: &crate::lessons::course_plan::CoursePlan) -> Result<String> {
    Ok(sha256_hex(serde_json::to_string(plan)?))
}

#[derive(Clone)]
pub struct LessonGenerationWorker {
    pub database: Database,
    /// Where a lesson's worked examples run while it is verified.
    pub runtimes: crate::embedded_runtime::BuiltinRuntimes,
    pub load_llm: ModelLoader,
    /// Every course's evidence index; `None` searches each lesson's
    /// references by their words from memory, which the stage text says.
    pub courses: Option<Arc<crate::references::course_index::CourseIndexes>>,
    pub refresh_sources: SourceRefresh,
    pub research_web: Option<Arc<dyn lattice_core::application::ports::WebServiceTrait>>,
    /// The Verifier, for courses that opted into its audit (§14.3); `None`
    /// publishes every lesson without the audit gate.
    pub verifier: Option<Arc<dyn crate::lessons::verifier_audit::lesson::VerifierAccess>>,
    /// Shared by every lesson job this worker runs.
    pub course_setups: crate::lessons::course_setup::CourseSetups,
}

#[async_trait::async_trait]
impl JobHandler for LessonGenerationWorker {
    async fn run(&self, context: &JobContext) -> Result<JobOutcome> {
        let repo = LearningCurriculumRepository::new(self.database.clone());
        // The worker is independent of the webview. Also tell macOS this
        // user-requested work must continue when the window loses focus.
        let _activity = lattice_core::shared::runtime::user_activity::UserActivity::begin(
            "Preparing learning material",
        );
        // Preparation may perform network/model work. Dropping it closes the
        // request immediately. Publication stays outside the cancellable part,
        // so its transaction commits or rolls back whole before shutdown
        // releases the database.
        let mut rounds = Rounds::default();
        loop {
            let prepared = match context
                .until_cancelled(crate::lesson_progress::run(
                    context,
                    self.prepare(&repo, context, &mut rounds),
                ))
                .await
            {
                Some(Ok(prepared)) if !context.is_cancelled() => prepared,
                Some(Err(error)) => return Err(error),
                _ => return Ok(JobOutcome::Stopped),
            };
            // Every lesson waits for the user: nothing to publish, and the
            // job completed, not failed (§6.1).
            if prepared.staged.is_empty() {
                let (result_ref, message) =
                    crate::content_verification::review_outcome(&prepared.held);
                return Ok(JobOutcome::Completed {
                    result_ref,
                    message,
                });
            }
            match repo
                .publish_prepared(
                    context.id(),
                    prepared.revision,
                    &prepared.staged,
                    &prepared.held,
                    Some(prepared.model.as_str()),
                )
                .await
            {
                // A source a lesson was verified against changed between
                // verification and publication: verify again (saved checks are
                // reused) once, and fail as found the second time (§12).
                Err(error) if crate::content_verification::sources_changed(&error) => {
                    tracing::info!(job_id = context.id(), %error, "Lesson references changed before publication; verifying again");
                    rounds.publishing_failed(error)?;
                }
                Err(error) => return Err(error),
                Ok(()) => return Ok(JobOutcome::Committed),
            }
        }
    }

    fn failure(&self, job: &JobRecord, error: &AppError) -> JobFailure {
        lesson_failure(job, error)
    }
}

/// What a delivery prepared: the lessons to publish, and the lessons that
/// wait for the user's review (§6.1).
struct Prepared {
    revision: i64,
    staged: Vec<(String, crate::dto::PreparedLearningLesson)>,
    held: Vec<crate::content_verification::HeldForReview>,
    /// The Learning model the lessons were prepared with.
    model: String,
}

impl LessonGenerationWorker {
    /// What a lesson does to the course's shared state before it reads it,
    /// one lesson of the course at a time.
    pub(crate) async fn set_up_course(
        &self,
        repo: &LearningCurriculumRepository,
        program_id: &str,
    ) -> Result<()> {
        let _setup = self.course_setups.enter(program_id).await;
        // Publication needs an accepted snapshot even when the renderer never
        // opened the plan view (including recovered background-only work).
        // This snapshots the already accepted outline; it approves no content.
        repo.seed_accepted(program_id).await?;
        (self.refresh_sources)(program_id.to_owned()).await?;
        // Pages the admission gate refuses leave retrieval and citation before
        // anything is compared with them (claim classes design §3.6).
        crate::lesson_progress::stage("Checking that the course's saved pages are sources");
        let refused =
            crate::source_library::LearningSourceLibraryRepository::new(self.database.clone())
                .sweep_admission(program_id)
                .await?;
        if !refused.is_empty() {
            tracing::info!(
                program_id,
                pages = refused.len(),
                "Saved pages that are not sources are left out of retrieval"
            );
        }
        Ok(())
    }

    async fn prepare(
        &self,
        repo: &LearningCurriculumRepository,
        context: &JobContext,
        rounds: &mut Rounds,
    ) -> Result<Prepared> {
        use crate::curriculum::LearningGenerationJobKind;
        let job_id = context.id();
        let job = repo.job(job_id).await?;
        if job.kind != LearningGenerationJobKind::LessonPreparation {
            return Err(lattice_core::shared::error::AppError::InvalidInput(
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
        let program_repo = LearningRepository::new(self.database.clone());
        let mut program = program_repo.get(&job.program_id).await?;
        if !repo.job_content_is_current(job_id).await? {
            return Err(lattice_core::shared::error::AppError::InvalidState(
                "Course content changed before generation resumed. Start preparation again for the updated course.".into(),
            ));
        }
        if program.summary.status != LearningProgramStatus::Active {
            return Err(lattice_core::shared::error::AppError::InvalidState(
                "Only active programs can prepare lessons.".into(),
            ));
        }
        let outline_revision =
            crate::lessons::course_plan::outline_revision(&self.database, &job.program_id).await?;
        self.set_up_course(repo, &job.program_id).await?;
        let had_sources = program_repo.has_source_history(&job.program_id).await?;
        program.sources = program_repo.verification_sources(&job.program_id).await?;
        if had_sources && program.sources.is_empty() {
            return Err(lattice_core::shared::error::AppError::InvalidState(
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
                return Err(lattice_core::shared::error::AppError::InvalidInput(
                    "A preparation job may target at most three upcoming lessons.".into(),
                ));
            }
            candidates.retain(|lesson| requested_ids.contains(&lesson.id));
            if candidates.len() != requested_ids.len() {
                return Err(lattice_core::shared::error::AppError::InvalidInput(
                    "A requested lesson is not an upcoming outline in this program.".into(),
                ));
            }
        } else {
            candidates.truncate(job.progress_total as usize);
        }
        if candidates.is_empty() || candidates.len() != job.progress_total as usize {
            return Err(lattice_core::shared::error::AppError::InvalidInput(
                "The job must target one to three outline lessons.".into(),
            ));
        }
        if program.sources.is_empty() {
            return Err(AppError::InvalidInput("Add reference material in Sources before preparing a lesson. Lattice checks what it teaches against saved sources.".into()));
        }
        let workspace =
            crate::source_library::LearningSourceLibraryRepository::new(self.database.clone())
                .workspace(&job.program_id)
                .await?;
        if workspace
            .sources
            .iter()
            .any(|s| s.active_version.as_ref().is_some_and(|v| v.truncated))
        {
            return Err(AppError::InvalidInput("A reference is incomplete or was saved as an old excerpt. Replace it in Sources with complete text before preparing lessons.".into()));
        }
        // The course's index, held for the whole job and brought up to date
        // before each lesson reads it, so captures meanwhile are picked up.
        let course = match &self.courses {
            Some(courses) => courses
                .search(&job.program_id, context.cancellation().clone())
                .await
                .inspect_err(|error| tracing::warn!(%error, "Course index could not be opened"))
                .ok(),
            None => None,
        };
        crate::lesson_progress::stage("Getting the lesson model ready");
        // Held until every lesson is prepared: a model chosen meanwhile never
        // reaches this job.
        let pin = (self.load_llm)().await?;
        let llm = Arc::clone(&pin.llm);
        crate::lesson_progress::model(llm.model_name());
        // Refused once, before any lesson research or model call.
        model_requirements::ensure_window(
            llm.as_ref(),
            &model_requirements::lesson_preparation(),
            &format!("Preparing '{}'", program.summary.title),
        )?;
        // Own the targets so source snapshots can refresh between lessons.
        let candidates: Vec<_> = candidates.into_iter().cloned().collect();
        let mut staged = Vec::new();
        let mut held: Vec<crate::content_verification::HeldForReview> = Vec::new();
        let mut completed_count = job.progress_completed;
        loop {
            staged.clear();
            for (index, lesson) in candidates.iter().enumerate() {
                if held.iter().any(|hold| hold.lesson_id == lesson.id) {
                    continue;
                }
                crate::lesson_progress::lesson(&lesson.title);
                let sources = program_repo.verification_sources(&job.program_id).await?;
                program.sources = sources;
                let current_plan =
                    crate::lessons::course_plan::stored(&self.database, &program, outline_revision)
                        .await?
                        .map(|plan| plan_digest(&plan))
                        .transpose()?;
                let saved = repo
                    .lesson_checkpoint(job_id, &lesson.id, "prepared-v2")
                    .await?
                    .and_then(|value| serde_json::from_value::<PreparedCheckpoint>(value).ok())
                    .filter(|saved| current_plan.as_deref() == Some(saved.plan.as_str()))
                    .filter(|saved| {
                        saved.lesson.verification.as_ref().is_some_and(|report| {
                            report.reusable(
                                &lesson.id,
                                &saved.lesson,
                                &program.sources,
                                llm.as_ref(),
                            )
                        })
                    });
                let (generated, used_plan) = if let Some(saved) = saved {
                    crate::lesson_progress::stage("Resuming from a verified lesson checkpoint");
                    (saved.lesson, saved.plan)
                } else {
                    // Made once for the outline revision and read by every
                    // lesson after it: a lesson never waits on another's output.
                    let plan = crate::lessons::course_plan::ensure(
                        &self.database,
                        llm.as_ref(),
                        &program,
                        outline_revision,
                        &self.course_setups,
                    )
                    .await?;
                    // Before every lesson it prepares, so references captured
                    // meanwhile (by the user or by research) are searched too.
                    crate::lesson_progress::stage(
                        "Bringing the course's reference index up to date",
                    );
                    let references = crate::reference_collection::ReferenceCollection::current(
                        course.as_ref(),
                        &program.sources,
                    )
                    .await?;
                    // Keep the large multi-stage future on the heap. In debug
                    // builds, nesting it inline through task-local scopes can
                    // exhaust a runtime worker's stack before the network call.
                    let preparation = Box::pin(crate::generation::prepare_lesson_with_references(
                        llm.as_ref(),
                        &self.runtimes,
                        &program,
                        lesson,
                        &plan,
                        &references,
                    ));
                    let topic = format!("{}: {}", program.summary.goal, lesson.title);
                    let prepared = crate::lesson_drafts::run(
                        &self.database,
                        repo,
                        job_id,
                        &lesson.id,
                        crate::content_verification::research::run(
                            self.database.clone(),
                            job.program_id.clone(),
                            topic.clone(),
                            self.research_web.clone(),
                            // Source provenance and trust weigh every
                            // supported claim's support (§6.2).
                            crate::content_verification::with_trust(
                                self.database.clone(),
                                job.program_id.clone(),
                                topic,
                                crate::lessons::verifier_audit::lesson::scope(
                                    self.verifier.clone(),
                                    preparation,
                                ),
                            ),
                        ),
                    )
                    .await;
                    let used_plan = plan_digest(&plan)?;
                    let written = match prepared {
                        Ok(prepared) => prepared,
                        Err(error) => {
                            // A lesson whose claims wait for the user is held,
                            // not failed; the others still publish (§6.1).
                            let hold = crate::content_verification::held_for_review(
                                &self.database,
                                &lesson.id,
                                job_id,
                            )
                            .await?;
                            let Some(hold) = hold else {
                                return Err(error);
                            };
                            tracing::info!(lesson_id = %lesson.id, job_id, claims = hold.claims, "lesson.needs_review");
                            held.push(hold);
                            completed_count = completed_count.max(index as u32 + 1);
                            crate::lesson_progress::completed(completed_count);
                            continue;
                        }
                    };
                    (written, used_plan)
                };
                // Research before writing may have saved references this
                // lesson cites.
                program.sources = program_repo.verification_sources(&job.program_id).await?;
                service::validate_generated(&generated, &program)?;
                repo.save_lesson_checkpoint(
                    job_id,
                    &lesson.id,
                    "prepared-v2",
                    serde_json::to_value(PreparedCheckpoint {
                        plan: used_plan,
                        lesson: generated.clone(),
                    })?,
                )
                .await?;
                crate::lesson_progress::checkpoint_saved();
                staged.push((lesson.id.clone(), generated));
                completed_count = completed_count.max(index as u32 + 1);
                crate::lesson_progress::completed(completed_count);
                let unverified = staged
                    .last()
                    .and_then(|(_, lesson)| lesson.verification.as_ref())
                    .map_or(0, |report| report.unverified_count());
                crate::lesson_progress::stage(
                    crate::content_verification::not_verified_line(unverified).map_or_else(
                        || "Saving verified lesson".to_owned(),
                        |line| format!("Lesson ready · {line}"),
                    ),
                );
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
            // A source added meanwhile, by research for a later lesson or by the
            // user, never invalidates a lesson prepared before it: only a source
            // its report is bound to, gone or changed, does, and that sends the
            // lesson round once more at most (claim classes design §3.2).
            let sources = program_repo.verification_sources(&job.program_id).await?;
            if !rounds.again(&staged, &sources, llm.as_ref())? {
                break;
            }
        }
        crate::lesson_progress::phase(
            crate::lesson_progress::Phase::Publishing,
            "Publishing verified lesson material",
        );
        Ok(Prepared {
            revision: program.summary.revision,
            staged,
            held,
            model: llm.model_name().to_owned(),
        })
    }
}

/// Runs one delivery of a saved job through a runtime of its own, as the app
/// would after a start.
#[cfg(any(test, feature = "test-support"))]
pub async fn run_attempt(worker: LessonGenerationWorker, job_id: &str) -> Result<()> {
    use crate::curriculum_repository::{lesson_job_config, LESSON_PREPARATION};
    use lattice_core::shared::runtime::jobs::JobRuntime;
    let runtime = JobRuntime::new(worker.database.clone());
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
    use crate::{
        curriculum::{LearningGenerationJobKind, LearningGenerationJobStatus},
        curriculum_repository::{lesson_job_config, LESSON_PREPARATION},
        dto::AcceptLearningProgramRequestDto,
        plan_dto::{LearningGenerationJobActionRequestDto, StartLearningGenerationJobRequestDto},
    };
    use lattice_core::shared::runtime::jobs::{JobRuntime, JobStore, RecoveryPolicy};

    /// Model use is the inference scheduler's to limit; the course's setup lock
    /// keeps the lessons of one course off the same sources and snapshot.
    #[test]
    fn lesson_jobs_run_side_by_side_and_resume_after_a_restart() {
        let config = lesson_job_config();
        assert!(!config.exclusive_per_subject);
        assert_eq!(config.recovery, RecoveryPolicy::Requeue);
        assert_eq!(config.concurrency, tokio::sync::Semaphore::MAX_PERMITS);
    }

    pub(super) async fn queued_job(
        pool: &SqlitePool,
    ) -> (
        LearningCurriculumRepository,
        crate::curriculum::LearningGenerationJob,
    ) {
        queued_job_with(pool, true).await
    }

    /// A course the learner has not added any reference material to.
    pub(super) async fn queued_job_without_sources(
        pool: &SqlitePool,
    ) -> (
        LearningCurriculumRepository,
        crate::curriculum::LearningGenerationJob,
    ) {
        queued_job_with(pool, false).await
    }

    async fn queued_job_with(
        pool: &SqlitePool,
        with_sources: bool,
    ) -> (
        LearningCurriculumRepository,
        crate::curriculum::LearningGenerationJob,
    ) {
        let program_repo = LearningRepository::new(pool.clone());
        let mut program = crate::tests::fixture();
        if !with_sources {
            program.sources.clear();
        }
        let references: Vec<_> = program
            .sources
            .iter()
            .map(|source| crate::sources::InitialReference {
                source_id: source.id.clone(),
                origin: source.id.clone(),
                captured: crate::source_library::CapturedLearningSource {
                    title: source.title.clone(),
                    publisher: None,
                    requested_url: None,
                    resolved_url: None,
                    text: source.excerpt.clone(),
                    truncated: false,
                    extraction_version: "fixture_complete".into(),
                    signals: crate::references::trust::capture::not_extracted("test fixture"),
                },
                found: false,
            })
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
            let pool = crate::tests::pool().await.unwrap();
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
                course_setups: Default::default(),
                runtimes: crate::embedded_runtime::BuiltinRuntimes::for_tests(),
                research_web: None,
                verifier: None,
                courses: None,
                database: pool.clone().into(),
                refresh_sources: Arc::new(|_| Box::pin(async { Ok(()) })),
                load_llm: Arc::new(move || {
                    let entered = notification.clone();
                    Box::pin(async move {
                        entered.notify_one();
                        std::future::pending().await
                    })
                }),
            };
            let runtime = JobRuntime::new(Database::from(pool.clone()));
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
        let pool = crate::tests::pool().await.unwrap();
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
#[path = "generation_recovery_tests.rs"]
mod recovery_tests;

#[cfg(test)]
#[path = "generation_plan_tests.rs"]
mod plan_tests;

#[cfg(test)]
#[path = "generation_overlap_tests.rs"]
pub(crate) mod overlap_tests;

#[cfg(test)]
#[path = "generation_setup_tests.rs"]
mod setup_tests;

#[cfg(test)]
#[path = "generation_side_by_side_tests.rs"]
mod side_by_side_tests;

use super::*;

#[tauri::command]
#[specta::specta]
pub async fn start_learning_generation_job(
    request: StartLearningGenerationJobRequestDto,
    container: State<'_, Container>,
) -> Result<LearningGenerationJob, ApiError> {
    let repo = crate::features::learning::curriculum_repository::LearningCurriculumRepository::new(
        container.database().clone(),
    );
    let job = repo.start_job(&request).await.map_err(ApiError::from)?;
    container.jobs().submitted(&job.id).await;
    Ok(job)
}

#[tauri::command]
#[specta::specta]
pub async fn cancel_learning_generation_job(
    request: LearningGenerationJobActionRequestDto,
    container: State<'_, Container>,
) -> Result<LearningGenerationJob, ApiError> {
    cancel_generation_job(container.database(), container.jobs(), &request).await
}

pub(crate) async fn cancel_generation_job(
    db: &lattice_core::shared::persistence::Database,
    jobs: &lattice_core::shared::runtime::jobs::JobRuntime,
    request: &LearningGenerationJobActionRequestDto,
) -> Result<LearningGenerationJob, ApiError> {
    let repo =
        crate::features::learning::curriculum_repository::LearningCurriculumRepository::new(
            db.clone(),
        );
    match repo.preflight_cancel(request).await.map_err(ApiError::from)? {
        crate::features::learning::curriculum_repository::CancelPreflight::Replayed(job) => {
            return Ok(job);
        }
        crate::features::learning::curriculum_repository::CancelPreflight::Proceed => {}
    }
    // Inputs, revision, ownership, and status validated: trip the in-memory
    // cancellation token immediately before waiting on the urgent write turn.
    jobs.begin_cancel(&request.job_id);
    match repo.cancel_job(request).await {
        Ok(job) => {
            jobs.finish_cancel(&job.id).await;
            Ok(job)
        }
        Err(error) => {
            jobs.abort_cancel(&request.job_id);
            Err(ApiError::from(error))
        }
    }
}

#[tauri::command]
#[specta::specta]
pub async fn retry_learning_generation_job(
    request: LearningGenerationJobActionRequestDto,
    container: State<'_, Container>,
) -> Result<LearningGenerationJob, ApiError> {
    let repo = crate::features::learning::curriculum_repository::LearningCurriculumRepository::new(
        container.database().clone(),
    );
    let job = repo.retry_job(&request).await.map_err(ApiError::from)?;
    container.jobs().submitted(&job.id).await;
    Ok(job)
}

#[tauri::command]
#[specta::specta]
pub async fn get_learning_generation_job(
    id: String,
    container: State<'_, Container>,
) -> Result<LearningGenerationJob, ApiError> {
    crate::features::learning::curriculum_repository::LearningCurriculumRepository::new(
        container.database().clone(),
    )
    .job(&id)
    .await
    .map_err(ApiError::from)
}

pub(super) fn generation_worker(
    container: &Container,
    runtimes: BuiltinRuntimes,
    course_setups: crate::features::learning::course_setup::CourseSetups,
) -> crate::features::learning::generation_jobs::LessonGenerationWorker {
    let model_container = container.clone();
    let source_deps = source_deps(container);
    crate::features::learning::generation_jobs::LessonGenerationWorker {
        course_setups,
        runtimes,
        research_web: Some(container.web_service()),
        verifier: Some(super::audit::container_verifier(container)),
        database: container.database().clone(),
        courses: container.course_indexes().cloned(),
        load_llm: std::sync::Arc::new(move || {
            let container = model_container.clone();
            Box::pin(async move {
                container
                    .learning_llm(
                        crate::features::learning::model_requirements::lesson_preparation(),
                    )
                    .await
            })
        }),
        refresh_sources: std::sync::Arc::new(move |program_id| {
            let deps = source_deps.clone();
            Box::pin(async move {
                crate::features::learning::sources::ensure_before_use_sources(&deps, &program_id)
                    .await
            })
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::cancel_generation_job;
    use lattice_core::shared::error::Result;
    use lattice_core::shared::persistence::Database;
    use lattice_core::shared::runtime::jobs::{
        JobContext, JobHandler, JobKindConfig, JobOutcome, JobRuntime, JobStatus, RecoveryPolicy,
    };
    use lattice_learning::curriculum::{
        LearningGenerationJobKind, LearningGenerationJobStatus,
    };
    use lattice_learning::curriculum_repository::{LearningCurriculumRepository, LESSON_PREPARATION};
    use lattice_learning::dto::LearningProgramStatus;
    use lattice_learning::plan_dto::{
        LearningGenerationJobActionRequestDto, StartLearningGenerationJobRequestDto,
    };
    use lattice_learning::repository::LearningRepository;
    use std::sync::Arc;
    use std::time::Duration;
    use tokio::sync::{mpsc, Notify};

    #[tokio::test]
    async fn rejected_cancel_preflight_never_cancels_interrupt_worker_token() -> Result<()> {
        struct InterruptWorker {
            started: mpsc::UnboundedSender<String>,
            release: Arc<Notify>,
        }

        #[async_trait::async_trait]
        impl JobHandler for InterruptWorker {
            async fn run(&self, context: &JobContext) -> Result<JobOutcome> {
                self.started.send(context.id().to_string()).unwrap();
                match context.until_cancelled(self.release.notified()).await {
                    Some(()) => Ok(JobOutcome::Completed {
                        result_ref: "ok".into(),
                        message: "Done".into(),
                    }),
                    None => Ok(JobOutcome::Stopped),
                }
            }
        }

        let pool = lattice_core::shared::testing::sqlite::migrated_in_memory().await?;
        let db = Database::new(pool.clone());
        let mut program = lattice_learning::test_support::fixture();
        program.summary.status = LearningProgramStatus::Active;
        LearningRepository::new(db.clone()).create(&program).await?;
        let lesson_ids: Vec<String> = sqlx::query_scalar(
            "SELECT id FROM learning_lessons WHERE program_id = ? ORDER BY ordinal LIMIT 2",
        )
        .bind(&program.summary.id)
        .fetch_all(&pool)
        .await?;
        assert_eq!(lesson_ids.len(), 2);

        let repo = LearningCurriculumRepository::new(db.clone());
        let runtime = JobRuntime::with_poll_interval(db.clone(), Duration::from_millis(25));
        let (started_tx, mut started_rx) = mpsc::unbounded_channel();
        let release = Arc::new(Notify::new());
        runtime
            .register(
                LESSON_PREPARATION,
                Arc::new(InterruptWorker {
                    started: started_tx,
                    release: Arc::clone(&release),
                }),
                JobKindConfig::new(RecoveryPolicy::Interrupt).concurrency(2),
            )
            .await?;

        let start_job = |lesson_id: &str| {
            let repo = &repo;
            let program = &program;
            let lesson_id = lesson_id.to_string();
            async move {
                repo.start_lesson_job(&StartLearningGenerationJobRequestDto {
                    operation_id: uuid::Uuid::new_v4().to_string(),
                    program_id: program.summary.id.clone(),
                    expected_revision: program.summary.revision,
                    kind: LearningGenerationJobKind::LessonPreparation,
                    request_json: serde_json::json!({"lessonIds": [lesson_id]}).to_string(),
                    progress_total: 1,
                })
                .await
            }
        };

        let job1 = start_job(&lesson_ids[0]).await?;
        runtime.submitted(&job1.id).await;
        assert_eq!(started_rx.recv().await.unwrap(), job1.id);

        // Stale expected_revision, invalid operation_id, and wrong program_id must
        // all reject during preflight without tripping the Interrupt worker's token.
        let stale_req = LearningGenerationJobActionRequestDto {
            operation_id: uuid::Uuid::new_v4().to_string(),
            program_id: program.summary.id.clone(),
            job_id: job1.id.clone(),
            expected_revision: program.summary.revision + 99,
        };
        assert!(cancel_generation_job(&db, &runtime, &stale_req).await.is_err());

        let bad_op_req = LearningGenerationJobActionRequestDto {
            operation_id: "not-a-uuid".into(),
            program_id: program.summary.id.clone(),
            job_id: job1.id.clone(),
            expected_revision: program.summary.revision,
        };
        assert!(cancel_generation_job(&db, &runtime, &bad_op_req).await.is_err());

        let wrong_prog_req = LearningGenerationJobActionRequestDto {
            operation_id: uuid::Uuid::new_v4().to_string(),
            program_id: uuid::Uuid::new_v4().to_string(),
            job_id: job1.id.clone(),
            expected_revision: program.summary.revision,
        };
        assert!(cancel_generation_job(&db, &runtime, &wrong_prog_req).await.is_err());

        tokio::time::sleep(Duration::from_millis(40)).await;
        assert_eq!(runtime.store().get_summary(&job1.id).await?.status, JobStatus::Running);
        release.notify_waiters();
        tokio::time::timeout(Duration::from_secs(2), async {
            while runtime.store().get_summary(&job1.id).await?.status == JobStatus::Running {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            Ok::<_, lattice_core::shared::error::AppError>(())
        })
        .await
        .unwrap()?;
        assert_eq!(
            runtime.store().get_summary(&job1.id).await?.status,
            JobStatus::Completed
        );

        // Preflight also rejects already-finished jobs.
        let completed_req = LearningGenerationJobActionRequestDto {
            operation_id: uuid::Uuid::new_v4().to_string(),
            program_id: program.summary.id.clone(),
            job_id: job1.id.clone(),
            expected_revision: program.summary.revision,
        };
        assert!(cancel_generation_job(&db, &runtime, &completed_req).await.is_err());

        // Valid cancel succeeds and subsequent replay returns Cancelled cleanly.
        let job2 = start_job(&lesson_ids[1]).await?;
        runtime.submitted(&job2.id).await;
        assert_eq!(started_rx.recv().await.unwrap(), job2.id);

        let valid_req = LearningGenerationJobActionRequestDto {
            operation_id: uuid::Uuid::new_v4().to_string(),
            program_id: program.summary.id.clone(),
            job_id: job2.id.clone(),
            expected_revision: program.summary.revision,
        };
        let cancelled = cancel_generation_job(&db, &runtime, &valid_req).await.unwrap();
        assert_eq!(cancelled.status, LearningGenerationJobStatus::Cancelled);
        let replayed = cancel_generation_job(&db, &runtime, &valid_req).await.unwrap();
        assert_eq!(replayed.status, LearningGenerationJobStatus::Cancelled);
        Ok(())
    }
}

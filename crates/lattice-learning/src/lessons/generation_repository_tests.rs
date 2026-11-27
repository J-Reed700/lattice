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
    db: &Database,
) -> (
    LearningCurriculumRepository,
    crate::curriculum::LearningGenerationJob,
) {
    queued_job_with(db, true).await
}

/// A course the learner has not added any reference material to.
pub(super) async fn queued_job_without_sources(
    db: &Database,
) -> (
    LearningCurriculumRepository,
    crate::curriculum::LearningGenerationJob,
) {
    queued_job_with(db, false).await
}

async fn queued_job_with(
    db: &Database,
    with_sources: bool,
) -> (
    LearningCurriculumRepository,
    crate::curriculum::LearningGenerationJob,
) {
    let program_repo = LearningRepository::new(db.clone());
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
    let repo = LearningCurriculumRepository::new(db.clone());
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
        let db = Database::new(pool.clone());
        let (repo, job) = queued_job(&db).await;
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
            database: db.clone(),
            refresh_sources: Arc::new(|_| Box::pin(async { Ok(()) })),
            load_llm: Arc::new(move || {
                let entered = notification.clone();
                Box::pin(async move {
                    entered.notify_one();
                    std::future::pending().await
                })
            }),
        };
        let runtime = JobRuntime::new(db.clone());
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
        let store = JobStore::new(db.clone());
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
        let (_, pending) = queued_job(&db).await;
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
    let db = Database::new(pool.clone());
    let (repo, job) = queued_job(&db).await;
    let store = JobStore::new(db.clone());
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

fn worker_loading(db: &Database, load_llm: ModelLoader) -> LessonGenerationWorker {
    LessonGenerationWorker {
        course_setups: Default::default(),
        runtimes: crate::embedded_runtime::BuiltinRuntimes::for_tests(),
        database: db.clone(),
        load_llm,
        courses: None,
        refresh_sources: Arc::new(|_| Box::pin(async { Ok(()) })),
        research_web: None,
        verifier: None,
    }
}

#[tokio::test]
async fn a_lesson_moved_out_of_the_outline_is_not_prepared() {
    let pool = crate::tests::pool().await.unwrap();
    let db = Database::new(pool.clone());
    let (repo, job) = queued_job(&db).await;
    let lesson = job.lesson_ids[0].clone();
    sqlx::query("UPDATE learning_lessons SET curriculum_state='skipped' WHERE id=?")
        .bind(&lesson)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE learning_jobs SET content_revision=(SELECT content_revision FROM learning_programs WHERE id=?) WHERE job_id=?")
        .bind(&job.program_id)
        .bind(&job.id)
        .execute(&pool)
        .await
        .unwrap();
    let loads = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let counted = loads.clone();
    let worker = worker_loading(
        &db,
        Arc::new(move || {
            counted.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Box::pin(async {
                Err::<PinnedModel, _>(AppError::InvalidState("the model is not wanted".into()))
            })
        }),
    );
    run_attempt(worker, &job.id).await.unwrap();
    assert_eq!(
        loads.load(std::sync::atomic::Ordering::SeqCst),
        0,
        "a lesson that left the outline is refused before the model is loaded"
    );
    assert_eq!(
        repo.job(&job.id).await.unwrap().status,
        LearningGenerationJobStatus::Failed
    );
}

/// Plans the course when asked and refuses every other request.
struct PlansThenFails;

#[async_trait::async_trait]
impl lattice_core::application::ports::LLMPort for PlansThenFails {
    async fn complete(
        &self,
        request: &lattice_core::application::ports::llm_port::CompletionRequest,
    ) -> Result<lattice_core::application::ports::llm_port::CompletionResponse> {
        let (_, user) = crate::lessons::structured_call::task_messages(request);
        match crate::lessons::course_plan::fixture_response(user) {
            Some(plan) => {
                Ok(lattice_core::application::ports::llm_port::CompletionResponse::from_text(plan))
            }
            None => Err(AppError::InvalidState(
                "the lesson could not be written".into(),
            )),
        }
    }
    fn model_name(&self) -> &str {
        "plan-fixture"
    }
    fn count_tokens(&self, text: &str) -> usize {
        text.len() / 4
    }
    fn max_context_tokens(&self) -> usize {
        128_000
    }
    async fn is_ready(&self) -> Result<bool> {
        Ok(true)
    }
    fn enforces(
        &self,
        _schema: &lattice_core::application::services::model_contract::shape::Schema,
    ) -> bool {
        false
    }
}

#[tokio::test]
async fn a_failure_after_a_review_row_fails_the_job() {
    let pool = crate::tests::pool().await.unwrap();
    let db = Database::new(pool.clone());
    let (repo, job) = queued_job(&db).await;
    sqlx::query("INSERT INTO learning_lesson_reviews(lesson_id,program_id,job_id,state,revision,draft_sha256,checker_model,opened_at,updated_at) VALUES(?,?,?,'needs_review',0,'sha','fixture',1,1)")
        .bind(&job.lesson_ids[0])
        .bind(&job.program_id)
        .bind(&job.id)
        .execute(&pool)
        .await
        .unwrap();
    let llm: Arc<dyn lattice_core::application::ports::LLMPort> = Arc::new(PlansThenFails);
    let worker = worker_loading(
        &db,
        Arc::new(move || {
            let llm = llm.clone();
            Box::pin(async move { Ok(pinned_for_test(llm)) })
        }),
    );
    run_attempt(worker, &job.id).await.unwrap();
    let ended = repo.job(&job.id).await.unwrap();
    assert_eq!(
        ended.status,
        LearningGenerationJobStatus::Failed,
        "{ended:?}"
    );
}

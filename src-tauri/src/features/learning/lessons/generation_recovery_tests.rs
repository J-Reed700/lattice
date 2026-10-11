#![allow(clippy::unwrap_used, clippy::panic, clippy::indexing_slicing)]
use super::*;
use crate::features::learning::{
    curriculum::LearningGenerationJobStatus as Status,
    curriculum_repository::{lesson_job_config, LESSON_PREPARATION},
    lesson_drafts,
    plan_dto::LearningGenerationJobActionRequestDto,
};
use crate::shared::runtime::jobs::{JobStore, RecoveryPolicy};
use serde_json::json;

/// Records an outage the way the runtime does after a failed attempt.
async fn defer(pool: &SqlitePool, id: &str, error: AppError) -> Result<()> {
    let store = JobStore::new(pool.clone());
    let job = store.get(id).await?;
    let JobFailure::Defer(deferral) = lesson_failure(&job, &error) else {
        panic!("an outage is deferred, not failed");
    };
    store
        .defer(id, &deferral, lesson_job_config().backoff)
        .await?;
    Ok(())
}

#[tokio::test]
async fn worker_defers_outages_but_preserves_terminal_validation_errors() {
    for (error, expected) in [
        (AppError::Network("connection lost".into()), Status::Pending),
        (
            AppError::ServiceNotAvailable("provider unavailable".into()),
            Status::Pending,
        ),
        (
            AppError::RateLimitExceeded("provider busy".into()),
            Status::Pending,
        ),
        (
            AppError::InvalidInput("invalid reference".into()),
            Status::Failed,
        ),
    ] {
        let pool = crate::features::learning::tests::pool().await.unwrap();
        let (repo, job) = repository_tests::queued_job(&pool).await;
        let worker = LessonGenerationWorker {
            pool: pool.clone(),
            load_llm: Arc::new(|| {
                Box::pin(async { panic!("Reference refresh must finish first") })
            }),
            load_library: Arc::new(|| Box::pin(async { None })),
            refresh_sources: Arc::new(move |_| {
                let error = error.clone();
                Box::pin(async move { Err(error) })
            }),
            research_web: None,
        };
        run_attempt(worker, &job.id).await.unwrap();
        let stopped = repo.job(&job.id).await.unwrap();
        assert_eq!(stopped.status, expected);
        assert!(stopped.result_id.is_none());
        if expected == Status::Pending {
            assert!(stopped.finished_at.is_none());
            assert!(stopped.error.unwrap().contains("retries automatically"));
            // An immediate refresh failure can precede the first progress
            // heartbeat. An absent snapshot is valid; a running model is not.
            assert!(stopped
                .activity
                .is_none_or(|activity| !activity.model_running));
            assert!(
                JobStore::new(pool.clone())
                    .claim(&job.id)
                    .await
                    .unwrap()
                    .is_none(),
                "Do not hot-loop during backoff"
            );
        }
    }
}

#[tokio::test]
async fn outage_schedule_and_checkpoints_survive_reopen_and_cancellation_wins() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let options = sqlx::sqlite::SqliteConnectOptions::new()
        .filename(directory.path().join("reconnect.sqlite"))
        .create_if_missing(true)
        .foreign_keys(true);
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(2)
        .connect_with(options.clone())
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;
    let (repo, job) = repository_tests::queued_job(&pool).await;
    let program = LearningRepository::new(pool.clone())
        .get(&job.program_id)
        .await?;
    let lesson_id = &program.modules[0].lessons[0].id;
    assert!(JobStore::new(pool.clone()).claim(&job.id).await?.is_some());
    lesson_drafts::run(&repo, &job.id, lesson_id, async {
        lesson_drafts::resume("fixture-inputs".into()).await?;
        lesson_drafts::save("saved candidate").await?;
        lesson_drafts::record_checkpoint("fixture-comparison", json!({"complete": true})).await
    })
    .await?;
    defer(
        &pool,
        &job.id,
        AppError::ServiceNotAvailable("fixture server failure".into()),
    )
    .await?;
    drop(repo);
    pool.close().await;

    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(2)
        .connect_with(options)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;
    let repo = LearningCurriculumRepository::new(pool.clone());
    JobStore::new(pool.clone())
        .recover(LESSON_PREPARATION, RecoveryPolicy::Requeue)
        .await?;
    assert_eq!(repo.job(&job.id).await?.status, Status::Pending);
    assert!(!JobStore::new(pool.clone())
        .due(LESSON_PREPARATION)
        .await?
        .iter()
        .any(|pending| pending.id == job.id));
    assert!(JobStore::new(pool.clone()).claim(&job.id).await?.is_none());
    // Advance only this test database's delivery schedule, without wall-clock waits.
    sqlx::query("UPDATE jobs SET retry_not_before=0 WHERE id=?")
        .bind(&job.id)
        .execute(&pool)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;
    assert!(JobStore::new(pool.clone())
        .due(LESSON_PREPARATION)
        .await?
        .iter()
        .any(|pending| pending.id == job.id));
    let store = JobStore::new(pool.clone());
    let (a, b) = tokio::join!(store.claim(&job.id), store.claim(&job.id));
    assert_ne!(
        a?.is_some(),
        b?.is_some(),
        "One durable job must have one owner"
    );
    assert!(repo.job(&job.id).await?.error.is_none());
    assert_eq!(
        repo.lesson_draft(&job.id, lesson_id)
            .await?
            .unwrap()
            .candidate,
        "saved candidate"
    );
    assert_eq!(
        repo.lesson_checkpoint(&job.id, lesson_id, "fixture-comparison")
            .await?,
        Some(json!({"complete":true}))
    );
    assert_eq!(
        LearningRepository::new(pool.clone())
            .get(&job.program_id)
            .await?
            .modules[0]
            .lessons[0]
            .preparation,
        LearningPreparation::Outline
    );

    // Repeated outages never turn into a content failure or exhaust a job budget.
    for _ in 0..7 {
        defer(
            &pool,
            &job.id,
            AppError::ServiceNotAvailable("fixture server failure".into()),
        )
        .await?;
        assert_eq!(repo.job(&job.id).await?.status, Status::Pending);
        sqlx::query("UPDATE jobs SET retry_not_before=0 WHERE id=?")
            .bind(&job.id)
            .execute(&pool)
            .await
            .map_err(|e| AppError::Database(e.to_string()))?;
        assert!(JobStore::new(pool.clone()).claim(&job.id).await?.is_some());
    }
    defer(
        &pool,
        &job.id,
        AppError::ServiceNotAvailable("fixture server failure".into()),
    )
    .await?;
    repo.cancel_job(&LearningGenerationJobActionRequestDto {
        operation_id: uuid::Uuid::new_v4().to_string(),
        program_id: job.program_id.clone(),
        job_id: job.id.clone(),
        expected_revision: program.summary.revision,
    })
    .await?;
    defer(
        &pool,
        &job.id,
        AppError::ServiceNotAvailable("fixture server failure".into()),
    )
    .await?;
    JobStore::new(pool.clone())
        .recover(LESSON_PREPARATION, RecoveryPolicy::Requeue)
        .await?;
    assert_eq!(repo.job(&job.id).await?.status, Status::Cancelled);
    assert!(JobStore::new(pool.clone()).claim(&job.id).await?.is_none());
    pool.close().await;
    Ok(())
}

struct OfflineModel;
#[async_trait::async_trait]
impl LLMPort for OfflineModel {
    async fn is_ready(&self) -> Result<bool> {
        Ok(false)
    }
    async fn complete(
        &self,
        _: &crate::application::ports::llm_port::CompletionRequest,
    ) -> Result<crate::application::ports::llm_port::CompletionResponse> {
        Err(AppError::Network("fixture connection interrupted".into()))
    }
    fn model_name(&self) -> &str {
        "offline-fixture"
    }
    fn count_tokens(&self, text: &str) -> usize {
        text.len() / 4
    }
    fn max_context_tokens(&self) -> usize {
        32000
    }
}

#[tokio::test]
async fn fidelity_propagates_connection_loss_without_creating_a_factual_finding() {
    let fixture = json!({"units":[{
        "index":0,"kind":"teaching",
        "passages":[{"id":"p0","field":"/body","text":"The measured value is 12."}],
        "claims":[{"id":"c0","statement":"The measured value is 12."}]
    }],"mapping":{"p0":{"claimIds":["c0"],"nonFactualReason":"","missingClaims":[]}}});
    let result = crate::features::learning::content_verification::live_mapping_fixture(
        &OfflineModel,
        &fixture,
    )
    .await;
    assert!(matches!(result, Err(AppError::Network(_))));
}

#[tokio::test]
async fn source_limit_failure_retries_after_restart_without_losing_saved_checks() -> Result<()> {
    use crate::features::learning::{dto::*, source_library::*};
    let directory = tempfile::tempdir()?;
    let options = sqlx::sqlite::SqliteConnectOptions::new()
        .filename(directory.path().join("source-recovery.sqlite"))
        .create_if_missing(true)
        .foreign_keys(true);
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(2)
        .connect_with(options.clone())
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;
    let (repo, job) = repository_tests::queued_job(&pool).await;
    let program = LearningRepository::new(pool.clone())
        .get(&job.program_id)
        .await?;
    let lesson_id = program.modules[0].lessons[0].id.clone();
    let library = LearningSourceLibraryRepository::new(pool.clone());
    for index in 1..100 {
        library
            .add(
                &uuid::Uuid::new_v4().to_string(),
                &uuid::Uuid::new_v4().to_string(),
                &uuid::Uuid::new_v4().to_string(),
                &job.program_id,
                LearningSourceKind::Pasted,
                "add_text",
                "Fixture reference",
                None,
                LearningSourcePolicy::Fixed,
                CapturedLearningSource {
                    title: format!("Reference {index}"),
                    publisher: None,
                    requested_url: None,
                    resolved_url: None,
                    text: format!("Saved reference {index} with complete text."),
                    truncated: false,
                    extraction_version: "fixture".into(),
                },
                format!("capture-{index}"),
            )
            .await?;
    }
    assert!(JobStore::new(pool.clone()).claim(&job.id).await?.is_some());
    lesson_drafts::run(&repo, &job.id, &lesson_id, async {
        lesson_drafts::resume("unchanged-inputs".into()).await?;
        lesson_drafts::save("Saved exact candidate").await?;
        lesson_drafts::record_checkpoint(
            "completed-claim",
            json!({"claim":"unchanged","result":"supported","evidence":"saved exact passage"}),
        )
        .await
    })
    .await?;
    JobStore::new(pool.clone())
        .fail(
            &job.id,
            "generation_failed",
            "A learning program can contain at most 100 sources.",
        )
        .await?;
    drop(library);
    drop(repo);
    pool.close().await;
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(2)
        .connect_with(options)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;
    let repo = LearningCurriculumRepository::new(pool.clone());
    let retry = repo
        .retry_job(&LearningGenerationJobActionRequestDto {
            operation_id: uuid::Uuid::new_v4().to_string(),
            program_id: job.program_id.clone(),
            job_id: job.id.clone(),
            expected_revision: program.summary.revision,
        })
        .await?;
    assert_eq!(retry.status, Status::Pending);
    assert!(retry.error.is_none());
    assert_eq!(
        repo.lesson_draft(&retry.id, &lesson_id)
            .await?
            .unwrap()
            .candidate,
        "Saved exact candidate"
    );
    assert_eq!(
        repo.lesson_checkpoint(&retry.id, &lesson_id, "completed-claim")
            .await?,
        repo.lesson_checkpoint(&job.id, &lesson_id, "completed-claim")
            .await?
    );
    let library = LearningSourceLibraryRepository::new(pool.clone());
    let source = uuid::Uuid::new_v4().to_string();
    let version = uuid::Uuid::new_v4().to_string();
    library
        .preflight_new_source(&job.program_id, &source, &version)
        .await?;
    library
        .add(
            &uuid::Uuid::new_v4().to_string(),
            &source,
            &version,
            &job.program_id,
            LearningSourceKind::Pasted,
            "add_text",
            "New evidence",
            None,
            LearningSourcePolicy::Fixed,
            CapturedLearningSource {
                title: "New evidence".into(),
                publisher: None,
                requested_url: None,
                resolved_url: None,
                text: "The next needed source can be saved after recovery.".into(),
                truncated: false,
                extraction_version: "fixture".into(),
            },
            "new-evidence".into(),
        )
        .await?;
    assert_eq!(
        LearningRepository::new(pool.clone())
            .verification_sources(&job.program_id)
            .await?
            .len(),
        101
    );
    assert!(JobStore::new(pool.clone())
        .claim(&retry.id)
        .await?
        .is_some());
    assert!(repo.job_content_is_current(&retry.id).await?);
    pool.close().await;
    Ok(())
}

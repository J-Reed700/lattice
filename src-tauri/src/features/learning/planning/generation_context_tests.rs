//! Course edits invalidate pending teaching; ordinary learner progress does not.
#![allow(clippy::unwrap_used)]
use super::*;
use crate::features::learning::{
    curriculum::LearningCurriculumOperation,
    dto::{
        AcceptLearningProgramRequestDto, CompleteLearningLessonRequestDto, LearningBlockDto,
        LearningBlockKind, LearningPreparation, LearningProgramDto, PreparedLearningLesson,
    },
    lesson_drafts,
    tests::{fixture, pool},
};
use crate::shared::runtime::jobs::RecoveryPolicy;

fn prepared(lesson: &str) -> PreparedLearningLesson {
    let mut result = PreparedLearningLesson {
        blocks: vec![LearningBlockDto {
            kind: LearningBlockKind::Recap,
            title: "Earlier teaching".into(),
            body: "A saved teaching fixture, independent of learner progress.".into(),
            source_ids: vec![],
            rubric: vec![],
        }],
        questions: vec![],
        keys: vec![],
        verification: None,
    };
    result.verification =
        Some(crate::features::learning::content_verification::tests::attest(lesson, &result));
    result
}

async fn setup() -> Result<(SqlitePool, LearningProgramDto, String, String)> {
    let pool = pool().await?;
    let repo = LearningRepository::new(pool.clone());
    let program = fixture();
    let earlier = program.modules[0].lessons[0].id.clone();
    let target = program
        .modules
        .iter()
        .flat_map(|m| &m.lessons)
        .find(|l| l.id != earlier && l.preparation == LearningPreparation::Outline)
        .unwrap()
        .id
        .clone();
    repo.create(&program).await?;
    repo.accept(&AcceptLearningProgramRequestDto {
        program_id: program.summary.id.clone(),
        expected_revision: 0,
        title: program.summary.title.clone(),
    })
    .await?;
    // Persist through the same repository path as real prepared teaching.
    // The report fixture tests storage/concurrency, not factual approval.
    let accepted = repo.get(&program.summary.id).await?;
    repo.prepare(
        &program.summary.id,
        &earlier,
        accepted.summary.revision,
        &prepared(&earlier),
    )
    .await?;
    LearningCurriculumRepository::new(pool.clone())
        .plan(&program.summary.id)
        .await?;
    let program = repo.get(&program.summary.id).await?;
    Ok((pool, program, earlier, target))
}

async fn enqueue(
    repo: &LearningCurriculumRepository,
    program: &LearningProgramDto,
    target: &str,
) -> Result<LearningGenerationJob> {
    repo.start_job(&StartLearningGenerationJobRequestDto {
        operation_id: uuid::Uuid::new_v4().to_string(),
        program_id: program.summary.id.clone(),
        expected_revision: program.summary.revision,
        kind: LearningGenerationJobKind::LessonPreparation,
        request_json: serde_json::json!({"lessonIds":[target]}).to_string(),
        progress_total: 1,
    })
    .await
}

#[tokio::test]
async fn learner_completion_preserves_resume_retry_and_atomic_publication() -> Result<()> {
    let (pool, program, earlier, target) = setup().await?;
    let programs = LearningRepository::new(pool.clone());
    let jobs = LearningCurriculumRepository::new(pool.clone());
    let job = enqueue(&jobs, &program, &target).await?;
    // The learner finishes a different ready lesson while this job is queued.
    programs
        .complete(&CompleteLearningLessonRequestDto {
            program_id: program.summary.id.clone(),
            lesson_id: earlier.clone(),
            expected_revision: program.summary.revision,
        })
        .await?;
    let progressed = programs.get(&program.summary.id).await?;
    assert_eq!(progressed.summary.revision, program.summary.revision + 1);
    assert!(jobs.job_content_is_current(&job.id).await?);
    // Reloading the course after studying creates a new UI operation, but must
    // rejoin this lesson's existing queued work rather than enqueue it twice.
    assert_eq!(enqueue(&jobs, &progressed, &target).await?.id, job.id);
    assert!(JobStore::new(pool.clone()).claim(&job.id).await?.is_some());
    assert_eq!(enqueue(&jobs, &progressed, &target).await?.id, job.id);
    lesson_drafts::run(&jobs, &job.id, &target, async {
        assert_eq!(
            lesson_drafts::authoring_revision(progressed.summary.revision),
            program.summary.revision
        );
        lesson_drafts::record_checkpoint("comparison", serde_json::json!("retained")).await
    })
    .await?;
    JobStore::new(pool.clone()).suspend(&job.id).await?;
    JobStore::new(pool.clone())
        .recover(LESSON_PREPARATION, RecoveryPolicy::Requeue)
        .await?;
    assert!(JobStore::new(pool.clone()).claim(&job.id).await?.is_some());
    JobStore::new(pool.clone())
        .fail(&job.id, "generation_failed", "Retryable provider error")
        .await?;
    let mut retry_request = LearningGenerationJobActionRequestDto {
        operation_id: uuid::Uuid::new_v4().to_string(),
        program_id: program.summary.id.clone(),
        job_id: job.id,
        expected_revision: progressed.summary.revision,
    };
    let retry = jobs.retry_job(&retry_request).await?;
    retry_request.operation_id = uuid::Uuid::new_v4().to_string();
    assert_eq!(jobs.retry_job(&retry_request).await?.id, retry.id);
    assert!(JobStore::new(pool.clone())
        .claim(&retry.id)
        .await?
        .is_some());
    retry_request.operation_id = uuid::Uuid::new_v4().to_string();
    assert_eq!(jobs.retry_job(&retry_request).await?.id, retry.id);
    lesson_drafts::run(&jobs, &retry.id, &target, async {
        assert_eq!(
            lesson_drafts::authoring_revision(progressed.summary.revision),
            program.summary.revision
        );
        assert_eq!(
            lesson_drafts::checkpoint("comparison").await?,
            Some(serde_json::json!("retained"))
        );
        Ok(())
    })
    .await?;
    // Publication must preserve the learner's intervening progress and advance
    // the current revision, rather than restore the older job's revision.
    jobs.publish_prepared_lessons(
        &retry.id,
        program.summary.revision,
        &[(target.clone(), prepared(&target))],
    )
    .await?;
    let saved = programs.get(&program.summary.id).await?;
    assert_eq!(saved.summary.revision, progressed.summary.revision + 1);
    assert!(
        saved
            .modules
            .iter()
            .flat_map(|m| &m.lessons)
            .find(|l| l.id == earlier)
            .unwrap()
            .completed
    );
    assert_eq!(
        saved
            .modules
            .iter()
            .flat_map(|m| &m.lessons)
            .find(|l| l.id == target)
            .unwrap()
            .preparation,
        LearningPreparation::Ready
    );
    assert_eq!(
        jobs.job(&retry.id).await?.status,
        LearningGenerationJobStatus::Completed
    );
    Ok(())
}

#[tokio::test]
async fn accepted_curriculum_edits_cannot_publish_or_retry_old_teaching() -> Result<()> {
    let (pool, program, _, target) = setup().await?;
    let jobs = LearningCurriculumRepository::new(pool.clone());
    let job = enqueue(&jobs, &program, &target).await?;
    assert!(JobStore::new(pool.clone()).claim(&job.id).await?.is_some());
    let preview = jobs
        .preview(&PreviewLearningCurriculumRevisionRequestDto {
            operation_id: uuid::Uuid::new_v4().to_string(),
            program_id: program.summary.id.clone(),
            expected_revision: program.summary.revision,
            reason: "Change the teaching objective".into(),
            operations: vec![LearningCurriculumOperation::EditLesson {
                lesson_id: target.clone(),
                title: "Changed teaching objective".into(),
                objective: "A different objective requires fresh authoring.".into(),
                estimated_minutes: 25,
            }],
        })
        .await?;
    let updated = jobs
        .accept_revision(&LearningCurriculumRevisionActionRequestDto {
            operation_id: uuid::Uuid::new_v4().to_string(),
            program_id: program.summary.id.clone(),
            revision_id: preview.draft_revision.unwrap().id,
            expected_revision: program.summary.revision,
        })
        .await?;
    assert!(!jobs.job_content_is_current(&job.id).await?);
    assert!(jobs
        .publish_prepared_lessons(
            &job.id,
            updated.program_revision,
            &[(target.clone(), prepared(&target))]
        )
        .await
        .is_err());
    JobStore::new(pool.clone())
        .fail(&job.id, "generation_failed", "Content changed")
        .await?;
    assert!(jobs
        .retry_job(&LearningGenerationJobActionRequestDto {
            operation_id: uuid::Uuid::new_v4().to_string(),
            program_id: program.summary.id.clone(),
            job_id: job.id,
            expected_revision: updated.program_revision,
        })
        .await
        .is_err());
    let fresh = enqueue(
        &jobs,
        &LearningRepository::new(pool)
            .get(&program.summary.id)
            .await?,
        &target,
    )
    .await?;
    assert!(jobs.job_content_is_current(&fresh.id).await?);
    Ok(())
}

#[tokio::test]
async fn changed_prior_teaching_invalidates_a_job_even_without_a_ui_revision_write() -> Result<()> {
    let (pool, program, earlier, target) = setup().await?;
    let jobs = LearningCurriculumRepository::new(pool.clone());
    let job = enqueue(&jobs, &program, &target).await?;
    assert!(JobStore::new(pool.clone()).claim(&job.id).await?.is_some());
    sqlx::query(
        "UPDATE learning_blocks SET body='Different prerequisite teaching' WHERE lesson_id=?",
    )
    .bind(earlier)
    .execute(&pool)
    .await
    .map_err(db)?;
    assert!(!jobs.job_content_is_current(&job.id).await?);
    assert!(jobs
        .publish_prepared_lessons(
            &job.id,
            program.summary.revision,
            &[(target.clone(), prepared(&target))]
        )
        .await
        .is_err());
    Ok(())
}

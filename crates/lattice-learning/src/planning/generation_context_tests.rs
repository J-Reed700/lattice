//! Course edits invalidate pending teaching; ordinary learner progress does not.
use super::*;
use crate::{
    curriculum::LearningCurriculumOperation,
    dto::{
        AcceptLearningProgramRequestDto, CompleteLearningLessonRequestDto, LearningBlockDto,
        LearningBlockKind, LearningPreparation, LearningProgramDto, PreparedLearningLesson,
    },
    lesson_drafts,
    tests::{fixture, pool},
};
use lattice_core::shared::persistence::Database;
use lattice_core::shared::runtime::jobs::RecoveryPolicy;

fn prepared(lesson: &str) -> PreparedLearningLesson {
    let mut result = PreparedLearningLesson {
        blocks: vec![LearningBlockDto::new(
            LearningBlockKind::Recap,
            "Earlier teaching".into(),
            "A saved teaching fixture, independent of learner progress.".into(),
            vec![],
            vec![],
        )],
        questions: vec![],
        keys: vec![],
        verification: None,
    };
    result.verification = Some(crate::content_verification::tests::attest(lesson, &result));
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
    lesson_drafts::run(
        &Database::from(pool.clone()),
        &jobs,
        &job.id,
        &target,
        async {
            assert_eq!(
                lesson_drafts::authoring_revision(progressed.summary.revision),
                program.summary.revision
            );
            lesson_drafts::record_checkpoint("comparison", serde_json::json!("retained")).await
        },
    )
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
    lesson_drafts::run(
        &Database::from(pool.clone()),
        &jobs,
        &retry.id,
        &target,
        async {
            assert_eq!(
                lesson_drafts::authoring_revision(progressed.summary.revision),
                program.summary.revision
            );
            assert_eq!(
                lesson_drafts::checkpoint("comparison").await?,
                Some(serde_json::json!("retained"))
            );
            Ok(())
        },
    )
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

struct Course {
    pool: SqlitePool,
    jobs: LearningCurriculumRepository,
    program_id: String,
    earlier: String,
    modules: Vec<String>,
    lessons: Vec<String>,
}

impl Course {
    async fn new() -> Result<Self> {
        let (pool, program, earlier, _) = setup().await?;
        let lessons = program
            .modules
            .iter()
            .flat_map(|m| &m.lessons)
            .filter(|l| l.preparation == LearningPreparation::Outline)
            .map(|l| l.id.clone())
            .collect();
        Ok(Self {
            jobs: LearningCurriculumRepository::new(pool.clone()),
            program_id: program.summary.id.clone(),
            modules: program.modules.iter().map(|m| m.id.clone()).collect(),
            pool,
            earlier,
            lessons,
        })
    }

    async fn revision(&self) -> Result<i64> {
        Ok(LearningRepository::new(self.pool.clone())
            .get(&self.program_id)
            .await?
            .summary
            .revision)
    }

    async fn queue(&self, lessons: &[&str]) -> Result<LearningGenerationJob> {
        self.jobs
            .start_job(&StartLearningGenerationJobRequestDto {
                operation_id: uuid::Uuid::new_v4().to_string(),
                program_id: self.program_id.clone(),
                expected_revision: self.revision().await?,
                kind: LearningGenerationJobKind::LessonPreparation,
                request_json: serde_json::json!({ "lessonIds": lessons }).to_string(),
                progress_total: lessons.len() as u32,
            })
            .await
    }

    async fn started(&self, lessons: &[&str]) -> Result<LearningGenerationJob> {
        let job = self.queue(lessons).await?;
        assert!(JobStore::new(self.pool.clone())
            .claim(&job.id)
            .await?
            .is_some());
        Ok(job)
    }

    async fn publish(&self, job: &LearningGenerationJob, lesson: &str) -> Result<()> {
        self.jobs
            .publish_prepared_lessons(
                &job.id,
                self.revision().await?,
                &[(lesson.to_string(), prepared(lesson))],
            )
            .await
    }

    async fn run(&self, sql: &str, id: &str) -> Result<()> {
        sqlx::query(sql)
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(db)?;
        Ok(())
    }

    async fn publish_one_and_hold_one(&self) -> Result<LearningGenerationJob> {
        let (first, second) = (self.lessons[0].as_str(), self.lessons[1].as_str());
        let published = self.started(&[first]).await?;
        self.publish(&published, first).await?;
        let job = self.started(&[second]).await?;
        let held = crate::content_verification::HeldForReview {
            lesson_id: second.to_string(),
            claims: 1,
            stop: None,
        };
        let (result_ref, message) = crate::content_verification::review_outcome(&[held]);
        JobStore::new(self.pool.clone())
            .complete(&job.id, &result_ref, &message)
            .await?;
        Ok(job)
    }
}

#[tokio::test]
async fn a_lesson_queued_behind_another_publishes_after_it() -> Result<()> {
    let course = Course::new().await?;
    let (a, b) = (course.lessons[0].as_str(), course.lessons[1].as_str());
    let first = course.queue(&[a]).await?;
    let second = course.queue(&[b]).await?;
    let store = JobStore::new(course.pool.clone());
    assert!(store.claim(&first.id).await?.is_some());
    course.publish(&first, a).await?;
    assert!(course.jobs.job_content_is_current(&second.id).await?);
    assert!(store.claim(&second.id).await?.is_some());
    course.publish(&second, b).await?;
    for job in [&first, &second] {
        assert_eq!(
            course.jobs.job(&job.id).await?.status,
            LearningGenerationJobStatus::Completed
        );
    }
    Ok(())
}

const BLOCK_TITLE: &str = "UPDATE learning_blocks SET title='Another title' WHERE lesson_id=?";
const BLOCK_KIND: &str = "UPDATE learning_blocks SET kind='explanation' WHERE lesson_id=?";
const BLOCK_ORDINAL: &str = "UPDATE learning_blocks SET ordinal=ordinal+5 WHERE lesson_id=?";
const BLOCK_BODY: &str =
    "UPDATE learning_blocks SET body='A sentence the learner wrote.' WHERE lesson_id=?";
const BLOCK_INSERTED: &str = "INSERT INTO learning_blocks(lesson_id,ordinal,kind,title,body,source_ids_json) VALUES(?,99,'recap','Added block','More teaching','[]')";
const BLOCK_DELETED: &str = "DELETE FROM learning_blocks WHERE lesson_id=?";

#[derive(Clone, Copy, Debug)]
enum Meanwhile {
    EarlierLesson(&'static str),
    AnotherLessonPublished,
    LessonCompleted,
    HeldLessonJobCompleted,
}

#[tokio::test]
async fn a_lesson_in_progress_publishes_after_learner_work_and_other_lessons_publish() -> Result<()>
{
    use Meanwhile::*;
    for event in [
        AnotherLessonPublished,
        EarlierLesson(BLOCK_TITLE),
        EarlierLesson(BLOCK_KIND),
        EarlierLesson(BLOCK_ORDINAL),
        EarlierLesson(BLOCK_BODY),
        EarlierLesson(BLOCK_INSERTED),
        EarlierLesson(BLOCK_DELETED),
        LessonCompleted,
        HeldLessonJobCompleted,
    ] {
        let course = Course::new().await?;
        let target = course.lessons[2].clone();
        let job = course.started(&[&target]).await?;
        match event {
            EarlierLesson(sql) => course.run(sql, &course.earlier).await?,
            AnotherLessonPublished => {
                let other = course.lessons[0].as_str();
                let published = course.started(&[other]).await?;
                course.publish(&published, other).await?;
            }
            LessonCompleted => {
                LearningRepository::new(course.pool.clone())
                    .complete(&CompleteLearningLessonRequestDto {
                        program_id: course.program_id.clone(),
                        lesson_id: course.earlier.clone(),
                        expected_revision: course.revision().await?,
                    })
                    .await?;
            }
            HeldLessonJobCompleted => {
                course.publish_one_and_hold_one().await?;
            }
        }
        assert!(
            course.jobs.job_content_is_current(&job.id).await?,
            "{event:?} moved the counter"
        );
        if let Err(error) = course.publish(&job, &target).await {
            panic!("{event:?} stopped the publication: {error}");
        }
    }
    Ok(())
}

const LESSON_TITLE: &str = "UPDATE learning_lessons SET title='Renamed lesson' WHERE id=?";
const LESSON_OBJECTIVE: &str =
    "UPDATE learning_lessons SET objective='Another objective' WHERE id=?";
const LESSON_REMOVED: &str = "DELETE FROM learning_lessons WHERE id=?";
const LESSON_MOVED: &str = "UPDATE learning_lessons SET ordinal=ordinal+10 WHERE id=?";
const LESSON_ADDED: &str = "INSERT INTO learning_lessons(id,program_id,module_id,ordinal,title,objective,estimated_minutes,preparation,completed) SELECT lower(hex(randomblob(16))),program_id,module_id,99,'An added lesson','Explain one more idea',20,'outline',0 FROM learning_lessons WHERE id=?";
const MODULE_TITLE: &str = "UPDATE learning_modules SET title='Renamed module' WHERE id=?";
const MODULE_OUTCOMES: &str =
    "UPDATE learning_modules SET outcomes_json='[\"Another outcome\"]' WHERE id=?";
const MODULE_MILESTONE: &str =
    "UPDATE learning_modules SET project_json='{\"title\":\"A milestone\"}' WHERE id=?";
const MODULE_PREREQUISITES: &str =
    "UPDATE learning_modules SET prerequisite_ids_json='[\"another-module\"]' WHERE id=?";

#[tokio::test]
async fn a_lesson_in_progress_is_refused_after_an_outline_edit() -> Result<()> {
    let on_lesson = [
        LESSON_TITLE,
        LESSON_OBJECTIVE,
        LESSON_ADDED,
        LESSON_REMOVED,
        LESSON_MOVED,
    ];
    let on_module = [
        MODULE_TITLE,
        MODULE_OUTCOMES,
        MODULE_MILESTONE,
        MODULE_PREREQUISITES,
    ];
    for sql in on_lesson.into_iter().chain(on_module) {
        let course = Course::new().await?;
        let target = course.lessons[2].clone();
        let job = course.started(&[&target]).await?;
        let id = if sql.contains("learning_modules") {
            &course.modules[0]
        } else {
            &course.lessons[0]
        };
        course.run(sql, id).await?;
        assert!(
            !course.jobs.job_content_is_current(&job.id).await?,
            "{sql} left the counter alone"
        );
        assert!(
            course.publish(&job, &target).await.is_err(),
            "{sql} did not stop the publication"
        );
    }
    Ok(())
}

#[tokio::test]
async fn a_lesson_whose_own_row_stopped_being_an_outline_lesson_is_refused() -> Result<()> {
    for (state, completed) in [
        ("outline", 1),
        ("skipped", 0),
        ("replaced", 0),
        ("completed", 0),
        ("challenged", 0),
        ("ready", 0),
    ] {
        let course = Course::new().await?;
        let target = course.lessons[2].clone();
        let job = course.started(&[&target]).await?;
        sqlx::query("UPDATE learning_lessons SET curriculum_state=?,completed=? WHERE id=?")
            .bind(state)
            .bind(completed)
            .bind(&target)
            .execute(&course.pool)
            .await
            .map_err(db)?;
        assert!(
            course.publish(&job, &target).await.is_err(),
            "{state} with completed={completed} was published"
        );
        let preparation: String =
            sqlx::query_scalar("SELECT preparation FROM learning_lessons WHERE id=?")
                .bind(&target)
                .fetch_one(&course.pool)
                .await
                .map_err(db)?;
        assert_eq!(preparation, "outline");
    }
    Ok(())
}

#[tokio::test]
async fn a_held_lesson_leaves_the_lesson_behind_it_and_its_own_resume_current() -> Result<()> {
    let course = Course::new().await?;
    let target = course.lessons[2].clone();
    let behind = course.queue(&[&target]).await?;
    let held_job = course.publish_one_and_hold_one().await?;
    assert_eq!(
        course.jobs.job(&held_job.id).await?.status,
        LearningGenerationJobStatus::Completed
    );
    assert!(JobStore::new(course.pool.clone())
        .claim(&behind.id)
        .await?
        .is_some());
    course.publish(&behind, &target).await?;
    assert!(course.jobs.job_content_is_current(&held_job.id).await?);
    let resumed = course
        .jobs
        .continue_review_job(&LearningGenerationJobActionRequestDto {
            operation_id: uuid::Uuid::new_v4().to_string(),
            program_id: course.program_id.clone(),
            job_id: held_job.id.clone(),
            expected_revision: course.revision().await?,
        })
        .await?;
    assert_eq!(
        resumed.retry_of_job_id.as_deref(),
        Some(held_job.id.as_str())
    );
    Ok(())
}

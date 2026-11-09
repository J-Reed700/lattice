//! A lesson's quiz questions are written by a job of their own, queued with
//! the lesson's publication, so the course's next lesson never waits for them.
#![allow(clippy::unwrap_used, clippy::panic, clippy::indexing_slicing)]
use super::*;
use crate::{
    curriculum::LearningGenerationJobStatus as Status,
    curriculum_repository::{lesson_job_config, LESSON_PREPARATION},
    quiz_bank_jobs::{quiz_job_config, QUIZ_BANK},
};
use lattice_core::application::ports::llm_port::{CompletionRequest, CompletionResponse};
use lattice_core::application::ports::LLMPort;
use lattice_core::shared::persistence::Transactions;
use lattice_core::shared::runtime::jobs::{JobRuntime, JobStatus, JobStore};
use std::sync::Mutex;

/// A word only the lesson's published block carries.
pub(crate) const BODY_MARK: &str = "zorblaxian";

/// Answers every request with no items and keeps what it was asked. It
/// is named as `attest` names the checker of a prepared lesson's report.
pub(crate) struct Recorder {
    requests: Mutex<Vec<String>>,
}

impl Recorder {
    pub(crate) fn new() -> Arc<Self> {
        Arc::new(Self {
            requests: Mutex::new(Vec::new()),
        })
    }

    pub(crate) fn requests(&self) -> Vec<String> {
        self.requests.lock().unwrap().clone()
    }
}

#[async_trait::async_trait]
impl LLMPort for Recorder {
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        let (_, user) = crate::lessons::structured_call::task_messages(request);
        self.requests.lock().unwrap().push(user.to_owned());
        Ok(CompletionResponse::from_text("{\"items\":[]}"))
    }
    fn model_name(&self) -> &str {
        "persistence-fixture"
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
}

pub(crate) struct Published {
    pub program_id: String,
    pub lesson_id: String,
    pub job_id: String,
}

fn questions() -> [(&'static str, [&'static str; 4], usize); 3] {
    [
        (
            "Which statement describes how the lesson's first idea works?",
            [
                "It tracks changes",
                "It hides changes",
                "It ignores changes",
                "It undoes changes",
            ],
            0,
        ),
        (
            "What does the lesson's second idea guarantee about ordering?",
            ["Stable order", "Random order", "Reverse order", "No order"],
            1,
        ),
        (
            "Where does the lesson say the third idea applies?",
            [
                "Only offline",
                "Only online",
                "In both settings",
                "In neither",
            ],
            2,
        ),
    ]
}

/// A prepared lesson for `lesson`, cited to `source_id`, whose block carries
/// [`BODY_MARK`] and the lesson's own words.
fn prepared(
    lesson: &crate::dto::LearningLessonDto,
    source_id: &str,
) -> crate::dto::PreparedLearningLesson {
    let mut prepared = crate::tests::prepared_with_questions(&lesson.id, &questions());
    prepared.blocks[0].body = format!("{} {} {BODY_MARK}", lesson.title, lesson.objective);
    prepared.blocks[0].source_ids = vec![source_id.to_owned()];
    for question in &mut prepared.questions {
        question.source_ids = vec![source_id.to_owned()];
    }
    prepared.verification = Some(crate::content_verification::tests::attest(
        &lesson.id, &prepared,
    ));
    prepared
}

/// Runs the course's first lesson job through the real worker and runtime
/// with its lesson already prepared and verified, as a resumed job has it,
/// and `model` as the Learning model.
pub(crate) async fn run_lesson_job(pool: &SqlitePool, model: Arc<Recorder>) -> Result<Published> {
    let (repo, job) = repository_tests::queued_job(pool).await;
    let program = LearningRepository::new(pool.clone())
        .get(&job.program_id)
        .await?;
    let sources = LearningRepository::new(pool.clone())
        .verification_sources(&job.program_id)
        .await?;
    let lesson = &program.modules[0].lessons[0];
    let prepared = prepared(lesson, &sources[0].id);
    let store = JobStore::new(pool.clone());
    store.claim(&job.id).await?.unwrap();
    repo.save_lesson_checkpoint(
        &job.id,
        &lesson.id,
        "prepared-v1",
        serde_json::to_value(&prepared)?,
    )
    .await?;
    store.suspend(&job.id).await?;
    let llm: Arc<dyn LLMPort> = model;
    let worker = LessonGenerationWorker {
        course_setups: Default::default(),
        runtimes: crate::embedded_runtime::BuiltinRuntimes::for_tests(),
        pool: pool.clone(),
        load_llm: Arc::new(move || {
            let llm = llm.clone();
            Box::pin(async move { Ok(pinned_for_test(llm)) })
        }),
        courses: None,
        refresh_sources: Arc::new(|_| Box::pin(async { Ok(()) })),
        research_web: None,
        verifier: None,
    };
    run_attempt(worker, &job.id).await?;
    Ok(Published {
        program_id: job.program_id,
        lesson_id: lesson.id.clone(),
        job_id: job.id,
    })
}

/// The course's quiz jobs, oldest first.
pub(crate) async fn quiz_jobs(
    pool: &SqlitePool,
    program_id: &str,
) -> Result<Vec<lattice_core::shared::runtime::jobs::JobRecord>> {
    let mut jobs = JobStore::new(pool.clone())
        .list_for_subject(QUIZ_BANK, program_id)
        .await?;
    jobs.sort_by_key(|job| job.created_at);
    Ok(jobs)
}

async fn count(pool: &SqlitePool, sql: &str, binds: &[&str]) -> i64 {
    let mut query = sqlx::query_scalar(sql);
    for bind in binds {
        query = query.bind(*bind);
    }
    query.fetch_one(pool).await.unwrap()
}

#[tokio::test]
async fn a_lesson_job_publishes_when_its_teaching_is_verified_and_queues_its_quiz() -> Result<()> {
    let pool = crate::tests::pool().await?;
    let model = Recorder::new();
    let published = run_lesson_job(&pool, model.clone()).await?;
    let (program, lesson) = (&published.program_id, &published.lesson_id);

    let job = LearningCurriculumRepository::new(pool.clone())
        .job(&published.job_id)
        .await?;
    assert_eq!(job.status, Status::Completed);
    assert!(
        model.requests().is_empty(),
        "the lesson job wrote quiz questions: {:?}",
        model.requests()
    );
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM learning_lessons WHERE id=? AND preparation='ready'",
            &[lesson]
        )
        .await,
        1
    );
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM learning_outcome_definitions WHERE program_id=? AND lesson_id=?",
            &[program, lesson]
        )
        .await,
        1
    );
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM learning_assessment_candidates WHERE lesson_id=? AND origin='lesson' AND review_status='agreed'",
            &[lesson]
        )
        .await,
        3
    );
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM learning_assessment_candidates WHERE lesson_id=? AND origin='lesson' AND author_model=?",
            &[lesson, "persistence-fixture"]
        )
        .await,
        3,
        "the lesson's own questions do not name the model that wrote them"
    );
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM learning_assessment_candidates WHERE lesson_id=? AND origin='bank'",
            &[lesson]
        )
        .await,
        0
    );
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM learning_assessment_blueprints WHERE program_id=? AND scope_kind='lesson' AND scope_id=?",
            &[program, lesson]
        )
        .await,
        1
    );

    let jobs = quiz_jobs(&pool, program).await?;
    assert_eq!(jobs.len(), 1, "one quiz job for the lesson");
    assert_eq!(jobs[0].status, JobStatus::Pending);
    assert_eq!(
        jobs[0].requested,
        serde_json::json!({
            "programId": program,
            "scope": {"kind": "lesson", "id": lesson},
            "topUp": false,
            "lessonStage": true,
        })
    );
    Ok(())
}

/// Guard: the quiz job is queued inside the publication's transaction, so a
/// publication that fails leaves none behind. On the old code no job is
/// queued at all, so this passes there too.
#[tokio::test]
async fn a_publication_that_fails_leaves_no_quiz_job_behind() -> Result<()> {
    let pool = crate::tests::pool().await?;
    let (repo, job) = repository_tests::queued_job(&pool).await;
    let program = LearningRepository::new(pool.clone())
        .get(&job.program_id)
        .await?;
    let sources = LearningRepository::new(pool.clone())
        .verification_sources(&job.program_id)
        .await?;
    let lesson = &program.modules[0].lessons[0];
    JobStore::new(pool.clone()).claim(&job.id).await?.unwrap();
    // No accepted snapshot exists yet, which publication finds only after it
    // has written the lesson and queued its quiz.
    let published = repo
        .publish_prepared_lessons(
            &job.id,
            i64::from(job.base_revision_number),
            &[(lesson.id.clone(), prepared(lesson, &sources[0].id))],
        )
        .await;
    assert!(published.is_err());
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM learning_lessons WHERE id=? AND preparation='outline'",
            &[&lesson.id]
        )
        .await,
        1
    );
    assert!(quiz_jobs(&pool, &job.program_id).await?.is_empty());
    Ok(())
}

/// Two lessons of one course publish together, as lesson jobs that no longer
/// wait for each other do: both succeed on the one accepted outline, each
/// keeps its own blocks, and each queues its own quiz job.
#[tokio::test]
async fn two_lessons_of_one_course_publish_at_the_same_time() -> Result<()> {
    use crate::curriculum::LearningGenerationJobKind;
    use crate::plan_dto::StartLearningGenerationJobRequestDto;
    let pool = crate::tests::pool().await?;
    let (repo, first) = repository_tests::queued_job(&pool).await;
    let program = LearningRepository::new(pool.clone())
        .get(&first.program_id)
        .await?;
    let sources = LearningRepository::new(pool.clone())
        .verification_sources(&first.program_id)
        .await?;
    let lessons = [
        &program.modules[0].lessons[0],
        &program.modules[0].lessons[1],
    ];
    let second = repo
        .start_lesson_job(&StartLearningGenerationJobRequestDto {
            operation_id: uuid::Uuid::new_v4().to_string(),
            program_id: first.program_id.clone(),
            expected_revision: program.summary.revision,
            kind: LearningGenerationJobKind::LessonPreparation,
            request_json: serde_json::json!({"lessonIds": [lessons[1].id]}).to_string(),
            progress_total: 1,
        })
        .await?;
    repo.seed_accepted(&first.program_id).await?;
    let store = JobStore::new(pool.clone());
    for job in [&first, &second] {
        store.claim(&job.id).await?.unwrap();
    }
    let jobs = [&first, &second];

    let staged = [
        [(lessons[0].id.clone(), prepared(lessons[0], &sources[0].id))],
        [(lessons[1].id.clone(), prepared(lessons[1], &sources[0].id))],
    ];
    let (one, other) = tokio::join!(
        repo.publish_prepared(
            &jobs[0].id,
            i64::from(jobs[0].base_revision_number),
            &staged[0],
            &[],
            Some("persistence-fixture"),
        ),
        repo.publish_prepared(
            &jobs[1].id,
            i64::from(jobs[1].base_revision_number),
            &staged[1],
            &[],
            Some("persistence-fixture"),
        ),
    );
    one?;
    other?;

    for (job, lesson) in jobs.iter().zip(lessons) {
        assert_eq!(repo.job(&job.id).await?.status, Status::Completed);
        assert_eq!(
            count(
                &pool,
                "SELECT count(*) FROM learning_lessons WHERE id=? AND preparation='ready'",
                &[&lesson.id]
            )
            .await,
            1
        );
        let bodies: Vec<String> =
            sqlx::query_scalar("SELECT body FROM learning_blocks WHERE lesson_id=?")
                .bind(&lesson.id)
                .fetch_all(&pool)
                .await
                .unwrap();
        assert_eq!(bodies.len(), 1);
        assert!(bodies[0].contains(&lesson.title) && bodies[0].contains(BODY_MARK));
    }
    let queued = quiz_jobs(&pool, &first.program_id).await?;
    let mut scoped: Vec<String> = queued
        .iter()
        .map(|job| job.requested["scope"]["id"].as_str().unwrap().to_owned())
        .collect();
    scoped.sort();
    let mut expected = vec![lessons[0].id.clone(), lessons[1].id.clone()];
    expected.sort();
    assert_eq!(scoped, expected, "each lesson queues its own quiz job");
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM learning_curriculum_revisions WHERE program_id=? AND status='accepted'",
            &[&first.program_id]
        )
        .await,
        1
    );
    Ok(())
}

struct Marks(Arc<tokio::sync::Notify>, bool);

#[async_trait::async_trait]
impl JobHandler for Marks {
    async fn run(&self, context: &JobContext) -> Result<JobOutcome> {
        self.0.notify_one();
        if self.1 {
            let _ = context.until_cancelled(std::future::pending::<()>()).await;
            return Ok(JobOutcome::Stopped);
        }
        Ok(JobOutcome::Completed {
            result_ref: "done".into(),
            message: "done".into(),
        })
    }
}

/// Guard: a quiz job and a lesson job of one course hold different locks
/// (one per kind and course), so a running quiz job never holds the next
/// lesson back. It passes on the old code, where the lesson job wrote the
/// quiz itself.
#[tokio::test]
async fn a_lesson_job_starts_while_a_quiz_job_of_its_course_is_running() -> Result<()> {
    let pool = crate::tests::pool().await?;
    let (_, job) = repository_tests::queued_job(&pool).await;
    let program = LearningRepository::new(pool.clone())
        .get(&job.program_id)
        .await?;
    let scope = crate::dto::LearningQuizScopeDto {
        kind: crate::dto::LearningQuizScopeKind::Lesson,
        id: Some(program.modules[0].lessons[0].id.clone()),
    };
    let quiz = crate::quiz_bank_jobs::queue_bank(
        &pool,
        &uuid::Uuid::new_v4().to_string(),
        &job.program_id,
        &scope,
        false,
    )
    .await?;
    let quiz_running = Arc::new(tokio::sync::Notify::new());
    let lesson_started = Arc::new(tokio::sync::Notify::new());
    let runtime = JobRuntime::new(pool.clone());
    runtime
        .register(
            QUIZ_BANK,
            Arc::new(Marks(quiz_running.clone(), true)),
            quiz_job_config(),
        )
        .await?;
    runtime
        .register(
            LESSON_PREPARATION,
            Arc::new(Marks(lesson_started.clone(), false)),
            lesson_job_config(),
        )
        .await?;
    runtime.dispatch(&quiz);
    let wait = std::time::Duration::from_secs(10);
    tokio::time::timeout(wait, quiz_running.notified())
        .await
        .unwrap();
    runtime.dispatch(&job.id);
    let started = tokio::time::timeout(wait, lesson_started.notified()).await;
    runtime.close();
    runtime.drain().await;
    assert!(
        started.is_ok(),
        "the lesson job waited for the course's quiz job"
    );
    Ok(())
}

/// Every lesson of the fixture course, in course order.
async fn lesson_ids_in_course_order(pool: &SqlitePool, program_id: &str) -> Vec<String> {
    let program = LearningRepository::new(pool.clone())
        .get(program_id)
        .await
        .unwrap();
    program
        .modules
        .iter()
        .flat_map(|module| module.lessons.iter().map(|lesson| lesson.id.clone()))
        .collect()
}

#[tokio::test]
async fn a_lesson_job_is_ranked_by_its_lessons_place_in_the_course() -> Result<()> {
    let pool = crate::tests::pool().await?;
    let (repo, first) = repository_tests::queued_job(&pool).await;
    let store = JobStore::new(pool.clone());
    assert_eq!(store.get(&first.id).await?.rank, Some(0));
    let lessons = lesson_ids_in_course_order(&pool, &first.program_id).await;
    assert_eq!(lessons.len(), 4);
    // Later lessons first: a rank is the lesson's place, not the job's order.
    for (place, lesson) in lessons.iter().enumerate().skip(1).rev() {
        let job = repo
            .start_job(&crate::plan_dto::StartLearningGenerationJobRequestDto {
                operation_id: uuid::Uuid::new_v4().to_string(),
                program_id: first.program_id.clone(),
                expected_revision: i64::from(first.base_revision_number),
                kind: crate::curriculum::LearningGenerationJobKind::LessonPreparation,
                request_json: serde_json::json!({ "lessonIds": [lesson] }).to_string(),
                progress_total: 1,
            })
            .await?;
        assert_eq!(
            store.get(&job.id).await?.rank,
            Some(i64::try_from(place).unwrap()),
            "lesson {place}"
        );
    }
    Ok(())
}

#[tokio::test]
async fn a_lessons_quiz_job_takes_the_lessons_rank() -> Result<()> {
    let pool = crate::tests::pool().await?;
    let (_, job) = repository_tests::queued_job(&pool).await;
    let lessons = lesson_ids_in_course_order(&pool, &job.program_id).await;
    for (place, lesson) in lessons.iter().enumerate().rev() {
        let mut tx = pool.begin_write().await.map_err(crate::persistence::db)?;
        crate::quiz_bank_jobs::queue_lesson_bank_in(&mut tx, &job.program_id, lesson).await?;
        tx.commit().await.map_err(crate::persistence::db)?;
        let quiz = quiz_jobs(&pool, &job.program_id)
            .await?
            .into_iter()
            .find(|quiz| quiz.requested["scope"]["id"] == serde_json::json!(lesson))
            .unwrap();
        assert_eq!(
            quiz.rank,
            Some(i64::try_from(place).unwrap()),
            "lesson {place}"
        );
    }
    Ok(())
}

/// Invariant 6: the lesson worker loads its model, and so makes every model
/// call, inside the job's rank.
#[tokio::test]
async fn the_lesson_worker_runs_inside_its_jobs_rank() -> Result<()> {
    let pool = crate::tests::pool().await?;
    let (repo, first) = repository_tests::queued_job(&pool).await;
    let lessons = lesson_ids_in_course_order(&pool, &first.program_id).await;
    let third = repo
        .start_job(&crate::plan_dto::StartLearningGenerationJobRequestDto {
            operation_id: uuid::Uuid::new_v4().to_string(),
            program_id: first.program_id.clone(),
            expected_revision: i64::from(first.base_revision_number),
            kind: crate::curriculum::LearningGenerationJobKind::LessonPreparation,
            request_json: serde_json::json!({ "lessonIds": [lessons[2]] }).to_string(),
            progress_total: 1,
        })
        .await?;
    let rank_at_load = Arc::new(Mutex::new(None));
    let entered = Arc::new(tokio::sync::Notify::new());
    let worker = LessonGenerationWorker {
        runtimes: crate::embedded_runtime::BuiltinRuntimes::for_tests(),
        pool: pool.clone(),
        load_llm: {
            let (rank_at_load, entered) = (rank_at_load.clone(), entered.clone());
            Arc::new(move || {
                let (rank_at_load, entered) = (rank_at_load.clone(), entered.clone());
                Box::pin(async move {
                    *rank_at_load.lock().unwrap() = Some(
                        lattice_core::shared::runtime::jobs::current_rank()
                            .map(|ticket| ticket.rank()),
                    );
                    entered.notify_one();
                    std::future::pending().await
                })
            })
        },
        courses: None,
        refresh_sources: Arc::new(|_| Box::pin(async { Ok(()) })),
        research_web: None,
        verifier: None,
    };
    let runtime = JobRuntime::new(pool.clone());
    runtime
        .register(LESSON_PREPARATION, Arc::new(worker), lesson_job_config())
        .await?;
    runtime.dispatch(&third.id);
    tokio::time::timeout(std::time::Duration::from_secs(10), entered.notified())
        .await
        .unwrap();
    runtime.close();
    runtime.drain().await;
    assert_eq!(*rank_at_load.lock().unwrap(), Some(Some(2)));
    Ok(())
}

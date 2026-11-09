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
        if let Some(plan) = crate::lessons::course_plan::fixture_response(user) {
            return Ok(CompletionResponse::from_text(plan));
        }
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

/// Plans the course's topics on `planner`.
pub(crate) async fn plan_course(
    pool: &SqlitePool,
    program_id: &str,
    planner: &dyn LLMPort,
) -> Result<crate::dto::LearningProgramDto> {
    let program = LearningRepository::new(pool.clone())
        .get(program_id)
        .await?;
    let revision = crate::lessons::course_plan::outline_revision(pool, program_id).await?;
    crate::lessons::course_plan::ensure(pool, planner, &program, revision).await?;
    Ok(program)
}

/// Plans the course on `planner` and saves the first lesson's prepared,
/// verified teaching as `job_id`'s checkpoint. Returns the lesson's id.
pub(crate) async fn save_verified(
    pool: &SqlitePool,
    job_id: &str,
    program_id: &str,
    planner: &dyn LLMPort,
) -> Result<String> {
    let program = plan_course(pool, program_id, planner).await?;
    let revision = crate::lessons::course_plan::outline_revision(pool, program_id).await?;
    let plan = crate::lessons::course_plan::stored(pool, &program, revision)
        .await?
        .unwrap();
    let sources = LearningRepository::new(pool.clone())
        .verification_sources(program_id)
        .await?;
    let lesson = &program.modules[0].lessons[0];
    let prepared = prepared(lesson, &sources[0].id);
    LearningCurriculumRepository::new(pool.clone())
        .save_lesson_checkpoint(
            job_id,
            &lesson.id,
            "prepared-v2",
            serde_json::json!({
                "plan": crate::content_verification::digest(&serde_json::to_string(&plan)?),
                "lesson": prepared,
            }),
        )
        .await?;
    Ok(lesson.id.clone())
}

/// Runs the course's first lesson job through the real worker and runtime
/// with its lesson already prepared and verified, as a resumed job has it,
/// and `model` as the Learning model.
pub(crate) async fn run_lesson_job(pool: &SqlitePool, model: Arc<Recorder>) -> Result<Published> {
    let (_, job) = repository_tests::queued_job(pool).await;
    let store = JobStore::new(pool.clone());
    store.claim(&job.id).await?.unwrap();
    let lesson_id = save_verified(pool, &job.id, &job.program_id, model.as_ref()).await?;
    store.suspend(&job.id).await?;
    let llm: Arc<dyn LLMPort> = model;
    let worker = LessonGenerationWorker {
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
        lesson_id,
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

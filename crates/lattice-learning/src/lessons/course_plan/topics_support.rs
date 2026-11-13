//! What the tests of the planned topics share: a model that plans and
//! changes topics and keeps its requests, programs with a saved plan, and a
//! snapshot of every row a change may touch.
#![allow(clippy::unwrap_used, clippy::panic, clippy::indexing_slicing)]
use super::edit::CHANGE_TASK;
use super::job::{course_plan_job_config, CoursePlanWorker, COURSE_PLAN};
use super::{ensure, fixture_response, outline_revision, PlannedLesson};
use crate::dto::{AcceptLearningProgramRequestDto, LearningProgramDto};
use crate::plan_dto::StartLearningGenerationJobRequestDto;
use crate::repository::LearningRepository;
use lattice_core::application::ports::llm_port::{CompletionRequest, CompletionResponse};
use lattice_core::application::ports::LLMPort;
use lattice_core::shared::error::Result;
use lattice_core::shared::persistence::Database;
use lattice_core::shared::runtime::jobs::{JobRuntime, JobStore};
use serde_json::{json, Value};
use sqlx::SqlitePool;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

/// A model that plans the course it is asked about and changes the topics it
/// is asked to change, keeping every request.
pub(super) struct Topics {
    requests: Mutex<Vec<CompletionRequest>>,
    /// Answers to give to changes, first to last, before the default one.
    answers: Mutex<Vec<String>>,
    calls: AtomicUsize,
    /// Run while the first change is being answered: a statement, given the
    /// course id.
    meanwhile: Mutex<Option<(SqlitePool, String, String)>>,
}

impl Topics {
    pub(super) fn new() -> Self {
        Self {
            requests: Mutex::new(Vec::new()),
            answers: Mutex::new(Vec::new()),
            calls: AtomicUsize::new(0),
            meanwhile: Mutex::new(None),
        }
    }

    pub(super) fn answering(answers: Vec<String>) -> Self {
        let model = Self::new();
        *model.answers.lock().unwrap() = answers.into_iter().rev().collect();
        model
    }

    /// Runs `sql`, given `program_id`, while the first change is being answered.
    pub(super) fn meanwhile(self, pool: &SqlitePool, sql: &str, program_id: &str) -> Self {
        *self.meanwhile.lock().unwrap() =
            Some((pool.clone(), sql.to_owned(), program_id.to_owned()));
        self
    }

    pub(super) fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }

    pub(super) fn request(&self, index: usize) -> CompletionRequest {
        self.requests.lock().unwrap()[index].clone()
    }

    /// The task of request `index`, as the model reads it.
    pub(super) fn task(&self, index: usize) -> Value {
        let request = self.request(index);
        let (_, user) = crate::lessons::structured_call::task_messages(&request);
        serde_json::from_str(user).unwrap()
    }
}

/// An answer that gives `lessons` the idea `idea`.
pub(super) fn change_answer(lessons: &[&str], idea: &str) -> String {
    json!({"lessons":lessons.iter().map(|id| json!({
        "lessonId":id,"ideas":[format!("{idea} of {id}")],"terms":["a changed term"],
        "actions":[],"contrast":"a changed confusion"
    })).collect::<Vec<_>>()})
    .to_string()
}

#[async_trait::async_trait]
impl LLMPort for Topics {
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.requests.lock().unwrap().push(request.clone());
        let (_, user) = crate::lessons::structured_call::task_messages(request);
        if let Some(plan) = fixture_response(user) {
            return Ok(CompletionResponse::from_text(plan));
        }
        let task: Value = serde_json::from_str(user).unwrap();
        assert_eq!(task["task"], CHANGE_TASK, "not a request this model knows");
        let hook = self.meanwhile.lock().unwrap().take();
        if let Some((pool, sql, program_id)) = hook {
            sqlx::query(sqlx::AssertSqlSafe(sql))
                .bind(program_id)
                .execute(&pool)
                .await
                .unwrap();
        }
        let given = self.answers.lock().unwrap().pop();
        let text = given.unwrap_or_else(|| {
            let ids: Vec<&str> = task["lessonsToChange"]
                .as_array()
                .unwrap()
                .iter()
                .map(|id| id.as_str().unwrap())
                .collect();
            change_answer(&ids, "a changed idea")
        });
        Ok(CompletionResponse::from_text(text))
    }
    fn model_name(&self) -> &str {
        "topics-fixture"
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

pub(super) async fn saved_program(db: &Database) -> LearningProgramDto {
    let program = crate::tests::fixture();
    LearningRepository::new(db.clone())
        .create(&program)
        .await
        .unwrap();
    program
}

pub(super) fn ids(program: &LearningProgramDto) -> Vec<String> {
    super::lesson_ids(program)
        .into_iter()
        .map(str::to_owned)
        .collect()
}

/// A saved course whose topics are planned, with the outline revision.
pub(super) async fn planned_program(db: &Database, model: &Topics) -> (LearningProgramDto, i64) {
    let program = saved_program(db).await;
    let revision = outline_revision(db, &program.summary.id).await.unwrap();
    ensure(db, model, &program, revision, &Default::default())
        .await
        .unwrap();
    (program, revision)
}

pub(super) fn topics_of(lesson_id: &str, idea: &str) -> PlannedLesson {
    PlannedLesson {
        lesson_id: lesson_id.to_owned(),
        ideas: vec![idea.to_owned()],
        terms: vec!["a term of the learner".to_owned()],
        actions: Vec::new(),
        contrast: String::new(),
    }
}

/// Accepts the course if it is a draft and starts a lesson-preparation job
/// for `lesson_id`, which is running when `running` is true.
pub(super) async fn lesson_job(
    db: &Database,
    program: &LearningProgramDto,
    lesson_id: &str,
    running: bool,
) -> String {
    let repo = LearningRepository::new(db.clone());
    let current = repo.get(&program.summary.id).await.unwrap();
    if current.summary.status == crate::dto::LearningProgramStatus::Draft {
        repo.accept(&AcceptLearningProgramRequestDto {
            program_id: program.summary.id.clone(),
            expected_revision: 0,
            title: program.summary.title.clone(),
        })
        .await
        .unwrap();
    }
    let program = repo.get(&program.summary.id).await.unwrap();
    let job = crate::curriculum_repository::LearningCurriculumRepository::new(db.clone())
        .start_lesson_job(&StartLearningGenerationJobRequestDto {
            operation_id: uuid::Uuid::new_v4().to_string(),
            program_id: program.summary.id.clone(),
            expected_revision: program.summary.revision,
            kind: crate::curriculum::LearningGenerationJobKind::LessonPreparation,
            request_json: json!({"lessonIds":[lesson_id]}).to_string(),
            progress_total: 1,
        })
        .await
        .unwrap();
    if running {
        assert!(JobStore::new(db.clone())
            .claim(&job.id)
            .await
            .unwrap()
            .is_some());
    }
    job.id
}

/// Runs the course-plan job `job_id` to its end on `llm`.
pub(super) async fn run_plan_job(db: &Database, llm: Arc<dyn LLMPort>, job_id: &str) {
    let worker = CoursePlanWorker {
        database: db.clone(),
        load_llm: Arc::new(move || {
            let llm = llm.clone();
            Box::pin(async move { Ok(crate::generation_jobs::pinned_for_test(llm)) })
        }),
        course_setups: Default::default(),
    };
    let runtime = JobRuntime::new(db.clone());
    runtime
        .register(COURSE_PLAN, Arc::new(worker), course_plan_job_config())
        .await
        .unwrap();
    runtime.dispatch(job_id);
    runtime.finished(job_id).await;
    runtime.close();
    runtime.drain().await;
}

/// Every plan entry and every replaced entry of a course, in a fixed order.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct Snapshot {
    pub entries: Vec<(String, i64, String, i64)>,
    pub history: Vec<(String, i64, i64, String)>,
}

pub(super) async fn snapshot(pool: &SqlitePool, program_id: &str) -> Snapshot {
    Snapshot {
        entries: sqlx::query_as("SELECT lesson_id,outline_revision,entry_json,entry_revision FROM learning_course_plan_lessons WHERE program_id=? ORDER BY lesson_id,outline_revision")
            .bind(program_id)
            .fetch_all(pool)
            .await
            .unwrap(),
        history: sqlx::query_as("SELECT lesson_id,outline_revision,entry_revision,entry_json FROM learning_course_plan_entry_history WHERE program_id=? ORDER BY lesson_id,outline_revision,entry_revision")
            .bind(program_id)
            .fetch_all(pool)
            .await
            .unwrap(),
    }
}

/// Changes the course's goal, which moves its outline revision.
pub(super) async fn change_outline(pool: &SqlitePool, program_id: &str) {
    sqlx::query("UPDATE learning_programs SET goal=goal||' (changed)' WHERE id=?")
        .bind(program_id)
        .execute(pool)
        .await
        .unwrap();
}

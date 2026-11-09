//! A lesson job reads the course's plan before it writes, so what a lesson is
//! written from never depends on another lesson's output.
#![allow(clippy::unwrap_used, clippy::panic, clippy::indexing_slicing)]
use super::*;
use crate::lessons::course_plan::{fixture_response, PLAN_TASK};
use lattice_core::application::ports::llm_port::{CompletionRequest, CompletionResponse};
use lattice_core::application::ports::LLMPort;
use std::sync::Mutex;

/// Plans the course when asked and refuses every other request, which ends the
/// job at its first request after the plan. It keeps each request's task.
struct PlansThenStops {
    tasks: Mutex<Vec<String>>,
}

#[async_trait::async_trait]
impl LLMPort for PlansThenStops {
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        let (_, user) = crate::lessons::structured_call::task_messages(request);
        let task = serde_json::from_str::<serde_json::Value>(user)
            .ok()
            .and_then(|value| value["task"].as_str().map(str::to_owned))
            .unwrap_or_else(|| "not a task".to_owned());
        self.tasks.lock().unwrap().push(task);
        match fixture_response(user) {
            Some(plan) => Ok(CompletionResponse::from_text(plan)),
            None => Err(AppError::InvalidState("stopped after the plan".into())),
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
}

#[tokio::test]
async fn a_lesson_job_makes_the_course_plan_before_the_lesson_is_written() {
    let pool = crate::tests::pool().await.unwrap();
    let (_, job) = repository_tests::queued_job(&pool).await;
    let model = Arc::new(PlansThenStops {
        tasks: Mutex::new(Vec::new()),
    });
    let llm: Arc<dyn LLMPort> = model.clone();
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

    run_attempt(worker, &job.id).await.unwrap();

    let tasks = model.tasks.lock().unwrap().clone();
    assert_eq!(
        tasks.first().map(String::as_str),
        Some(PLAN_TASK),
        "{tasks:?}"
    );
    assert!(
        tasks.len() > 1 && tasks[1..].iter().all(|task| task != PLAN_TASK),
        "the plan is made once, then the lesson's own calls follow: {tasks:?}"
    );
    let revision = crate::lessons::course_plan::outline_revision(&pool, &job.program_id)
        .await
        .unwrap();
    let saved: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM learning_course_plan_lessons WHERE program_id=? AND outline_revision=?",
    )
    .bind(&job.program_id)
    .bind(revision)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(saved, 4, "a row for each lesson of the outline");
}

/// Answers the lesson's evidence plan, keeps what else it is asked, and
/// refuses that, which ends the job at the lesson writer's request.
struct AnswersEvidenceThenStops {
    asked: Mutex<Vec<String>>,
}

#[async_trait::async_trait]
impl LLMPort for AnswersEvidenceThenStops {
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        let (_, user) = crate::lessons::structured_call::task_messages(request);
        if user.contains("referenceCatalog") {
            return Ok(CompletionResponse::from_text(
                r#"{"topics":[{"topic":"the central idea","queries":["central idea"]}]}"#
                    .to_owned(),
            ));
        }
        self.asked.lock().unwrap().push(user.to_owned());
        Err(AppError::InvalidState("stopped at the writer".into()))
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

#[tokio::test]
async fn a_lesson_job_writes_from_the_entry_the_learner_edited() {
    use crate::lessons::course_plan::{
        edit_entry, ensure, outline_revision, EditCoursePlanEntryRequestDto, PlannedLesson,
    };
    let pool = crate::tests::pool().await.unwrap();
    let (_, job) = repository_tests::queued_job(&pool).await;
    let program = LearningRepository::new(pool.clone())
        .get(&job.program_id)
        .await
        .unwrap();
    let revision = outline_revision(&pool, &job.program_id).await.unwrap();
    let planner = PlansThenStops {
        tasks: Mutex::new(Vec::new()),
    };
    ensure(&pool, &planner, &program, revision).await.unwrap();
    edit_entry(
        &pool,
        EditCoursePlanEntryRequestDto {
            program_id: job.program_id.clone(),
            outline_revision: revision,
            expected_entry_revision: 1,
            topics: PlannedLesson {
                lesson_id: program.modules[0].lessons[0].id.clone(),
                ideas: vec!["an idea the learner wrote".into()],
                terms: Vec::new(),
                actions: Vec::new(),
                contrast: String::new(),
            },
        },
    )
    .await
    .unwrap();
    let model = Arc::new(AnswersEvidenceThenStops {
        asked: Mutex::new(Vec::new()),
    });
    let llm: Arc<dyn LLMPort> = model.clone();
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

    run_attempt(worker, &job.id).await.unwrap();

    let asked = model.asked.lock().unwrap().clone();
    let writer = asked
        .iter()
        .find(|user| user.contains("coursePlan"))
        .unwrap_or_else(|| panic!("the lesson writer was never asked: {asked:?}"));
    let request: serde_json::Value = serde_json::from_str(writer).unwrap();
    let planned = request["courseSequence"]["coursePlan"].as_array().unwrap();
    assert_eq!(planned[0]["position"], "this");
    assert_eq!(planned[0]["ideas"][0], "an idea the learner wrote");
    assert_eq!(planned[1]["ideas"][0], "the central idea");
    assert!(
        asked.iter().all(|user| !user.contains(PLAN_TASK)),
        "the plan is read, not made again"
    );
}

fn worker_on(pool: &SqlitePool, llm: Arc<dyn LLMPort>) -> LessonGenerationWorker {
    LessonGenerationWorker {
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
    }
}

/// A lesson job that stopped before it published: claimed, then suspended,
/// with the course planned and, when `verified`, its first lesson prepared
/// and verified. Returns the job's id, its course and the lesson.
async fn stopped_job(pool: &SqlitePool, verified: bool) -> (String, String, String) {
    let (_, job) = repository_tests::queued_job(pool).await;
    let store = lattice_core::shared::runtime::jobs::JobStore::new(pool.clone());
    assert!(store.claim(&job.id).await.unwrap().is_some());
    let planner = PlansThenStops {
        tasks: Mutex::new(Vec::new()),
    };
    let lesson = if verified {
        overlap_tests::save_verified(pool, &job.id, &job.program_id, &planner)
            .await
            .unwrap()
    } else {
        let program = overlap_tests::plan_course(pool, &job.program_id, &planner)
            .await
            .unwrap();
        program.modules[0].lessons[0].id.clone()
    };
    store.suspend(&job.id).await.unwrap();
    (job.id, job.program_id, lesson)
}

async fn learner_rewrites(pool: &SqlitePool, program_id: &str, lesson_id: &str) {
    use crate::lessons::course_plan::{
        edit_entry, outline_revision, EditCoursePlanEntryRequestDto, PlannedLesson,
    };
    let revision = outline_revision(pool, program_id).await.unwrap();
    edit_entry(
        pool,
        EditCoursePlanEntryRequestDto {
            program_id: program_id.to_owned(),
            outline_revision: revision,
            expected_entry_revision: 1,
            topics: PlannedLesson {
                lesson_id: lesson_id.to_owned(),
                ideas: vec!["an idea the learner wrote".into()],
                terms: Vec::new(),
                actions: Vec::new(),
                contrast: String::new(),
            },
        },
    )
    .await
    .unwrap();
}

/// The first idea the lesson writer was given for the lesson it writes, or
/// `None` when it was never asked.
fn written_from(model: &AnswersEvidenceThenStops) -> Option<String> {
    let asked = model.asked.lock().unwrap().clone();
    let writer = asked.iter().find(|user| user.contains("coursePlan"))?;
    let request: serde_json::Value = serde_json::from_str(writer).unwrap();
    request["courseSequence"]["coursePlan"][0]["ideas"][0]
        .as_str()
        .map(str::to_owned)
}

async fn ready_lessons(pool: &SqlitePool, lesson_id: &str) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM learning_lessons WHERE id=? AND preparation='ready'")
        .bind(lesson_id)
        .fetch_one(pool)
        .await
        .unwrap()
}

#[tokio::test]
async fn a_verified_lesson_is_written_again_when_its_entry_was_edited_before_the_retry() {
    let pool = crate::tests::pool().await.unwrap();
    let (job, program_id, lesson) = stopped_job(&pool, true).await;
    learner_rewrites(&pool, &program_id, &lesson).await;
    let model = Arc::new(AnswersEvidenceThenStops {
        asked: Mutex::new(Vec::new()),
    });

    run_attempt(worker_on(&pool, model.clone()), &job)
        .await
        .unwrap();

    assert_eq!(
        written_from(&model).as_deref(),
        Some("an idea the learner wrote"),
        "the old lesson was published instead of one written from the edit"
    );
    assert_eq!(ready_lessons(&pool, &lesson).await, 0);
}

#[tokio::test]
async fn a_verified_lesson_is_reused_when_the_plan_is_unchanged() {
    let pool = crate::tests::pool().await.unwrap();
    let (job, _, lesson) = stopped_job(&pool, true).await;
    let model = Arc::new(AnswersEvidenceThenStops {
        asked: Mutex::new(Vec::new()),
    });

    run_attempt(worker_on(&pool, model.clone()), &job)
        .await
        .unwrap();

    assert_eq!(written_from(&model), None, "the writer was asked again");
    assert_eq!(ready_lessons(&pool, &lesson).await, 1);
}

#[tokio::test]
async fn a_lesson_stopped_while_drafting_is_written_from_the_edited_entry() {
    let pool = crate::tests::pool().await.unwrap();
    let (job, program_id, lesson) = stopped_job(&pool, false).await;
    learner_rewrites(&pool, &program_id, &lesson).await;
    let model = Arc::new(AnswersEvidenceThenStops {
        asked: Mutex::new(Vec::new()),
    });

    run_attempt(worker_on(&pool, model.clone()), &job)
        .await
        .unwrap();

    assert_eq!(
        written_from(&model).as_deref(),
        Some("an idea the learner wrote")
    );
}

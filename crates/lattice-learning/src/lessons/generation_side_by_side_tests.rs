//! The lessons of one course prepare side by side and plan the course once.
#![allow(clippy::unwrap_used, clippy::panic, clippy::indexing_slicing)]
use super::*;
use crate::{
    curriculum_repository::{lesson_job_config, LESSON_PREPARATION},
    lessons::course_plan::{course_plan_job_config, CoursePlanWorker, COURSE_PLAN},
    lessons::course_setup::CourseSetups,
};
use lattice_core::application::ports::llm_port::{CompletionRequest, CompletionResponse};
use lattice_core::application::ports::LLMPort;
use lattice_core::shared::runtime::jobs::{JobContext, JobOutcome, JobRuntime, NewJob};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

const BOUND: Duration = Duration::from_secs(30);

/// Waits at a barrier for another job, up to a bound, and counts those that met.
struct Meet {
    barrier: tokio::sync::Barrier,
    met: AtomicUsize,
}

#[async_trait::async_trait]
impl lattice_core::shared::runtime::jobs::JobHandler for Meet {
    async fn run(&self, _: &JobContext) -> Result<JobOutcome> {
        if tokio::time::timeout(Duration::from_secs(2), self.barrier.wait())
            .await
            .is_ok()
        {
            self.met.fetch_add(1, Ordering::SeqCst);
        }
        Ok(JobOutcome::Completed {
            result_ref: "met".into(),
            message: "Done".into(),
        })
    }
}

#[tokio::test]
async fn two_lesson_jobs_of_one_course_run_at_the_same_time() -> Result<()> {
    let pool = crate::tests::pool().await?;
    let runtime = JobRuntime::new(Database::from(pool));
    let meet = Arc::new(Meet {
        barrier: tokio::sync::Barrier::new(2),
        met: AtomicUsize::new(0),
    });
    runtime
        .register(LESSON_PREPARATION, meet.clone(), lesson_job_config())
        .await?;
    let mut ids = Vec::new();
    for operation in ["first", "second"] {
        let job = runtime
            .submit(&NewJob {
                kind: LESSON_PREPARATION.into(),
                subject_id: Some("one-course".into()),
                operation_id: operation.into(),
                payload_hash: operation.into(),
                requested: serde_json::json!({}),
                progress_total: 1,
                message: "Queued".into(),
            })
            .await?;
        ids.push(job.id);
    }
    for id in &ids {
        tokio::time::timeout(BOUND, runtime.finished(id))
            .await
            .unwrap();
    }
    runtime.close();
    runtime.drain().await;
    assert_eq!(
        meet.met.load(Ordering::SeqCst),
        2,
        "the lesson jobs of one course did not both run before either finished: \
         a course's lessons wait for each other"
    );
    Ok(())
}

/// Answers a plan request once at least two callers are asking (or after a
/// bound), and refuses every other request.
struct SlowPlanner {
    plan_calls: AtomicUsize,
}

#[async_trait::async_trait]
impl LLMPort for SlowPlanner {
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        let (_, user) = crate::lessons::structured_call::task_messages(request);
        let Some(plan) = crate::lessons::course_plan::fixture_response(user) else {
            return Err(AppError::InvalidInput("not a plan request".into()));
        };
        let call = self.plan_calls.fetch_add(1, Ordering::SeqCst) + 1;
        let _ = tokio::time::timeout(Duration::from_secs(3), async {
            while self.plan_calls.load(Ordering::SeqCst) < 2 {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await;
        Ok(CompletionResponse::from_text(plan.replace(
            "the central idea",
            &format!("the idea of call {call}"),
        )))
    }
    fn model_name(&self) -> &str {
        "slow-planner"
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

fn loader(llm: Arc<dyn LLMPort>) -> crate::quiz_bank_jobs::ModelLoader {
    Arc::new(move || {
        let llm = llm.clone();
        Box::pin(async move { Ok(pinned_for_test(llm)) })
    })
}

fn lesson_worker(
    pool: &SqlitePool,
    llm: &Arc<dyn LLMPort>,
    setups: &CourseSetups,
) -> LessonGenerationWorker {
    LessonGenerationWorker {
        course_setups: setups.clone(),
        runtimes: crate::embedded_runtime::BuiltinRuntimes::for_tests(),
        database: Database::from(pool.clone()),
        load_llm: loader(llm.clone()),
        courses: None,
        refresh_sources: Arc::new(|_| Box::pin(async { Ok(()) })),
        research_web: None,
        verifier: None,
    }
}

fn plan_worker(
    pool: &SqlitePool,
    llm: &Arc<dyn LLMPort>,
    setups: &CourseSetups,
) -> CoursePlanWorker {
    CoursePlanWorker {
        database: Database::from(pool.clone()),
        load_llm: loader(llm.clone()),
        course_setups: setups.clone(),
    }
}

/// Runs a lesson job and the course-plan job of a course with no plan, side
/// by side on one runtime, until both finish. Returns the course's id.
async fn plan_from_both_paths(pool: &SqlitePool, planner: &Arc<SlowPlanner>) -> Result<String> {
    let (_, lesson) = repository_tests::queued_job(pool).await;
    let llm: Arc<dyn LLMPort> = planner.clone();
    let setups = CourseSetups::default();
    let runtime = JobRuntime::new(Database::from(pool.clone()));
    runtime
        .register(
            LESSON_PREPARATION,
            Arc::new(lesson_worker(pool, &llm, &setups)),
            lesson_job_config(),
        )
        .await?;
    runtime
        .register(
            COURSE_PLAN,
            Arc::new(plan_worker(pool, &llm, &setups)),
            course_plan_job_config(),
        )
        .await?;
    let plan_job =
        crate::lessons::course_plan::queue(&Database::from(pool.clone()), &lesson.program_id)
            .await?
            .unwrap();
    runtime.dispatch(&plan_job);
    runtime.dispatch(&lesson.id);
    for id in [&plan_job, &lesson.id] {
        tokio::time::timeout(BOUND, runtime.finished(id))
            .await
            .unwrap_or_else(|_| {
                panic!("job {id} did not finish: a lesson job and the plan job wait for each other")
            });
    }
    runtime.close();
    runtime.drain().await;
    Ok(lesson.program_id)
}

#[tokio::test]
async fn a_lesson_job_and_the_plan_job_make_one_model_call_for_the_course() -> Result<()> {
    let pool = crate::tests::pool().await?;
    let planner = Arc::new(SlowPlanner {
        plan_calls: AtomicUsize::new(0),
    });
    let program_id = plan_from_both_paths(&pool, &planner).await?;

    assert_eq!(
        planner.plan_calls.load(Ordering::SeqCst),
        1,
        "a course's plan is made by one model call however many callers ask"
    );
    let rows: Vec<(String,)> =
        sqlx::query_as("SELECT entry_json FROM learning_course_plan_lessons WHERE program_id=?")
            .bind(&program_id)
            .fetch_all(&pool)
            .await
            .unwrap();
    let ideas: std::collections::HashSet<_> = rows
        .iter()
        .map(|(entry,)| {
            serde_json::from_str::<serde_json::Value>(entry).unwrap()["ideas"].to_string()
        })
        .collect();
    assert_eq!(
        ideas.len(),
        1,
        "the entries come from several answers: {ideas:?}"
    );
    Ok(())
}

#[tokio::test]
async fn a_lesson_job_of_a_course_with_no_plan_plans_it_and_finishes() -> Result<()> {
    let pool = crate::tests::pool().await?;
    let planner = Arc::new(SlowPlanner {
        plan_calls: AtomicUsize::new(1),
    });
    let (_, lesson) = repository_tests::queued_job(&pool).await;
    let llm: Arc<dyn LLMPort> = planner;
    let runtime = JobRuntime::new(Database::from(pool.clone()));
    runtime
        .register(
            LESSON_PREPARATION,
            Arc::new(lesson_worker(&pool, &llm, &CourseSetups::default())),
            lesson_job_config(),
        )
        .await?;
    runtime.dispatch(&lesson.id);
    tokio::time::timeout(BOUND, runtime.finished(&lesson.id))
        .await
        .expect("a lesson job of a course with no plan did not finish");
    runtime.close();
    runtime.drain().await;

    let planned: i64 =
        sqlx::query_scalar("SELECT count(*) FROM learning_course_plan_lessons WHERE program_id=?")
            .bind(&lesson.program_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(
        planned > 0,
        "the lesson job finished without planning the course"
    );
    Ok(())
}

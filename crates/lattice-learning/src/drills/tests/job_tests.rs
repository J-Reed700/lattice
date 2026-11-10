//! The `learning.drill_build` job on the job runtime: what it is queued with, how it ends with the judge that grades
//! reviews, and that cancelling it stops the model and stores nothing.

use super::support::{
    blind_answer, blind_assessment, built_drill, clean_code_answer, code_check_answer, code_spec,
    entry_answer, grade_answer, judge_answer, lesson_spec, messages, plant_answer, review_answer,
    ScriptedModel,
};
use crate::drills::bank::queue_build;
use crate::drills::jobs::{drill_job_config, judge_grader, spec_of, DrillBuildWorker, DRILL_BUILD};
use crate::embedded_runtime::BuiltinRuntimes;
use crate::generation_jobs::pinned_for_test;
use crate::persistence::db;
use crate::practice::practice_tests::text_length_rules;
use crate::tests::pool;
use async_trait::async_trait;
use lattice_core::application::ports::llm_port::{CompletionRequest, CompletionResponse};
use lattice_core::application::ports::LLMPort;
use lattice_core::domain::learning::drills::Proof;
use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::runtime::jobs::{
    Backoff, JobRuntime, JobStatus, JobStore, NewJob, RecoveryPolicy,
};
use serde_json::json;
use sqlx::SqlitePool;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// A worker on `model`, without the built-in runtimes, so that every defect is proven by judgement, and with the
/// judge that grades reviews, as the app wires it.
fn worker(pool: &SqlitePool, model: Arc<dyn LLMPort>) -> DrillBuildWorker {
    DrillBuildWorker {
        pool: pool.clone().into(),
        load_llm: Arc::new(move || {
            let model = model.clone();
            Box::pin(async move { Ok(pinned_for_test(model)) })
        }),
        runtimes: BuiltinRuntimes::new(None),
        grader: judge_grader,
    }
}

async fn drills(pool: &SqlitePool) -> Result<i64> {
    sqlx::query_scalar("SELECT count(*) FROM learning_drills")
        .fetch_one(pool)
        .await
        .map_err(db)
}

#[test]
fn builds_are_one_per_language_and_resume_after_a_restart() {
    let config = drill_job_config();
    assert_eq!(config.recovery, RecoveryPolicy::Requeue);
    assert!(config.exclusive_per_subject);
    assert_eq!(config.backoff, Backoff::default());
    assert_eq!(DRILL_BUILD, "learning.drill_build");
}

#[tokio::test]
async fn a_job_names_the_drill_it_builds() -> Result<()> {
    let pool = pool().await?;
    let store = JobStore::new(pool.clone());
    for requested in [
        json!({}),
        json!({"spec": {"language": "klingon", "domain": null, "lines": 40, "patterns": ["readability.magic_number"], "programId": null}}),
    ] {
        let admitted = store
            .submit(&NewJob {
                kind: DRILL_BUILD.into(),
                subject_id: Some("python".into()),
                operation_id: uuid::Uuid::new_v4().to_string(),
                payload_hash: "h".into(),
                requested,
                progress_total: 7,
                message: "Waiting".into(),
            })
            .await?;
        let job = store.get(&admitted.id).await?;
        assert!(matches!(spec_of(&job), Err(AppError::InvalidInput(_))));
    }
    let queued = queue_build(&pool, &lesson_spec(), "named").await?;
    assert_eq!(spec_of(&store.get(&queued).await?)?, lesson_spec());
    Ok(())
}

#[tokio::test]
async fn a_build_queued_before_drills_had_lessons_is_refused_by_its_spec() -> Result<()> {
    let pool = pool().await?;
    let store = JobStore::new(pool.clone());
    let admitted = store
        .submit(&NewJob {
            kind: DRILL_BUILD.into(),
            subject_id: Some("python".into()),
            operation_id: uuid::Uuid::new_v4().to_string(),
            payload_hash: "h".into(),
            requested: json!({"spec": {"language": "python", "domain": null, "lines": 40, "patterns": ["readability.magic_number"], "programId": null}}),
            progress_total: 7,
            message: "Waiting".into(),
        })
        .await?;
    let job = store.get(&admitted.id).await?;
    let error = spec_of(&job).unwrap_err();
    assert!(
        matches!(&error, AppError::InvalidInput(message) if message.contains("lesson")),
        "{error:?}"
    );
    Ok(())
}

/// Runs the job through a runtime of its own, as the app would after a queue.
async fn run_to_the_end(pool: &SqlitePool, worker: DrillBuildWorker, id: &str) -> Result<()> {
    let runtime = JobRuntime::new(pool.clone());
    runtime
        .register(DRILL_BUILD, Arc::new(worker), drill_job_config())
        .await?;
    runtime.submitted(id).await;
    runtime.finished(id).await;
    runtime.close();
    runtime.drain().await;
    Ok(())
}

#[tokio::test]
async fn a_queued_build_runs_on_the_runtime_and_the_judge_grades_its_blind_review() -> Result<()> {
    let pool = pool().await?;
    let model = Arc::new(ScriptedModel::new([
        code_check_answer(true),
        clean_code_answer(),
        review_answer(vec![]),
        plant_answer(),
        judge_answer(true),
        judge_answer(true),
        blind_answer(),
        grade_answer(),
        entry_answer(),
    ]));
    let spec = code_spec(&pool).await?;
    let id = queue_build(&pool, &spec, "end to end").await?;
    run_to_the_end(&pool, worker(&pool, model.clone()), &id).await?;

    let job = JobStore::new(pool.clone()).get(&id).await?;
    assert_eq!(job.status, JobStatus::Completed, "{:?}", job.error_message);
    assert_eq!(
        model.purposes(),
        [
            "code fit check",
            "clean code",
            "clean review",
            "plant",
            "judge",
            "judge",
            "blind review",
            "grade",
            "key entry"
        ],
        "without a runner both defects are judged, and one call grades the review"
    );
    let stored = built_drill(&pool).await?.expect("the drill is in the bank");
    assert_eq!(job.result_ref, Some(format!("drill:{}", stored.drill.id)));
    assert_eq!(stored.suite, None);
    assert_eq!(stored.runner, None);
    assert_eq!(stored.drill.key.len(), 3);
    assert!(stored
        .drill
        .key
        .iter()
        .all(|issue| matches!(issue.proof, Proof::Judged { .. })));
    assert_eq!(
        stored.evidence.blind_review.assessment,
        Some(blind_assessment()),
        "the judge's grade of the review is kept as evidence"
    );
    assert_eq!(stored.drill.key[2].id, "review-1");

    let requests = model.requests();
    let grade = messages(&requests[7]);
    assert!(
        grade[1]
            .1
            .contains("A caller gets one call more than the limit."),
        "the judge reads the key"
    );
    assert!(
        grade[1]
            .1
            .contains("This lets one call more than the limit through."),
        "and the review"
    );
    for request in &requests {
        let schema = request
            .json_schema
            .as_ref()
            .expect("every call is structured");
        assert!(text_length_rules(schema).is_empty(), "{schema}");
    }
    Ok(())
}

#[tokio::test]
async fn a_model_outage_defers_the_build_and_stores_nothing() -> Result<()> {
    let pool = pool().await?;
    let id = queue_build(&pool, &code_spec(&pool).await?, "outage").await?;
    let outage = DrillBuildWorker {
        pool: pool.clone().into(),
        load_llm: Arc::new(|| {
            Box::pin(async { Err(AppError::Network("The model is down.".into())) })
        }),
        runtimes: BuiltinRuntimes::new(None),
        grader: judge_grader,
    };
    run_to_the_end(&pool, outage, &id).await?;
    let job = JobStore::new(pool.clone()).get(&id).await?;
    assert_eq!(
        job.status,
        JobStatus::Pending,
        "an outage is retried, not failed"
    );
    assert!(job.retry_not_before.is_some());
    assert_eq!(drills(&pool).await?, 0);
    Ok(())
}

#[tokio::test]
async fn a_review_the_judge_cannot_grade_fails_the_build_with_the_work_before_it_saved(
) -> Result<()> {
    let pool = pool().await?;
    let nothing = || json!({"issues": [], "findings": []}).to_string();
    let model = Arc::new(ScriptedModel::new([
        code_check_answer(true),
        clean_code_answer(),
        review_answer(vec![]),
        plant_answer(),
        judge_answer(true),
        judge_answer(true),
        blind_answer(),
        nothing(),
        nothing(),
    ]));
    let spec = code_spec(&pool).await?;
    let id = queue_build(&pool, &spec, "ungradable").await?;
    run_to_the_end(&pool, worker(&pool, model.clone()), &id).await?;

    let store = JobStore::new(pool.clone());
    let job = store.get(&id).await?;
    assert_eq!(job.status, JobStatus::Failed);
    assert!(
        job.error_message
            .as_deref()
            .is_some_and(|message| message.contains("Issue 1 has no entry")),
        "{:?}",
        job.error_message
    );
    let purposes = model.purposes();
    assert_eq!(
        purposes[purposes.len() - 2..],
        ["grade", "grade"],
        "the judge is asked once more, never twice"
    );
    assert_eq!(drills(&pool).await?, 0);
    for stage in ["clean_code", "clean_review", "plant", "prove"] {
        assert!(
            store
                .checkpoint(&id, &format!("drill-build-v1:{stage}"))
                .await?
                .is_some(),
            "{stage}"
        );
    }
    assert!(store
        .checkpoint(&id, "drill-build-v1:blind_review")
        .await?
        .is_none());
    Ok(())
}

/// A model that never answers.
struct Hang {
    started: AtomicUsize,
}

#[async_trait]
impl LLMPort for Hang {
    async fn complete(&self, _request: &CompletionRequest) -> Result<CompletionResponse> {
        self.started.fetch_add(1, Ordering::SeqCst);
        std::future::pending::<Result<CompletionResponse>>().await
    }
    fn model_name(&self) -> &str {
        "hangs"
    }
    fn max_context_tokens(&self) -> usize {
        128_000
    }
    async fn is_ready(&self) -> Result<bool> {
        Ok(true)
    }
}

#[tokio::test]
async fn cancelling_a_build_stops_its_model_call_and_stores_nothing() -> Result<()> {
    let pool = pool().await?;
    let model = Arc::new(Hang {
        started: AtomicUsize::new(0),
    });
    let spec = code_spec(&pool).await?;
    let id = queue_build(&pool, &spec, "cancelled").await?;
    let runtime = JobRuntime::new(pool.clone());
    runtime
        .register(
            DRILL_BUILD,
            Arc::new(worker(&pool, model.clone())),
            drill_job_config(),
        )
        .await?;
    runtime.submitted(&id).await;
    for _ in 0..1_000 {
        if model.started.load(Ordering::SeqCst) > 0 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    assert_eq!(
        model.started.load(Ordering::SeqCst),
        1,
        "the build is waiting on the model"
    );
    runtime.cancel(&id).await?;
    runtime.finished(&id).await;
    runtime.close();
    runtime.drain().await;

    let store = JobStore::new(pool.clone());
    assert_eq!(store.get(&id).await?.status, JobStatus::Cancelled);
    assert_eq!(drills(&pool).await?, 0);
    assert!(store
        .checkpoint(&id, "drill-build-v1:clean_code")
        .await?
        .is_none());
    Ok(())
}

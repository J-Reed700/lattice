//! The `learning.drill_build` job on the job runtime: what it is queued with, how it ends, and that cancelling it
//! stops the model and stores nothing.

use super::support::{
    blind_answer, blind_assessment, clean_code_answer, entry_answer, judge_answer, plant_answer,
    review_answer, spec, FakeGrader, ScriptedModel,
};
use crate::features::learning::drills::bank::queue_build;
use crate::features::learning::drills::grader::ReviewGrader;
use crate::features::learning::drills::jobs::{
    drill_job_config, spec_of, unconnected_grader, DrillBuildWorker, GraderFactory, DRILL_BUILD,
};
use crate::features::learning::drills::repository;
use crate::features::learning::embedded_runtime::BuiltinRuntimes;
use crate::features::learning::generation_jobs::pinned_for_test;
use crate::features::learning::persistence::db;
use crate::features::learning::tests::pool;
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

/// A worker on `model`, without the built-in runtimes, so that every defect is proven by judgement.
fn worker(pool: &SqlitePool, model: Arc<dyn LLMPort>, grader: GraderFactory) -> DrillBuildWorker {
    DrillBuildWorker {
        pool: pool.clone(),
        load_llm: Arc::new(move || {
            let model = model.clone();
            Box::pin(async move { Ok(pinned_for_test(model)) })
        }),
        runtimes: BuiltinRuntimes::new(None),
        grader,
    }
}

fn judging(assessment: bool) -> GraderFactory {
    Arc::new(move |_llm: Arc<dyn LLMPort>| -> Box<dyn ReviewGrader> {
        if assessment {
            Box::new(FakeGrader::new(blind_assessment()))
        } else {
            Box::new(FakeGrader::failing("unused"))
        }
    })
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
                progress_total: 6,
                message: "Waiting".into(),
            })
            .await?;
        let job = store.get(&admitted.id).await?;
        assert!(matches!(spec_of(&job), Err(AppError::InvalidInput(_))));
    }
    let queued = queue_build(&pool, &spec(), "named").await?;
    assert_eq!(spec_of(&store.get(&queued).await?)?, spec());
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
async fn a_queued_build_runs_on_the_runtime_and_stores_the_drill() -> Result<()> {
    let pool = pool().await?;
    let model = Arc::new(ScriptedModel::new([
        clean_code_answer(),
        review_answer(vec![]),
        plant_answer(),
        judge_answer(true),
        judge_answer(true),
        blind_answer(),
        entry_answer(),
    ]));
    let id = queue_build(&pool, &spec(), "end to end").await?;
    run_to_the_end(&pool, worker(&pool, model.clone(), judging(true)), &id).await?;

    let job = JobStore::new(pool.clone()).get(&id).await?;
    assert_eq!(job.status, JobStatus::Completed, "{:?}", job.error_message);
    assert_eq!(
        model.purposes(),
        [
            "clean code",
            "clean review",
            "plant",
            "judge",
            "judge",
            "blind review",
            "key entry"
        ],
        "without a runner both defects are judged"
    );
    let mut connection = pool.acquire().await.map_err(db)?;
    let stored = repository::oldest_ready_in(&mut connection, "python")
        .await?
        .expect("the drill is in the bank");
    assert_eq!(job.result_ref, Some(format!("drill:{}", stored.drill.id)));
    assert_eq!(stored.suite, None);
    assert_eq!(stored.runner, None);
    assert_eq!(stored.drill.key.len(), 3);
    assert!(stored
        .drill
        .key
        .iter()
        .all(|issue| matches!(issue.proof, Proof::Judged { .. })));
    Ok(())
}

#[tokio::test]
async fn a_model_outage_defers_the_build_and_stores_nothing() -> Result<()> {
    let pool = pool().await?;
    let id = queue_build(&pool, &spec(), "outage").await?;
    let outage = DrillBuildWorker {
        pool: pool.clone(),
        load_llm: Arc::new(|| {
            Box::pin(async { Err(AppError::Network("The model is down.".into())) })
        }),
        runtimes: BuiltinRuntimes::new(None),
        grader: judging(true),
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
async fn until_the_judge_is_connected_a_build_stops_at_its_blind_review_with_its_work_saved(
) -> Result<()> {
    let pool = pool().await?;
    let model = Arc::new(ScriptedModel::new([
        clean_code_answer(),
        review_answer(vec![]),
        plant_answer(),
        judge_answer(true),
        judge_answer(true),
        blind_answer(),
    ]));
    let id = queue_build(&pool, &spec(), "unconnected").await?;
    let grader: GraderFactory = Arc::new(unconnected_grader);
    run_to_the_end(&pool, worker(&pool, model, grader), &id).await?;
    let store = JobStore::new(pool.clone());
    let job = store.get(&id).await?;
    assert_eq!(job.status, JobStatus::Failed);
    assert!(
        job.error_message
            .as_deref()
            .is_some_and(|message| message.contains("not connected")),
        "{:?}",
        job.error_message
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
    let id = queue_build(&pool, &spec(), "cancelled").await?;
    let runtime = JobRuntime::new(pool.clone());
    runtime
        .register(
            DRILL_BUILD,
            Arc::new(worker(&pool, model.clone(), judging(true))),
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

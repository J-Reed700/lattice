//! The `learning.drill_grading` job: an attempt graded and its grade stored whole, a grader that gives none, a stopped
//! job, a grade asked for again, and the job on the job runtime with the judge that grades every review.

use super::support::{
    blind_assessment, extra_finding, files, fix_answer, grade_answer, last, operator_finding,
    operator_fixed, proved, review_assessment, FakeGrader, FakeRunner, ScriptedModel, Waiting,
    LITERALS, MODEL, OPERATOR,
};
use crate::features::learning::drills::attempts::{self, AttemptStatus};
use crate::features::learning::drills::grader::{Review, ReviewGrader};
use crate::features::learning::drills::jobs::{
    attempt_of, grading_job_config, judge_grader, DrillGradingWorker, DRILL_GRADING, GRADING_STEPS,
};
use crate::features::learning::embedded_runtime::BuiltinRuntimes;
use crate::features::learning::generation_jobs::pinned_for_test;
use async_trait::async_trait;
use lattice_core::application::ports::LLMPort;
use lattice_core::domain::learning::drills::{
    ratings, score, Assessment, DecidedBy, Finding, FindingOutcome, FixOutcome, IssueOutcome,
    IssueRating, PatternRating,
};
use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::runtime::jobs::{
    JobContext, JobHandler, JobOutcome, JobRecord, JobRuntime, JobStatus, JobStore, NewJob,
    RecoveryPolicy,
};
use serde_json::json;
use sqlx::SqlitePool;
use std::sync::Arc;
use std::time::Duration;

fn none() -> ScriptedModel {
    ScriptedModel::new(Vec::<String>::new())
}

fn findings() -> Vec<Finding> {
    vec![operator_finding(), extra_finding()]
}

#[test]
fn a_grading_job_is_one_per_attempt_and_resumes_after_a_restart() {
    let config = grading_job_config();
    assert_eq!(config.recovery, RecoveryPolicy::Requeue);
    assert!(config.exclusive_per_subject);
    assert_eq!(DRILL_GRADING, "learning.drill_grading");
}

#[tokio::test]
async fn a_job_names_the_attempt_it_grades() -> Result<()> {
    let waiting = Waiting::unclaimed(findings(), None).await?;
    let job = waiting.store.get(&waiting.job).await?;
    assert_eq!(attempt_of(&job)?, waiting.attempt);
    let nameless = waiting
        .store
        .submit(&NewJob {
            kind: DRILL_GRADING.into(),
            subject_id: Some("x".into()),
            operation_id: uuid::Uuid::new_v4().to_string(),
            payload_hash: "h".into(),
            requested: json!({}),
            progress_total: GRADING_STEPS,
            message: "Waiting".into(),
        })
        .await?;
    assert!(matches!(
        attempt_of(&waiting.store.get(&nameless.id).await?),
        Err(AppError::InvalidInput(_))
    ));
    Ok(())
}

#[tokio::test]
async fn a_submitted_review_is_graded_and_the_grade_is_stored_whole() -> Result<()> {
    let waiting = Waiting::new(findings(), None).await?;
    let model = none();
    let grader = FakeGrader::new(review_assessment());
    let outcome = waiting.grade(&model, None, &grader).await?;
    assert_eq!(outcome, JobOutcome::Committed);

    let attempt = waiting.attempt().await?;
    assert_eq!(attempt.status, AttemptStatus::Graded);
    let key = proved().planted_key();
    assert_eq!(attempt.assessment, Some(review_assessment()));
    assert_eq!(
        attempt.score,
        Some(score(&key, &findings(), &review_assessment()))
    );
    assert_eq!(
        attempt.ratings,
        Some(vec![
            IssueRating {
                pattern: OPERATOR.into(),
                rating: PatternRating::Easy
            },
            IssueRating {
                pattern: LITERALS.into(),
                rating: PatternRating::Again
            },
        ])
    );
    assert_eq!(
        attempt.ratings,
        Some(ratings(&key, &review_assessment())),
        "one rating for each key issue's pattern"
    );
    assert_eq!(attempt.fix, None);
    assert_eq!(attempt.grader_model.as_deref(), Some(MODEL));
    assert!(attempt.graded_at.is_some());
    assert_eq!(attempt.error, None);
    assert_eq!(
        attempt.findings,
        Some(findings()),
        "the learner's own words"
    );

    let job = waiting.store.get(&waiting.job).await?;
    assert_eq!(job.status, JobStatus::Completed);
    assert_eq!(job.result_ref, Some(format!("attempt:{}", waiting.attempt)));
    assert!(model.requests().is_empty(), "no fix, no other model call");
    Ok(())
}

#[tokio::test]
async fn the_grader_is_shown_the_drill_the_clean_code_and_the_learners_findings() -> Result<()> {
    let waiting = Waiting::new(findings(), None).await?;
    let grader = FakeGrader::new(review_assessment());
    waiting.grade(&none(), None, &grader).await?;
    let seen = grader.seen();
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].issues, ["planted-1", "planted-2"]);
    assert_eq!(seen[0].files, proved().files);
    assert_eq!(seen[0].clean, files());
    assert_eq!(seen[0].findings, findings());
    Ok(())
}

#[tokio::test]
async fn a_review_with_a_fix_is_graded_with_the_fix_run_or_judged_for_each_issue() -> Result<()> {
    let waiting = Waiting::new(findings(), Some(operator_fixed())).await?;
    let model = ScriptedModel::new([fix_answer("not", "The literals are as they were.")]);
    let runner = FakeRunner::killing_the_operator();
    let grader = FakeGrader::new(review_assessment());
    let outcome = waiting.grade(&model, Some(&runner), &grader).await?;
    assert_eq!(outcome, JobOutcome::Committed);

    let attempt = waiting.attempt().await?;
    assert_eq!(attempt.status, AttemptStatus::Graded);
    let fix = attempt.fix.expect("the fix is graded");
    assert_eq!(fix.len(), 2);
    assert_eq!(
        (fix[0].outcome, fix[0].decided_by),
        (FixOutcome::Fixed, DecidedBy::Test)
    );
    assert_eq!(
        (fix[1].outcome, fix[1].decided_by),
        (FixOutcome::NotFixed, DecidedBy::Judge)
    );
    assert_eq!(model.purposes(), ["fix judge"]);
    assert_eq!(runner.runs().len(), 2, "the suite and the killing test");
    Ok(())
}

#[tokio::test]
async fn the_job_reports_its_steps() -> Result<()> {
    let waiting = Waiting::new(findings(), Some(operator_fixed())).await?;
    let model = ScriptedModel::new([fix_answer("not", "As they were.")]);
    let runner = FakeRunner::killing_the_operator();
    let grader = FakeGrader::new(review_assessment());
    waiting.grade(&model, Some(&runner), &grader).await?;
    let job = waiting.store.get(&waiting.job).await?;
    assert_eq!(job.progress_total, GRADING_STEPS);
    assert_eq!(
        job.progress_current, GRADING_STEPS,
        "a completed job is at its end"
    );
    Ok(())
}

#[tokio::test]
async fn a_grader_that_gives_no_assessment_leaves_an_attempt_that_could_not_be_graded() -> Result<()>
{
    let waiting = Waiting::new(findings(), None).await?;
    let grader = FakeGrader::failing("The model is down.");
    let outcome = waiting.grade(&none(), None, &grader).await?;
    assert_eq!(outcome, JobOutcome::Committed, "the job did what it could");

    let attempt = waiting.attempt().await?;
    assert_eq!(attempt.status, AttemptStatus::CouldntGrade);
    assert!(
        attempt
            .error
            .as_deref()
            .is_some_and(|error| error.contains("The model is down.")),
        "the error is kept: {:?}",
        attempt.error
    );
    assert_eq!(attempt.grader_model.as_deref(), Some(MODEL));
    assert_eq!(
        (
            attempt.assessment,
            attempt.score,
            attempt.ratings,
            attempt.fix
        ),
        (None, None, None, None),
        "it is never a score"
    );
    assert_eq!(attempt.graded_at, None);
    assert_eq!(
        attempt.findings,
        Some(findings()),
        "the review is kept to grade again"
    );
    let job = waiting.store.get(&waiting.job).await?;
    assert_eq!(job.status, JobStatus::Completed);
    Ok(())
}

#[tokio::test]
async fn an_assessment_that_contradicts_itself_is_not_a_grade() -> Result<()> {
    let waiting = Waiting::new(findings(), None).await?;
    let contradiction = Assessment {
        issues: vec![
            (IssueOutcome::Found { finding: 0 }, "Named.".into()),
            (IssueOutcome::Missed, "Not named.".into()),
        ],
        findings: vec![
            (FindingOutcome::FalsePositive, "Not a problem.".into()),
            (FindingOutcome::ValidExtra, "A real one.".into()),
        ],
    };
    let grader = FakeGrader::new(contradiction);
    waiting.grade(&none(), None, &grader).await?;
    let attempt = waiting.attempt().await?;
    assert_eq!(attempt.status, AttemptStatus::CouldntGrade);
    assert!(
        attempt
            .error
            .as_deref()
            .is_some_and(|error| error.contains("contradicts itself")),
        "{:?}",
        attempt.error
    );
    assert_eq!((attempt.score, attempt.ratings), (None, None));
    Ok(())
}

#[tokio::test]
async fn a_fix_that_cannot_be_judged_leaves_the_whole_attempt_ungraded() -> Result<()> {
    let waiting = Waiting::new(findings(), Some(operator_fixed())).await?;
    let model = ScriptedModel::new([fix_answer("fixed", ""), fix_answer("fixed", " ")]);
    let runner = FakeRunner::killing_the_operator();
    let grader = FakeGrader::new(review_assessment());
    let outcome = waiting.grade(&model, Some(&runner), &grader).await?;
    assert_eq!(outcome, JobOutcome::Committed);
    let attempt = waiting.attempt().await?;
    assert_eq!(attempt.status, AttemptStatus::CouldntGrade);
    assert_eq!(
        (attempt.assessment, attempt.score, attempt.fix),
        (None, None, None),
        "a review is not stored graded beside a fix that was not"
    );
    Ok(())
}

#[tokio::test]
async fn an_attempt_that_could_not_be_graded_is_graded_by_a_regrade() -> Result<()> {
    let waiting = Waiting::new(findings(), None).await?;
    let failing = FakeGrader::failing("The model is down.");
    waiting.grade(&none(), None, &failing).await?;
    assert_eq!(waiting.attempt().await?.status, AttemptStatus::CouldntGrade);

    let queued = attempts::regrade(&waiting.pool, &waiting.attempt).await?;
    let second = queued.job.expect("a new job");
    assert_ne!(second, waiting.job);
    Waiting::claim(&waiting.store, &second).await?;
    let context = JobContext::detached(&waiting.store, &second).await?;
    let (attempt, stored) = attempts::waiting(&waiting.pool, &waiting.attempt)
        .await?
        .expect("the attempt waits again");
    let model = none();
    let grader = FakeGrader::new(review_assessment());
    let grading = waiting.grading(&model, None, &grader);
    let outcome = grading.run(&context, &attempt, &stored).await?;
    assert_eq!(outcome, JobOutcome::Committed);

    let graded = waiting.attempt().await?;
    assert_eq!(graded.status, AttemptStatus::Graded);
    assert_eq!(graded.regrades, 1);
    assert_eq!(graded.error, None, "the error of the last try is gone");
    assert_eq!(graded.assessment, Some(review_assessment()));
    assert_eq!(graded.grader_model.as_deref(), Some(MODEL));
    assert_eq!(
        waiting.store.get(&second).await?.status,
        JobStatus::Completed
    );
    Ok(())
}

fn worker(pool: &SqlitePool, model: Arc<dyn LLMPort>) -> DrillGradingWorker {
    DrillGradingWorker {
        pool: pool.clone(),
        load_llm: Arc::new(move || {
            let model = model.clone();
            Box::pin(async move { Ok(pinned_for_test(model)) })
        }),
        runtimes: BuiltinRuntimes::new(None),
        grader: judge_grader,
    }
}

/// Runs the job through a runtime of its own, as the app would after a queue.
async fn run_to_the_end(pool: &SqlitePool, worker: DrillGradingWorker, job: &str) -> Result<()> {
    let runtime = JobRuntime::new(pool.clone());
    runtime
        .register(DRILL_GRADING, Arc::new(worker), grading_job_config())
        .await?;
    runtime.submitted(job).await;
    runtime.finished(job).await;
    runtime.close();
    runtime.drain().await;
    Ok(())
}

/// The findings of the blind review that `grade_answer` is the judge's answer to.
fn judged_findings() -> Vec<Finding> {
    vec![
        operator_finding(),
        Finding {
            span: None,
            text: "The dictionary is written twice.".into(),
            severity: None,
        },
        Finding {
            span: None,
            text: "Nothing calls forget, so the dictionary grows without bound.".into(),
            severity: None,
        },
        Finding {
            span: None,
            text: "Something is wrong here.".into(),
            severity: None,
        },
    ]
}

#[tokio::test]
async fn a_queued_job_runs_on_the_runtime_and_the_judge_grades_the_review() -> Result<()> {
    let waiting = Waiting::unclaimed(judged_findings(), None).await?;
    let model = Arc::new(ScriptedModel::new([grade_answer()]));
    run_to_the_end(
        &waiting.pool,
        worker(&waiting.pool, model.clone()),
        &waiting.job,
    )
    .await?;

    let job = waiting.store.get(&waiting.job).await?;
    assert_eq!(job.status, JobStatus::Completed, "{:?}", job.error_message);
    let attempt = waiting.attempt().await?;
    assert_eq!(attempt.status, AttemptStatus::Graded);
    assert_eq!(attempt.assessment, Some(blind_assessment()));
    assert_eq!(attempt.grader_model.as_deref(), Some(MODEL));
    assert_eq!(model.purposes(), ["grade"], "one call grades the review");
    let requests = model.requests();
    let task = last(&requests[0]);
    assert!(
        task.contains("Nothing calls forget, so the dictionary grows without bound."),
        "the judge reads the learner's own words"
    );
    Ok(())
}

#[tokio::test]
async fn a_model_that_cannot_be_reached_leaves_the_attempt_ungraded_with_the_reason() -> Result<()>
{
    let waiting = Waiting::unclaimed(findings(), None).await?;
    let unreachable = DrillGradingWorker {
        pool: waiting.pool.clone(),
        load_llm: Arc::new(|| {
            Box::pin(async { Err(AppError::InvalidConfig("No Learning model is set.".into())) })
        }),
        runtimes: BuiltinRuntimes::new(None),
        grader: judge_grader,
    };
    run_to_the_end(&waiting.pool, unreachable, &waiting.job).await?;
    let attempt = waiting.attempt().await?;
    assert_eq!(attempt.status, AttemptStatus::CouldntGrade);
    assert!(
        attempt
            .error
            .as_deref()
            .is_some_and(|error| error.contains("No Learning model is set.")),
        "{:?}",
        attempt.error
    );
    assert_eq!(attempt.grader_model, None, "no model was reached");
    assert_eq!(
        waiting.store.get(&waiting.job).await?.status,
        JobStatus::Completed
    );
    Ok(())
}

#[tokio::test]
async fn a_job_for_an_attempt_that_is_not_waiting_changes_nothing() -> Result<()> {
    let waiting = Waiting::new(findings(), None).await?;
    let grader = FakeGrader::new(review_assessment());
    waiting.grade(&none(), None, &grader).await?;
    let graded = waiting.attempt().await?;

    // A second job for the attempt, queued after it was graded.
    let again = waiting
        .store
        .submit(&NewJob {
            kind: DRILL_GRADING.into(),
            subject_id: Some(waiting.attempt.clone()),
            operation_id: uuid::Uuid::new_v4().to_string(),
            payload_hash: "h".into(),
            requested: json!({"attemptId": waiting.attempt}),
            progress_total: GRADING_STEPS,
            message: "Waiting".into(),
        })
        .await?;
    let model = Arc::new(none());
    run_to_the_end(
        &waiting.pool,
        worker(&waiting.pool, model.clone()),
        &again.id,
    )
    .await?;
    assert_eq!(
        waiting.store.get(&again.id).await?.status,
        JobStatus::Completed
    );
    assert!(model.requests().is_empty(), "nothing was graded again");
    assert_eq!(waiting.attempt().await?, graded);

    // A job for an attempt that is not there.
    let missing = waiting
        .store
        .submit(&NewJob {
            kind: DRILL_GRADING.into(),
            subject_id: Some("nothing".into()),
            operation_id: uuid::Uuid::new_v4().to_string(),
            payload_hash: "h".into(),
            requested: json!({"attemptId": "nothing"}),
            progress_total: GRADING_STEPS,
            message: "Waiting".into(),
        })
        .await?;
    run_to_the_end(
        &waiting.pool,
        worker(&waiting.pool, model.clone()),
        &missing.id,
    )
    .await?;
    assert_eq!(
        waiting.store.get(&missing.id).await?.status,
        JobStatus::Completed
    );
    Ok(())
}

/// A grader that never answers.
struct Hanging;

#[async_trait]
impl ReviewGrader for Hanging {
    async fn assess(&self, _review: &Review<'_>) -> Result<Assessment> {
        std::future::pending().await
    }
}

fn hanging(_llm: &dyn LLMPort) -> Box<dyn ReviewGrader + '_> {
    Box::new(Hanging)
}

/// Waits until the job has begun grading the review.
async fn wait_until_grading(store: &JobStore, job: &str) -> Result<()> {
    for _ in 0..1_000 {
        if store
            .get(job)
            .await?
            .progress_message
            .starts_with("Grading your review")
        {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    Err(AppError::InternalError(
        "The job never began grading.".into(),
    ))
}

/// Panics before grading; the runtime's word that it failed the job goes to the real worker.
struct Panics(DrillGradingWorker);

#[async_trait]
impl JobHandler for Panics {
    async fn run(&self, _: &JobContext) -> Result<JobOutcome> {
        panic!("the grading exploded");
    }

    async fn failed_by_runtime(&self, job: &JobRecord, code: &str) {
        self.0.failed_by_runtime(job, code).await;
    }
}

/// Waits until the review stands as "couldn't grade", as its learner would read it.
async fn wait_until_ungraded(waiting: &Waiting) -> Result<()> {
    for _ in 0..1_000 {
        if waiting.attempt().await?.status == AttemptStatus::CouldntGrade {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    Err(AppError::InternalError(
        "The review was left waiting for a grade.".into(),
    ))
}

#[tokio::test]
async fn a_panic_while_grading_leaves_the_review_marked_not_graded() -> Result<()> {
    let waiting = Waiting::unclaimed(findings(), None).await?;
    let runtime = JobRuntime::new(waiting.pool.clone());
    runtime
        .register(
            DRILL_GRADING,
            Arc::new(Panics(worker(&waiting.pool, Arc::new(none())))),
            grading_job_config(),
        )
        .await?;
    runtime.submitted(&waiting.job).await;
    runtime.finished(&waiting.job).await;

    assert_eq!(
        waiting.store.get(&waiting.job).await?.status,
        JobStatus::Failed
    );
    wait_until_ungraded(&waiting).await?;
    assert!(waiting.attempt().await?.error.is_some());
    runtime.close();
    runtime.drain().await;
    Ok(())
}

#[tokio::test]
async fn a_grading_job_the_runtime_failed_for_stopping_the_app_leaves_the_review_marked_not_graded(
) -> Result<()> {
    let waiting = Waiting::unclaimed(findings(), None).await?;
    Waiting::claim(&waiting.store, &waiting.job).await?;
    waiting
        .store
        .recover(DRILL_GRADING, RecoveryPolicy::Requeue)
        .await?;
    Waiting::claim(&waiting.store, &waiting.job).await?;

    let runtime = JobRuntime::new(waiting.pool.clone());
    runtime
        .register(
            DRILL_GRADING,
            Arc::new(worker(&waiting.pool, Arc::new(none()))),
            grading_job_config(),
        )
        .await?;

    let job = waiting.store.get(&waiting.job).await?;
    assert_eq!(job.error_code.as_deref(), Some("stopped_the_app"));
    wait_until_ungraded(&waiting).await?;
    assert!(waiting.attempt().await?.error.is_some());
    runtime.close();
    runtime.drain().await;
    Ok(())
}

fn stuck_worker(pool: &SqlitePool) -> DrillGradingWorker {
    DrillGradingWorker {
        grader: hanging,
        ..worker(pool, Arc::new(none()))
    }
}

#[tokio::test]
async fn cancelling_the_job_stops_the_grading_and_leaves_the_attempt_submitted() -> Result<()> {
    let waiting = Waiting::unclaimed(findings(), None).await?;
    let runtime = JobRuntime::new(waiting.pool.clone());
    runtime
        .register(
            DRILL_GRADING,
            Arc::new(stuck_worker(&waiting.pool)),
            grading_job_config(),
        )
        .await?;
    runtime.submitted(&waiting.job).await;
    wait_until_grading(&waiting.store, &waiting.job).await?;
    runtime.cancel(&waiting.job).await?;
    runtime.finished(&waiting.job).await;
    runtime.close();
    runtime.drain().await;

    assert_eq!(
        waiting.store.get(&waiting.job).await?.status,
        JobStatus::Cancelled
    );
    let attempt = waiting.attempt().await?;
    assert_eq!(attempt.status, AttemptStatus::Submitted, "never failed");
    assert_eq!(
        (attempt.assessment, attempt.score, attempt.error),
        (None, None, None)
    );
    Ok(())
}

#[tokio::test]
async fn shutting_down_stops_the_grading_and_the_job_resumes_at_the_next_start() -> Result<()> {
    let waiting = Waiting::unclaimed(judged_findings(), None).await?;
    let runtime = JobRuntime::new(waiting.pool.clone());
    runtime
        .register(
            DRILL_GRADING,
            Arc::new(stuck_worker(&waiting.pool)),
            grading_job_config(),
        )
        .await?;
    runtime.submitted(&waiting.job).await;
    wait_until_grading(&waiting.store, &waiting.job).await?;
    runtime.close();
    runtime.drain().await;

    assert_eq!(
        waiting.store.get(&waiting.job).await?.status,
        JobStatus::Pending,
        "a stopped job is put back, not failed"
    );
    assert_eq!(waiting.attempt().await?.status, AttemptStatus::Submitted);

    let model = Arc::new(ScriptedModel::new([grade_answer()]));
    run_to_the_end(&waiting.pool, worker(&waiting.pool, model), &waiting.job).await?;
    assert_eq!(
        waiting.store.get(&waiting.job).await?.status,
        JobStatus::Completed
    );
    assert_eq!(waiting.attempt().await?.status, AttemptStatus::Graded);
    Ok(())
}

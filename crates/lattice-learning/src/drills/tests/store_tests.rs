//! Stage 6: the key is checked against the code, then one transaction writes the drill and completes the job.

use super::support::{
    baseline, code, files, literals_mutation, planted, proved, quiet, review, spec, FakeGrader,
    FakeRunner, Harness, ScriptedModel, LITERALS, MODEL, OPERATOR, SUITE,
};
use crate::drills::build::store::Built;
use crate::drills::build::{DroppedMutation, Proved, Trail};
use crate::drills::repository::{self, DrillStatus};
use crate::persistence::db;
use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::runtime::jobs::{JobOutcome, JobStatus};

async fn count(harness: &Harness, table: &str) -> Result<i64> {
    sqlx::query_scalar(&format!("SELECT count(*) FROM {table}"))
        .fetch_one(&harness.pool)
        .await
        .map_err(db)
}

#[tokio::test]
async fn a_finished_drill_is_stored_with_its_patterns_and_the_job_completes_with_it() -> Result<()>
{
    let harness = Harness::new(&spec()).await?;
    let (model, grader, runner) = (
        ScriptedModel::new([]),
        FakeGrader::failing("unused"),
        FakeRunner::passing(),
    );
    let (code, review, planted, proved, baseline, blind) =
        (code(), review(), planted(), proved(), baseline(), quiet());
    let context = harness.context().await?;
    let outcome = harness
        .build(&model, Some(&runner), &grader)
        .store(
            &context,
            &spec(),
            Built {
                baseline: &baseline,
                code: &code,
                review: &review,
                planted: &planted,
                proved: &proved,
                blind: &blind,
            },
        )
        .await?;
    assert_eq!(outcome, JobOutcome::Committed);

    let job = harness.store.get(&harness.job).await?;
    assert_eq!(job.status, JobStatus::Completed);
    assert_eq!(job.result_ref, Some(format!("drill:{}", code.drill_id)));

    let mut connection = harness.pool.acquire().await.map_err(db)?;
    let stored = repository::oldest_ready_in(&mut connection, "python")
        .await?
        .expect("the drill is in the bank");
    assert_eq!(stored.drill.id, code.drill_id);
    assert_eq!(stored.drill.title, code.title);
    assert_eq!(stored.drill.files, proved.files);
    assert_eq!(stored.drill.key, proved.planted_key());
    assert_eq!(stored.clean_files, files());
    assert_eq!(stored.spec, spec());
    assert_eq!(stored.domain, "rate limiting");
    assert_eq!(stored.suite.as_deref(), Some(SUITE));
    assert_eq!(stored.runner.as_deref(), Some("fake-python"));
    assert_eq!(stored.model, MODEL);
    assert_eq!(stored.status, DrillStatus::Ready);
    assert_eq!(stored.taken_at, None);
    assert_eq!(stored.evidence.proofs.len(), 2);
    assert_eq!(stored.evidence.proofs[0].issue, "planted-1");
    drop(connection);

    let patterns: Vec<String> = sqlx::query_scalar(
        "SELECT pattern_id FROM learning_drill_patterns WHERE drill_id=? ORDER BY pattern_id",
    )
    .bind(&code.drill_id)
    .fetch_all(&harness.pool)
    .await
    .map_err(db)?;
    assert_eq!(patterns, [OPERATOR, LITERALS]);
    Ok(())
}

#[tokio::test]
async fn without_a_runner_no_suite_or_runner_is_stored() -> Result<()> {
    let harness = Harness::new(&spec()).await?;
    let (model, grader) = (ScriptedModel::new([]), FakeGrader::failing("unused"));
    let (code, review, planted, proved, mut baseline, blind) =
        (code(), review(), planted(), proved(), baseline(), quiet());
    baseline.suite = None;
    let context = harness.context().await?;
    harness
        .build(&model, None, &grader)
        .store(
            &context,
            &spec(),
            Built {
                baseline: &baseline,
                code: &code,
                review: &review,
                planted: &planted,
                proved: &proved,
                blind: &blind,
            },
        )
        .await?;
    let mut connection = harness.pool.acquire().await.map_err(db)?;
    let stored = repository::oldest_ready_in(&mut connection, "python")
        .await?
        .expect("the drill is in the bank");
    assert_eq!(stored.suite, None);
    assert_eq!(stored.runner, None);
    Ok(())
}

#[tokio::test]
async fn a_key_that_ends_empty_stores_nothing_and_fails_the_job_with_the_reason() -> Result<()> {
    let harness = Harness::new(&spec()).await?;
    let (model, grader, runner) = (
        ScriptedModel::new([]),
        FakeGrader::failing("unused"),
        FakeRunner::passing(),
    );
    let (code, review, planted, baseline, blind) =
        (code(), review(), planted(), baseline(), quiet());
    let nothing = Proved {
        proven: Vec::new(),
        dropped: vec![DroppedMutation {
            mutation: literals_mutation(),
            why: "The original code already has this problem.".into(),
            trail: Trail::default(),
        }],
        files: files(),
        spans: Vec::new(),
    };
    let context = harness.context().await?;
    let error = harness
        .build(&model, Some(&runner), &grader)
        .store(
            &context,
            &spec(),
            Built {
                baseline: &baseline,
                code: &code,
                review: &review,
                planted: &planted,
                proved: &nothing,
                blind: &blind,
            },
        )
        .await
        .unwrap_err();
    let AppError::InvalidData(reason) = &error else {
        panic!("a key with nothing in it is invalid data: {error:?}");
    };
    assert!(
        reason.contains("No planted defect could be proven"),
        "{reason}"
    );
    assert!(reason.contains(LITERALS), "{reason}");
    assert!(
        reason.contains("The original code already has this problem."),
        "{reason}"
    );
    assert_eq!(count(&harness, "learning_drills").await?, 0);
    assert_eq!(count(&harness, "learning_drill_patterns").await?, 0);
    assert_eq!(
        harness.store.get(&harness.job).await?.status,
        JobStatus::Running,
        "the runtime then fails the job with the reason"
    );
    Ok(())
}

#[tokio::test]
async fn a_key_that_does_not_agree_with_its_code_is_refused_before_anything_is_stored() -> Result<()>
{
    let harness = Harness::new(&spec()).await?;
    let (model, grader, runner) = (
        ScriptedModel::new([]),
        FakeGrader::failing("unused"),
        FakeRunner::passing(),
    );
    let (code, review, planted, baseline, blind) =
        (code(), review(), planted(), baseline(), quiet());
    let mut shifted = proved();
    shifted.spans[0].start += 1;
    shifted.spans[0].end += 1;
    let context = harness.context().await?;
    let error = harness
        .build(&model, Some(&runner), &grader)
        .store(
            &context,
            &spec(),
            Built {
                baseline: &baseline,
                code: &code,
                review: &review,
                planted: &planted,
                proved: &shifted,
                blind: &blind,
            },
        )
        .await
        .unwrap_err();
    assert!(
        matches!(&error, AppError::InvalidData(reason) if reason.contains("does not agree with its code") && reason.contains("SpanMismatch")),
        "{error:?}"
    );
    assert_eq!(count(&harness, "learning_drills").await?, 0);
    Ok(())
}

#[tokio::test]
async fn a_job_cancelled_meanwhile_stores_nothing() -> Result<()> {
    let harness = Harness::new(&spec()).await?;
    let (model, grader, runner) = (
        ScriptedModel::new([]),
        FakeGrader::failing("unused"),
        FakeRunner::passing(),
    );
    let (code, review, planted, proved, baseline, blind) =
        (code(), review(), planted(), proved(), baseline(), quiet());
    let context = harness.context().await?;
    harness.store.cancel(&harness.job).await?;
    let outcome = harness
        .build(&model, Some(&runner), &grader)
        .store(
            &context,
            &spec(),
            Built {
                baseline: &baseline,
                code: &code,
                review: &review,
                planted: &planted,
                proved: &proved,
                blind: &blind,
            },
        )
        .await?;
    assert_eq!(outcome, JobOutcome::Stopped);
    assert_eq!(count(&harness, "learning_drills").await?, 0);
    assert_eq!(
        harness.store.get(&harness.job).await?.status,
        JobStatus::Cancelled
    );
    Ok(())
}

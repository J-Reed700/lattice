//! Stage 7: the key is checked against the code, then one transaction writes the drill and completes the job.

use super::support::{
    baseline, code, files, lesson_brief, lesson_proved, literals_mutation, planted, proved, quiet,
    review, unchecked, FakeGrader, FakeRunner, Harness, ScriptedModel, LITERALS, MODEL, OPERATOR,
    SUITE,
};
use crate::drills::build::store::Built;
use crate::drills::build::{CleanCode, DroppedMutation, LessonCheck, LessonChecked, Proved, Trail};
use crate::drills::repository::{self, DrillStatus, StoredDrill};
use crate::drills::schedules;
use crate::drills::spec::DrillSpec;
use lattice_core::domain::learning::drills::Material;
use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::runtime::jobs::{JobOutcome, JobStatus};

async fn count(harness: &Harness, table: &str) -> Result<i64> {
    sqlx::query_scalar(sqlx::AssertSqlSafe(format!("SELECT count(*) FROM {table}")))
        .fetch_one(&harness.pool)
        .await
        .map_err(AppError::from)
}

#[tokio::test]
async fn a_finished_drill_is_stored_with_its_patterns_and_the_job_completes_with_it() -> Result<()>
{
    let (harness, spec) = Harness::lesson().await?;
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
            &spec,
            Built {
                baseline: &baseline,
                code: &code,
                review: &review,
                planted: &planted,
                proved: &proved,
                checked: &unchecked(),
                blind: &blind,
            },
        )
        .await?;
    assert_eq!(outcome, JobOutcome::Committed);

    let job = harness.store.get(&harness.job).await?;
    assert_eq!(job.status, JobStatus::Completed);
    assert_eq!(job.result_ref, Some(format!("drill:{}", code.drill_id)));

    let mut connection = harness.pool.acquire().await?;
    let stored = repository::find_in(&mut connection, &code.drill_id)
        .await?
        .expect("the drill is in the bank");
    assert_eq!(stored.drill.id, code.drill_id);
    assert_eq!(stored.drill.title, code.title);
    assert_eq!(stored.drill.files, proved.files);
    assert_eq!(stored.drill.key, proved.planted_key(&unchecked()));
    assert_eq!(stored.clean_files, files());
    assert_eq!(stored.spec, spec);
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
    .await?;
    assert_eq!(patterns, [OPERATOR, LITERALS]);
    Ok(())
}

#[tokio::test]
async fn without_a_runner_no_suite_or_runner_is_stored() -> Result<()> {
    let (harness, spec) = Harness::lesson().await?;
    let (model, grader) = (ScriptedModel::new([]), FakeGrader::failing("unused"));
    let (code, review, planted, proved, mut baseline, blind) =
        (code(), review(), planted(), proved(), baseline(), quiet());
    baseline.suite = None;
    let context = harness.context().await?;
    harness
        .build(&model, None, &grader)
        .store(
            &context,
            &spec,
            Built {
                baseline: &baseline,
                code: &code,
                review: &review,
                planted: &planted,
                proved: &proved,
                checked: &unchecked(),
                blind: &blind,
            },
        )
        .await?;
    let mut connection = harness.pool.acquire().await?;
    let stored = repository::find_in(&mut connection, &code.drill_id)
        .await?
        .expect("the drill is in the bank");
    assert_eq!(stored.suite, None);
    assert_eq!(stored.runner, None);
    Ok(())
}

#[tokio::test]
async fn a_key_that_ends_empty_stores_nothing_and_fails_the_job_with_the_reason() -> Result<()> {
    let (harness, spec) = Harness::lesson().await?;
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
            &spec,
            Built {
                baseline: &baseline,
                code: &code,
                review: &review,
                planted: &planted,
                proved: &nothing,
                checked: &unchecked(),
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
    let (harness, spec) = Harness::lesson().await?;
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
            &spec,
            Built {
                baseline: &baseline,
                code: &code,
                review: &review,
                planted: &planted,
                proved: &shifted,
                checked: &unchecked(),
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
    let (harness, spec) = Harness::lesson().await?;
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
            &spec,
            Built {
                baseline: &baseline,
                code: &code,
                review: &review,
                planted: &planted,
                proved: &proved,
                checked: &unchecked(),
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

// What the stored drill says of its lesson.

/// A check of the issue `issue`, which cites `sentences`, that `breaks` the sentences.
fn check_of(issue: &str, sentences: &[usize], breaks: bool) -> LessonCheck {
    LessonCheck {
        issue: issue.into(),
        pattern: OPERATOR.into(),
        sentences: sentences.to_vec(),
        breaks,
        follows: true,
        reason: "The edit lets one call more through.".into(),
        trail: Trail::default(),
    }
}

/// What the lesson was recorded as fitting `material` (`code`, `passage`), with the basis it was recorded on.
async fn fits_of(harness: &Harness, material: &str) -> Result<Vec<(i64, String)>> {
    sqlx::query_as("SELECT fits,basis FROM learning_drill_lesson_fits WHERE material=?")
        .bind(material)
        .fetch_all(&harness.pool)
        .await
        .map_err(AppError::from)
}

/// Whether the lesson the stored drill was built from still fits code.
async fn lesson_fits_code(harness: &Harness, stored: &StoredDrill) -> Result<bool> {
    let (Some(program), Some(lesson)) = (stored.program_id.as_deref(), stored.lesson_id.as_deref())
    else {
        return Err(AppError::InternalError("The drill has no lesson.".into()));
    };
    let mut connection = harness.pool.acquire().await?;
    schedules::lesson_fits_in(&mut connection, program, lesson, &Material::code("python")).await
}

/// Stores the drill of `proved` with `checked` and the clean stage `code` on a new course's lesson, and returns the
/// harness it was stored in.
async fn stored_with(
    code: CleanCode,
    proved: Proved,
    checked: LessonChecked,
) -> Result<(Harness, StoredDrill)> {
    stored_on(Harness::lesson().await?, code, proved, checked).await
}

/// [`stored_with`] on the harness of a build of `spec`.
async fn stored_on(
    (harness, spec): (Harness, DrillSpec),
    code: CleanCode,
    proved: Proved,
    checked: LessonChecked,
) -> Result<(Harness, StoredDrill)> {
    let (model, grader, runner) = (
        ScriptedModel::new([]),
        FakeGrader::failing("unused"),
        FakeRunner::passing(),
    );
    let (review, planted, baseline, blind) = (review(), planted(), baseline(), quiet());
    let context = harness.context().await?;
    let outcome = harness
        .build(&model, Some(&runner), &grader)
        .store(
            &context,
            &spec,
            Built {
                baseline: &baseline,
                code: &code,
                review: &review,
                planted: &planted,
                proved: &proved,
                checked: &checked,
                blind: &blind,
            },
        )
        .await?;
    assert_eq!(outcome, JobOutcome::Committed);
    let mut connection = harness.pool.acquire().await?;
    let stored = repository::find_in(&mut connection, &code.drill_id)
        .await?
        .expect("the drill is in the bank");
    drop(connection);
    Ok((harness, stored))
}

fn with_basis() -> CleanCode {
    CleanCode {
        basis: Some("basis-1".into()),
        ..code()
    }
}

#[tokio::test]
async fn a_stored_drill_keeps_the_lesson_link_in_its_key_and_the_checks_in_its_evidence_and_records_no_fit_for_code(
) -> Result<()> {
    let link = lesson_brief().link(&[1]);
    let checked = LessonChecked {
        links: vec![link.clone(), None],
        checks: vec![check_of("planted-1", &[1], true)],
    };
    let (harness, stored) =
        stored_with(with_basis(), lesson_proved(&[1], &[]), checked.clone()).await?;
    assert_eq!(stored.drill.key[0].lesson, link);
    assert_eq!(stored.drill.key[1].lesson, None);
    assert_eq!(stored.evidence.lesson_checks, checked.checks);
    assert!(fits_of(&harness, "code").await?.is_empty());
    Ok(())
}

#[tokio::test]
async fn a_code_drill_with_no_linked_issue_records_no_fit_and_the_lesson_still_fits_code(
) -> Result<()> {
    let failed = LessonChecked {
        links: vec![None, None],
        checks: vec![check_of("planted-1", &[1], false)],
    };
    let (harness, stored) =
        stored_with(with_basis(), lesson_proved(&[1], &[]), failed.clone()).await?;
    assert_eq!(stored.drill.key[0].lesson, None);
    assert_eq!(stored.evidence.lesson_checks, failed.checks);
    assert!(fits_of(&harness, "code").await?.is_empty());
    assert!(lesson_fits_code(&harness, &stored).await?);

    let (harness, stored) = stored_with(with_basis(), proved(), unchecked()).await?;
    assert!(stored.evidence.lesson_checks.is_empty());
    assert!(fits_of(&harness, "code").await?.is_empty());
    assert!(lesson_fits_code(&harness, &stored).await?);
    Ok(())
}

#[tokio::test]
async fn a_code_drill_records_no_fit_whether_or_not_its_clean_stage_has_a_basis() -> Result<()> {
    let checked = LessonChecked {
        links: vec![lesson_brief().link(&[1]), None],
        checks: vec![check_of("planted-1", &[1], true)],
    };
    let (harness, _) = stored_with(with_basis(), lesson_proved(&[1], &[]), checked.clone()).await?;
    assert!(
        fits_of(&harness, "code").await?.is_empty(),
        "with a basis it is not recorded either"
    );
    let (harness, stored) = stored_with(code(), lesson_proved(&[1], &[]), checked).await?;
    assert!(stored.drill.key[0].lesson.is_some());
    assert!(fits_of(&harness, "code").await?.is_empty());
    Ok(())
}

#[tokio::test]
async fn a_drill_whose_lesson_is_gone_by_the_store_is_stored_without_it_and_records_no_fit(
) -> Result<()> {
    let (harness, spec) = Harness::lesson().await?;
    let gone = sqlx::query("DELETE FROM learning_lessons WHERE program_id=? AND id=?")
        .bind(spec.program_id.as_deref())
        .bind(spec.lesson_id.as_deref())
        .execute(&harness.pool)
        .await?;
    assert_eq!(gone.rows_affected(), 1);
    let checked = LessonChecked {
        links: vec![lesson_brief().link(&[1]), None],
        checks: vec![check_of("planted-1", &[1], true)],
    };
    let program_id = spec.program_id.clone();
    let (harness, stored) = stored_on(
        (harness, spec),
        with_basis(),
        lesson_proved(&[1], &[]),
        checked,
    )
    .await?;
    let course: (Option<String>, Option<String>) =
        sqlx::query_as("SELECT program_id,lesson_id FROM learning_drills WHERE id=?")
            .bind(&stored.drill.id)
            .fetch_one(&harness.pool)
            .await?;
    assert_eq!(course, (program_id, None));
    assert!(fits_of(&harness, "code").await?.is_empty());
    Ok(())
}

//! A learner's attempt: the table and what it refuses, the lifecycle from taking a drill to asking for its grade
//! again, what a submission is refused for, and that asking twice finds the same job.

use super::support::{
    code_spec, course, extra_finding, opened, operator_finding, operator_fixed, proved,
    review_assessment, stock, stock_on, unchecked, MODEL,
};
use crate::drills::attempts::{self, Attempt, AttemptMode, AttemptStatus, Graded, Submission};
use crate::drills::bank::Shelf;
use crate::drills::jobs::{DRILL_BUILD, DRILL_GRADING, GRADING_STEPS};
use crate::drills::repository::DrillStatus;
use crate::persistence::db;
use crate::tests::pool;
use lattice_core::domain::learning::drills::{
    ratings, score, Assessment, DrillFile, Finding, IssueFix, IssueRating, LineSpan, Material,
    PatternRating, Score, Severity,
};
use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::persistence::Database;
use lattice_core::shared::runtime::jobs::{JobStatus, JobStore};
use serde_json::json;
use sqlx::SqlitePool;

async fn scalar(pool: &SqlitePool, sql: &str) -> Result<i64> {
    sqlx::query_scalar(sql).fetch_one(pool).await.map_err(db)
}

fn on(path: &str, start: u32, end: u32) -> Option<LineSpan> {
    Some(LineSpan {
        path: path.into(),
        start,
        end,
    })
}

fn submission(findings: Vec<Finding>) -> Submission {
    Submission {
        findings,
        fix_files: None,
        time_spent_ms: 90_000,
    }
}

/// A pool with one drill in the bank and an attempt opened on it.
async fn open() -> Result<(SqlitePool, Database, Attempt)> {
    let pool = pool().await?;
    let database = Database::new(pool.clone());
    stock(&database, "drill").await?;
    let attempt = opened(&database, "drill", AttemptMode::Practice).await?;
    Ok((pool, database, attempt))
}

/// An open attempt that has been handed in with one finding.
async fn submitted() -> Result<(SqlitePool, Database, Attempt, String)> {
    let (pool, database, attempt) = open().await?;
    let queued =
        attempts::submit(&database, &attempt.id, submission(vec![operator_finding()])).await?;
    let job = queued.job.expect("a grading job");
    Ok((pool, database, queued.attempt, job))
}

async fn jobs_of(pool: &SqlitePool, kind: &str) -> Result<i64> {
    sqlx::query_scalar("SELECT count(*) FROM jobs WHERE kind=?")
        .bind(kind)
        .fetch_one(pool)
        .await
        .map_err(db)
}

#[tokio::test]
async fn the_table_and_its_index_exist() -> Result<()> {
    let pool = pool().await?;
    let names: Vec<String> = sqlx::query_scalar(
        "SELECT name FROM sqlite_master WHERE name LIKE 'learning_drill_attempts%' ORDER BY name",
    )
    .fetch_all(&pool)
    .await
    .map_err(db)?;
    assert_eq!(
        names,
        ["learning_drill_attempts", "learning_drill_attempts_drill"]
    );
    Ok(())
}

#[tokio::test]
async fn a_row_must_agree_with_its_status() -> Result<()> {
    let pool = pool().await?;
    let database = Database::new(pool.clone());
    stock(&database, "drill").await?;
    let head = "INSERT INTO learning_drill_attempts(id,drill_id,mode,started_at,status";
    let cases: [(&str, String, bool); 20] = [
        ("an open attempt", format!("{head}) VALUES('a','drill','practice',1,'open')"), true),
        ("an open attempt that was submitted", format!("{head},submitted_at) VALUES('a','drill','practice',1,'open',2)"), false),
        ("a submitted attempt", format!("{head},submitted_at,time_spent_ms,findings_json) VALUES('a','drill','practice',1,'submitted',2,3,'[]')"), true),
        ("a submitted attempt with no findings", format!("{head},submitted_at,time_spent_ms) VALUES('a','drill','practice',1,'submitted',2,3)"), false),
        ("a submitted attempt with no time", format!("{head},submitted_at,findings_json) VALUES('a','drill','practice',1,'submitted',2,'[]')"), false),
        ("a submitted attempt with negative time", format!("{head},submitted_at,time_spent_ms,findings_json) VALUES('a','drill','practice',1,'submitted',2,-3,'[]')"), false),
        ("findings that are not JSON", format!("{head},submitted_at,time_spent_ms,findings_json) VALUES('a','drill','practice',1,'submitted',2,3,'not json')"), false),
        ("an unknown mode", format!("{head}) VALUES('a','drill','blitz',1,'open')"), false),
        ("an unknown status", format!("{head}) VALUES('a','drill','practice',1,'late')"), false),
        ("an unknown drill", format!("{head}) VALUES('a','nothing','practice',1,'open')"), false),
        ("a model on a submitted attempt", format!("{head},submitted_at,time_spent_ms,findings_json,grader_model) VALUES('a','drill','practice',1,'submitted',2,3,'[]','m')"), false),
        ("an error on a submitted attempt", format!("{head},submitted_at,time_spent_ms,findings_json,grading_error) VALUES('a','drill','practice',1,'submitted',2,3,'[]','e')"), false),
        ("an attempt that could not be graded", format!("{head},submitted_at,time_spent_ms,findings_json,grading_error) VALUES('a','drill','practice',1,'couldnt_grade',2,3,'[]','e')"), true),
        ("an attempt that could not be graded with a model", format!("{head},submitted_at,time_spent_ms,findings_json,grading_error,grader_model) VALUES('a','drill','practice',1,'couldnt_grade',2,3,'[]','e','m')"), true),
        ("an attempt that could not be graded and says nothing", format!("{head},submitted_at,time_spent_ms,findings_json) VALUES('a','drill','practice',1,'couldnt_grade',2,3,'[]')"), false),
        ("an attempt that could not be graded with a score", format!("{head},submitted_at,time_spent_ms,findings_json,grading_error,score_json) VALUES('a','drill','practice',1,'couldnt_grade',2,3,'[]','e','{{}}')"), false),
        ("a graded attempt", format!("{head},submitted_at,time_spent_ms,findings_json,assessment_json,score_json,ratings_json,grader_model,graded_at) VALUES('a','drill','practice',1,'graded',2,3,'[]','{{}}','{{}}','[]','m',4)"), true),
        ("a graded attempt with no model", format!("{head},submitted_at,time_spent_ms,findings_json,assessment_json,score_json,ratings_json,graded_at) VALUES('a','drill','practice',1,'graded',2,3,'[]','{{}}','{{}}','[]',4)"), false),
        ("a graded attempt with no score", format!("{head},submitted_at,time_spent_ms,findings_json,assessment_json,ratings_json,grader_model,graded_at) VALUES('a','drill','practice',1,'graded',2,3,'[]','{{}}','[]','m',4)"), false),
        ("a graded attempt with fix results and no fix", format!("{head},submitted_at,time_spent_ms,findings_json,assessment_json,score_json,ratings_json,fix_json,grader_model,graded_at) VALUES('a','drill','practice',1,'graded',2,3,'[]','{{}}','{{}}','[]','[]','m',4)"), false),
    ];
    for (name, sql, accepted) in cases {
        sqlx::query("DELETE FROM learning_drill_attempts")
            .execute(&pool)
            .await
            .map_err(db)?;
        let result = sqlx::query(&sql).execute(&pool).await;
        assert_eq!(result.is_ok(), accepted, "{name}: {result:?}");
    }
    Ok(())
}

#[tokio::test]
async fn deleting_a_drill_deletes_its_attempts() -> Result<()> {
    let (pool, database, _) = open().await?;
    stock(&database, "other").await?;
    opened(&database, "other", AttemptMode::Interview).await?;
    assert_eq!(
        scalar(&pool, "SELECT count(*) FROM learning_drill_attempts").await?,
        2
    );
    sqlx::query("DELETE FROM learning_drills WHERE id='drill'")
        .execute(&pool)
        .await
        .map_err(db)?;
    assert_eq!(
        scalar(&pool, "SELECT count(*) FROM learning_drill_attempts").await?,
        1
    );
    Ok(())
}

#[tokio::test]
async fn starting_takes_the_drill_opens_an_attempt_and_queues_the_next_build() -> Result<()> {
    let pool = pool().await?;
    let database = Database::new(pool.clone());
    let spec = code_spec(&pool, &database).await?;
    let program = spec.course().expect("a course").to_owned();
    stock_on(&database, "drill", &spec).await?;
    let begun = attempts::start(&database, Shelf::of(&spec)?, None, AttemptMode::Interview)
        .await?
        .expect("a ready drill");
    let attempt = &begun.attempt;
    assert_eq!(attempt.drill_id, "drill");
    assert_eq!(attempt.mode, AttemptMode::Interview);
    assert_eq!(attempt.status, AttemptStatus::Open);
    assert!(attempt.started_at > 0);
    assert_eq!(
        (
            attempt.submitted_at,
            attempt.time_spent_ms,
            &attempt.findings,
            &attempt.fix_files,
            &attempt.assessment,
            &attempt.grader_model,
            &attempt.error,
            attempt.regrades
        ),
        (None, None, &None, &None, &None, &None, &None, 0)
    );
    assert_eq!(begun.drill.drill.id, "drill");
    assert_eq!(begun.drill.status, DrillStatus::Taken);
    let queued = begun.next_build.as_deref().expect("the next build");
    let next = JobStore::new(database.clone()).get(queued).await?;
    assert_eq!(next.kind, DRILL_BUILD);
    assert_eq!(
        next.subject_id,
        Some(format!("drill:{program}:code:python"))
    );
    assert_eq!(attempts::get(&database, &attempt.id).await?.0, *attempt);
    Ok(())
}

#[tokio::test]
async fn with_no_drill_ready_nothing_is_opened() -> Result<()> {
    let pool = pool().await?;
    let database = Database::new(pool.clone());
    let spec = code_spec(&pool, &database).await?;
    let program = spec.course().expect("a course").to_owned();
    stock_on(&database, "drill", &spec).await?;
    let javascript = Material::code("javascript");
    let other = course(&database).await?;
    for shelf in [
        Shelf {
            program_id: &program,
            material: &javascript,
        },
        Shelf {
            program_id: &other,
            material: &spec.material,
        },
    ] {
        assert!(
            attempts::start(&database, shelf, None, AttemptMode::Practice)
                .await?
                .is_none()
        );
    }
    assert_eq!(
        scalar(&pool, "SELECT count(*) FROM learning_drill_attempts").await?,
        0
    );
    assert_eq!(jobs_of(&pool, DRILL_BUILD).await?, 0);
    Ok(())
}

#[tokio::test]
async fn a_drill_is_given_to_one_attempt() -> Result<()> {
    let pool = pool().await?;
    let database = Database::new(pool.clone());
    let spec = code_spec(&pool, &database).await?;
    stock_on(&database, "drill", &spec).await?;
    let shelf = Shelf::of(&spec)?;
    assert!(
        attempts::start(&database, shelf, None, AttemptMode::Practice)
            .await?
            .is_some()
    );
    assert!(
        attempts::start(&database, shelf, None, AttemptMode::Practice)
            .await?
            .is_none()
    );
    Ok(())
}

#[tokio::test]
async fn submitting_stores_the_review_and_queues_one_grading_job() -> Result<()> {
    let (pool, database, attempt) = open().await?;
    let findings = vec![operator_finding(), extra_finding()];
    let queued = attempts::submit(
        &database,
        &attempt.id,
        Submission {
            findings: findings.clone(),
            fix_files: Some(operator_fixed()),
            time_spent_ms: 61_500,
        },
    )
    .await?;
    let stored = &queued.attempt;
    assert_eq!(stored.status, AttemptStatus::Submitted);
    assert_eq!(stored.findings, Some(findings));
    assert_eq!(stored.fix_files, Some(operator_fixed()));
    assert_eq!(stored.time_spent_ms, Some(61_500));
    assert!(stored.submitted_at.is_some());
    assert_eq!(attempts::get(&database, &attempt.id).await?.0, *stored);

    let job = JobStore::new(database.clone())
        .get(&queued.job.expect("a job"))
        .await?;
    assert_eq!(job.kind, DRILL_GRADING);
    assert_eq!(job.subject_id.as_deref(), Some(attempt.id.as_str()));
    assert_eq!(job.status, JobStatus::Pending);
    assert_eq!(job.progress_total, GRADING_STEPS);
    assert_eq!(
        job.requested,
        json!({"attemptId": attempt.id, "regrades": 0})
    );
    assert_eq!(
        job.operation_id,
        uuid::Uuid::new_v5(
            &uuid::Uuid::NAMESPACE_OID,
            format!("learning.drill_grading:{}:0", attempt.id).as_bytes()
        )
        .to_string(),
        "the operation id derives from the attempt"
    );
    assert_eq!(jobs_of(&pool, DRILL_GRADING).await?, 1);
    Ok(())
}

#[tokio::test]
async fn a_finding_is_kept_as_the_learner_wrote_it() -> Result<()> {
    let (_, database, attempt) = open().await?;
    let finding = Finding {
        span: on("src/config.py", 8, 8),
        text: "  Magic numbers.\n\nUse the constants.  ".into(),
        severity: Some(Severity::Nit),
    };
    let queued =
        attempts::submit(&database, &attempt.id, submission(vec![finding.clone()])).await?;
    assert_eq!(queued.attempt.findings, Some(vec![finding]));
    Ok(())
}

#[tokio::test]
async fn a_finding_with_no_text_or_on_no_line_of_the_drill_is_refused_and_the_attempt_stays_open(
) -> Result<()> {
    let (pool, database, attempt) = open().await?;
    let finding = |span: Option<LineSpan>, text: &str| Finding {
        span,
        text: text.into(),
        severity: None,
    };
    let refused = [
        ("blank", finding(on("src/limiter.py", 13, 13), "   \n ")),
        ("empty", finding(None, "")),
        ("another file", finding(on("src/other.py", 1, 1), "Odd.")),
        ("line zero", finding(on("src/limiter.py", 0, 3), "Odd.")),
        ("backwards", finding(on("src/limiter.py", 9, 5), "Odd.")),
        (
            "past the end",
            finding(on("src/limiter.py", 22, 22), "Odd."),
        ),
    ];
    for (name, bad) in refused {
        let result = attempts::submit(
            &database,
            &attempt.id,
            submission(vec![finding(None, "Fine."), bad]),
        )
        .await;
        assert!(
            matches!(result, Err(AppError::InvalidInput(_))),
            "{name}: {result:?}"
        );
    }
    let kept = attempts::get(&database, &attempt.id).await?.0;
    assert_eq!(kept.status, AttemptStatus::Open);
    assert_eq!(kept.findings, None);
    assert_eq!(jobs_of(&pool, DRILL_GRADING).await?, 0);
    Ok(())
}

#[tokio::test]
async fn the_message_for_a_refused_finding_names_it() -> Result<()> {
    let (_, database, attempt) = open().await?;
    let bad = Finding {
        span: on("src/limiter.py", 30, 31),
        text: "Odd.".into(),
        severity: None,
    };
    let Err(AppError::InvalidInput(message)) = attempts::submit(
        &database,
        &attempt.id,
        submission(vec![operator_finding(), bad]),
    )
    .await
    else {
        panic!("the finding is refused");
    };
    assert!(message.contains("Finding 2"), "{message}");
    assert!(
        message.contains("30 to 31") && message.contains("src/limiter.py"),
        "{message}"
    );
    Ok(())
}

#[tokio::test]
async fn nothing_else_about_a_review_is_refused() -> Result<()> {
    let (_, database, attempt) = open().await?;
    let mut findings = vec![
        // The last line, and a span that runs past the end of the file but starts in it.
        Finding {
            span: on("src/limiter.py", 21, 21),
            text: "Last line.".into(),
            severity: None,
        },
        Finding {
            span: on("src/limiter.py", 21, 400),
            text: "Runs on.".into(),
            severity: Some(Severity::Critical),
        },
        // The same lines twice, and no lines at all.
        operator_finding(),
        operator_finding(),
        extra_finding(),
    ];
    findings.extend((0..500).map(|n| Finding {
        span: None,
        text: format!("Remark {n}."),
        severity: None,
    }));
    let queued = attempts::submit(&database, &attempt.id, submission(findings)).await?;
    assert_eq!(
        queued.attempt.findings.map(|stored| stored.len()),
        Some(505)
    );
    Ok(())
}

#[tokio::test]
async fn a_review_with_no_findings_can_be_handed_in() -> Result<()> {
    let (_, database, attempt) = open().await?;
    let queued = attempts::submit(&database, &attempt.id, submission(Vec::new())).await?;
    assert_eq!(queued.attempt.findings, Some(Vec::<Finding>::new()));
    assert!(queued.job.is_some());
    Ok(())
}

#[tokio::test]
async fn a_negative_time_is_refused() -> Result<()> {
    let (_, database, attempt) = open().await?;
    let result = attempts::submit(
        &database,
        &attempt.id,
        Submission {
            findings: Vec::new(),
            fix_files: None,
            time_spent_ms: -1,
        },
    )
    .await;
    assert!(
        matches!(result, Err(AppError::InvalidInput(_))),
        "{result:?}"
    );
    Ok(())
}

#[tokio::test]
async fn files_left_as_they_were_are_no_fix() -> Result<()> {
    let (_, database, attempt) = open().await?;
    let drill_files = attempts::get(&database, &attempt.id).await?.1.drill.files;
    let queued = attempts::submit(
        &database,
        &attempt.id,
        Submission {
            findings: Vec::new(),
            fix_files: Some(drill_files),
            time_spent_ms: 1,
        },
    )
    .await?;
    assert_eq!(queued.attempt.fix_files, None);
    Ok(())
}

#[tokio::test]
async fn the_line_endings_of_a_fix_are_the_ones_of_a_drill() -> Result<()> {
    let (_, database, attempt) = open().await?;
    let drill_files = attempts::get(&database, &attempt.id).await?.1.drill.files;
    let windows: Vec<_> = drill_files
        .iter()
        .map(|file| DrillFile {
            path: file.path.clone(),
            text: file.text.replace('\n', "\r\n"),
        })
        .collect();
    let unchanged = attempts::submit(
        &database,
        &attempt.id,
        Submission {
            findings: Vec::new(),
            fix_files: Some(windows.clone()),
            time_spent_ms: 1,
        },
    )
    .await?;
    assert_eq!(
        unchanged.attempt.fix_files, None,
        "the same code with other line endings is no fix"
    );

    let (_, database, attempt) = open().await?;
    let fixed: Vec<_> = operator_fixed()
        .iter()
        .map(|file| DrillFile {
            path: file.path.clone(),
            text: file.text.replace('\n', "\r\n"),
        })
        .collect();
    let queued = attempts::submit(
        &database,
        &attempt.id,
        Submission {
            findings: Vec::new(),
            fix_files: Some(fixed),
            time_spent_ms: 1,
        },
    )
    .await?;
    assert_eq!(queued.attempt.fix_files, Some(operator_fixed()));
    Ok(())
}

#[tokio::test]
async fn a_fix_that_sends_some_files_keeps_the_drills_for_the_rest() -> Result<()> {
    let (_, database, attempt) = open().await?;
    let limiter = operator_fixed()
        .into_iter()
        .find(|file| file.path == "src/limiter.py")
        .expect("the limiter");
    let queued = attempts::submit(
        &database,
        &attempt.id,
        Submission {
            findings: Vec::new(),
            fix_files: Some(vec![limiter]),
            time_spent_ms: 1,
        },
    )
    .await?;
    assert_eq!(queued.attempt.fix_files, Some(operator_fixed()));
    Ok(())
}

#[tokio::test]
async fn a_file_the_drill_does_not_have_comes_after_the_drills_files() -> Result<()> {
    let (_, database, attempt) = open().await?;
    let helper = DrillFile {
        path: "src/helper.py".into(),
        text: "def helper():\n    return 1\n".into(),
    };
    let mut sent = operator_fixed();
    sent.push(helper.clone());
    let queued = attempts::submit(
        &database,
        &attempt.id,
        Submission {
            findings: Vec::new(),
            fix_files: Some(sent.clone()),
            time_spent_ms: 1,
        },
    )
    .await?;
    assert_eq!(queued.attempt.fix_files, Some(sent));
    let kept = queued.attempt.fix_files.expect("a fix");
    assert_eq!(kept.last(), Some(&helper));
    Ok(())
}

#[tokio::test]
async fn handing_in_the_same_review_again_finds_the_same_job() -> Result<()> {
    let (pool, database, attempt, job) = submitted().await?;
    let again =
        attempts::submit(&database, &attempt.id, submission(vec![operator_finding()])).await?;
    assert_eq!(again.job, Some(job));
    assert_eq!(again.attempt, attempt);
    assert_eq!(jobs_of(&pool, DRILL_GRADING).await?, 1);
    Ok(())
}

#[tokio::test]
async fn handing_in_another_review_is_refused() -> Result<()> {
    let (_, database, attempt, _) = submitted().await?;
    for other in [
        submission(vec![extra_finding()]),
        Submission {
            fix_files: Some(operator_fixed()),
            ..submission(vec![operator_finding()])
        },
    ] {
        let result = attempts::submit(&database, &attempt.id, other).await;
        assert!(
            matches!(result, Err(AppError::InvalidState(_))),
            "{result:?}"
        );
    }
    assert_eq!(
        attempts::get(&database, &attempt.id).await?.0.findings,
        Some(vec![operator_finding()])
    );
    Ok(())
}

#[tokio::test]
async fn an_attempt_that_is_not_there_is_not_found() -> Result<()> {
    let database = Database::new(pool().await?);
    assert!(matches!(
        attempts::submit(&database, "nothing", submission(Vec::new())).await,
        Err(AppError::NotFound(_))
    ));
    assert!(matches!(
        attempts::regrade(&database, "nothing").await,
        Err(AppError::NotFound(_))
    ));
    assert!(matches!(
        attempts::get(&database, "nothing").await,
        Err(AppError::NotFound(_))
    ));
    assert!(attempts::waiting(&database, "nothing").await?.is_none());
    Ok(())
}

#[tokio::test]
async fn only_a_submitted_attempt_is_waiting_for_its_grade() -> Result<()> {
    let (_, database, attempt) = open().await?;
    assert!(attempts::waiting(&database, &attempt.id).await?.is_none());
    attempts::submit(&database, &attempt.id, submission(vec![operator_finding()])).await?;
    let (waiting, drill) = attempts::waiting(&database, &attempt.id)
        .await?
        .expect("a submitted attempt waits");
    assert_eq!(waiting.status, AttemptStatus::Submitted);
    assert_eq!(drill.drill.id, "drill");
    Ok(())
}

/// What a grade writes for the key of the stocked drill and the two findings of [`review_assessment`].
fn grade_of_the_review() -> (Assessment, Score, Vec<IssueRating>) {
    let key = proved().planted_key(&unchecked());
    let assessment = review_assessment();
    let scored = score(&key, &[operator_finding(), extra_finding()], &assessment);
    let rated = ratings(&key, &assessment);
    (assessment, scored, rated)
}

#[tokio::test]
async fn a_grade_is_written_with_the_model_that_gave_it() -> Result<()> {
    let (_, database, attempt, _) = submitted().await?;
    let (assessment, scored, rated) = grade_of_the_review();
    let mut tx = database.begin_write().await?;
    let recorded = attempts::repository::record_graded_in(
        &mut tx,
        &attempt.id,
        &Graded {
            assessment: &assessment,
            score: &scored,
            ratings: &rated,
            fix: None,
            model: MODEL,
        },
    )
    .await?;
    assert!(recorded);
    tx.commit().await?;
    let graded = attempts::get(&database, &attempt.id).await?.0;
    assert_eq!(graded.status, AttemptStatus::Graded);
    assert_eq!(graded.assessment, Some(assessment.clone()));
    assert_eq!(graded.score, Some(scored.clone()));
    assert_eq!(graded.ratings, Some(rated.clone()));
    assert_eq!(graded.fix, None);
    assert_eq!(graded.grader_model.as_deref(), Some(MODEL));
    assert!(graded.graded_at.is_some());
    assert_eq!(graded.error, None);
    assert_eq!(
        rated.iter().map(|rated| rated.rating).collect::<Vec<_>>(),
        [PatternRating::Easy, PatternRating::Again]
    );

    let mut tx = database.begin_write().await?;
    let again = attempts::repository::record_graded_in(
        &mut tx,
        &attempt.id,
        &Graded {
            assessment: &assessment,
            score: &scored,
            ratings: &rated,
            fix: None,
            model: "another",
        },
    )
    .await?;
    assert!(!again, "a graded attempt is not graded twice");
    let kept =
        attempts::repository::record_ungraded_in(&mut tx, &attempt.id, Some("m"), "No.").await?;
    assert!(!kept, "a graded attempt does not become ungraded");
    tx.commit().await?;
    Ok(())
}

#[tokio::test]
async fn a_fix_is_written_beside_its_grade_when_the_attempt_has_one() -> Result<()> {
    let (_, database, attempt) = open().await?;
    attempts::submit(
        &database,
        &attempt.id,
        Submission {
            fix_files: Some(operator_fixed()),
            ..submission(vec![operator_finding()])
        },
    )
    .await?;
    let (assessment, scored, rated) = grade_of_the_review();
    let fix: Vec<IssueFix> = Vec::new();
    let mut tx = database.begin_write().await?;
    attempts::repository::record_graded_in(
        &mut tx,
        &attempt.id,
        &Graded {
            assessment: &assessment,
            score: &scored,
            ratings: &rated,
            fix: Some(fix.as_slice()),
            model: MODEL,
        },
    )
    .await?;
    tx.commit().await?;
    assert_eq!(
        attempts::get(&database, &attempt.id).await?.0.fix,
        Some(Vec::<IssueFix>::new())
    );
    Ok(())
}

#[tokio::test]
async fn an_attempt_that_could_not_be_graded_keeps_why_and_regrades() -> Result<()> {
    let (pool, database, attempt, first) = submitted().await?;
    let mut tx = database.begin_write().await?;
    assert!(
        attempts::repository::record_ungraded_in(&mut tx, &attempt.id, Some(MODEL), "No answer.")
            .await?
    );
    tx.commit().await?;
    let failed = attempts::get(&database, &attempt.id).await?.0;
    assert_eq!(failed.status, AttemptStatus::CouldntGrade);
    assert_eq!(failed.error.as_deref(), Some("No answer."));
    assert_eq!(failed.grader_model.as_deref(), Some(MODEL));
    assert_eq!(
        (failed.score, failed.assessment, failed.ratings),
        (None, None, None)
    );

    let queued = attempts::regrade(&database, &attempt.id).await?;
    let second = queued.job.clone().expect("a new job");
    assert_ne!(second, first, "a regrade is a new job");
    let regraded = &queued.attempt;
    assert_eq!(regraded.status, AttemptStatus::Submitted);
    assert_eq!(regraded.regrades, 1);
    assert_eq!((&regraded.error, &regraded.grader_model), (&None, &None));
    assert_eq!(regraded.findings, Some(vec![operator_finding()]));
    let job = JobStore::new(database.clone()).get(&second).await?;
    assert_eq!(job.kind, DRILL_GRADING);
    assert_eq!(
        job.requested,
        json!({"attemptId": attempt.id, "regrades": 1})
    );
    assert_eq!(job.operation_id, attempts::operation_id(&attempt.id, 1));
    assert_ne!(
        job.operation_id,
        attempts::operation_id(&attempt.id, 0),
        "each regrade has an operation id of its own"
    );

    let again = attempts::regrade(&database, &attempt.id).await?;
    assert_eq!(again.job, Some(second), "asking again finds the job");
    assert_eq!(again.attempt.regrades, 1);
    assert_eq!(jobs_of(&pool, DRILL_GRADING).await?, 2);
    Ok(())
}

#[tokio::test]
async fn an_open_or_graded_attempt_is_not_regraded() -> Result<()> {
    let (pool, database, attempt) = open().await?;
    assert!(matches!(
        attempts::regrade(&database, &attempt.id).await,
        Err(AppError::InvalidState(_))
    ));
    attempts::submit(&database, &attempt.id, submission(vec![operator_finding()])).await?;
    let (assessment, scored, rated) = grade_of_the_review();
    let mut tx = database.begin_write().await?;
    attempts::repository::record_graded_in(
        &mut tx,
        &attempt.id,
        &Graded {
            assessment: &assessment,
            score: &scored,
            ratings: &rated,
            fix: None,
            model: MODEL,
        },
    )
    .await?;
    tx.commit().await?;
    assert!(matches!(
        attempts::regrade(&database, &attempt.id).await,
        Err(AppError::InvalidState(_))
    ));
    assert_eq!(jobs_of(&pool, DRILL_GRADING).await?, 1);
    Ok(())
}

#[test]
fn the_findings_of_these_tests_are_on_the_lines_the_stocked_drill_has() {
    assert_eq!(
        operator_finding().span,
        Some(proved().planted_key(&unchecked())[0].span.clone())
    );
}

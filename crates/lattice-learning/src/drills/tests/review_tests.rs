//! What the review view's commands do: starting a drill and the learner's view of it, which holds nothing of the key,
//! handing a review in, and the reveal of a graded review, which holds everything the design lists.

use super::support::{
    code_spec, course, extra_finding, files, fix_answer, lessons_of, opened, operator_finding,
    operator_fixed, proved, record_unfit, review_assessment, schedule_pattern, set_state, spec,
    stock, stock_on, unchecked, FakeGrader, FakeRunner, ScriptedModel, Waiting, KILLING_TEST,
    LITERALS, MODEL, OPERATOR, SLEEP_LESSON, SPACING_LESSON, SUITE,
};
use crate::drills::attempts::{self, AttemptMode, AttemptStatus};
use crate::drills::dto::{
    CodeReviewFileDto, CodeReviewFindingDto, CodeReviewMaterialDto, CodeReviewSeverity,
    CodeReviewSpanDto, DrillLessonRefDto, FindingResultKind, IssueResultKind,
    RevealedLessonLinkDto, RevealedLessonSentenceDto, StartedCodeReviewDto,
};
use crate::drills::jobs::{spec_of, DRILL_BUILD, DRILL_GRADING};
use crate::drills::reveal;
use crate::drills::review::{self, CourseDrill};
use crate::drills::spec::DrillSpec;
use crate::persistence::db;
use crate::tests::pool;
use lattice_core::domain::learning::drills::{
    line_diff, pattern, targets, DecidedBy, DiffKind, FixOutcome, IssueRating, LessonLink,
    LessonSentence, Material, Origin, PatternRating, Proof, LANGUAGES,
};
use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::runtime::jobs::JobStore;
use serde_json::{json, Value};
use sqlx::SqlitePool;

/// The text as it reads inside a JSON string.
fn escaped(text: &str) -> String {
    let quoted = serde_json::to_string(text).expect("text is JSON");
    quoted
        .strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
        .expect("a JSON string is quoted")
        .to_owned()
}

/// Every text of the stocked drill that a learner must not read before the grade, and some not at all.
fn secrets() -> Vec<(String, String)> {
    let mut secrets = Vec::new();
    for issue in proved().planted_key(&unchecked()) {
        let which = |what: &str| format!("{} {what}", issue.id);
        secrets.push((which("id"), issue.id.clone()));
        secrets.push((which("pattern"), issue.pattern.clone()));
        secrets.push((which("consequence"), issue.consequence.clone()));
        secrets.push((which("explanation"), issue.explanation.clone()));
        secrets.push((which("fix"), issue.fix.after.trim().to_owned()));
        match &issue.proof {
            Proof::Killed { test } => secrets.push((which("killing test"), test.clone())),
            Proof::Judged { reason } => secrets.push((which("proof"), reason.clone())),
            Proof::Sourced { quote, reason, .. } => {
                secrets.push((which("source quote"), quote.clone()));
                secrets.push((which("proof"), reason.clone()));
            }
        }
    }
    secrets.push(("the suite".into(), SUITE.into()));
    secrets.push(("the killing test".into(), KILLING_TEST.into()));
    secrets.push(("the model that built the drill".into(), MODEL.into()));
    secrets
}

const GRADER: &str = "scripted-grading-model";

/// A request for a drill on code in `language` from any lesson of the course `program_id`.
fn code(program_id: &str, language: &str) -> CourseDrill {
    CourseDrill {
        program_id: program_id.to_owned(),
        lesson_id: None,
        material: Material::code(language),
    }
}

/// A request for a drill on code in python from the lesson `lesson_id` of the course `program_id`.
fn lesson_of(program_id: &str, lesson_id: &str) -> CourseDrill {
    CourseDrill {
        lesson_id: Some(lesson_id.to_owned()),
        ..code(program_id, "python")
    }
}

/// A new course with the drill "drill" stocked, built from its first lesson; the course's id.
async fn stocked(pool: &SqlitePool) -> Result<String> {
    let spec = code_spec(pool).await?;
    stock_on(pool, "drill", &spec).await?;
    spec.program_id
        .ok_or_else(|| AppError::InternalError("The spec has no course.".into()))
}

async fn status_of(pool: &SqlitePool, id: &str) -> Result<String> {
    sqlx::query_scalar("SELECT status FROM learning_drills WHERE id=?")
        .bind(id)
        .fetch_one(pool)
        .await
        .map_err(db)
}

/// What a DTO shows of the key: the texts it holds and the names of the fields it has.
fn leaks(shown: &Value) -> Vec<String> {
    let json = serde_json::to_string(shown).expect("a DTO is JSON");
    let mut leaked = Vec::new();
    for (what, text) in secrets() {
        if json.contains(&escaped(&text)) {
            leaked.push(format!("it shows {what}: {text}"));
        }
    }
    for name in [
        "key",
        "suite",
        "clean",
        "cleanFiles",
        "evidence",
        "proof",
        "killingTest",
        "consequence",
        "explanation",
        "fix",
    ] {
        if json.contains(&format!("\"{name}\"")) {
            leaked.push(format!("it has a field named {name}"));
        }
    }
    leaked
}

fn assert_nothing_of_the_key(shown: &Value) {
    let leaked = leaks(shown);
    assert!(leaked.is_empty(), "{leaked:?} in {shown}");
}

async fn jobs_of(pool: &SqlitePool, kind: &str) -> Result<i64> {
    sqlx::query_scalar("SELECT count(*) FROM jobs WHERE kind=?")
        .bind(kind)
        .fetch_one(pool)
        .await
        .map_err(db)
}

fn finding_dtos() -> Vec<CodeReviewFindingDto> {
    vec![
        CodeReviewFindingDto {
            span: Some(CodeReviewSpanDto {
                path: "src/limiter.py".into(),
                start: 13,
                end: 13,
            }),
            text: "This lets one call more than the limit through.".into(),
            severity: Some(CodeReviewSeverity::Major),
        },
        CodeReviewFindingDto {
            span: None,
            text: "The dictionary grows without bound.".into(),
            severity: None,
        },
    ]
}

/// A pool with one drill stocked and an attempt started on it through the command's own function.
async fn started() -> Result<(SqlitePool, String, Value)> {
    let pool = pool().await?;
    let program = stocked(&pool).await?;
    let started = review::start(
        &pool,
        &code(&program, "python"),
        AttemptMode::Practice,
        40,
        2,
    )
    .await?;
    let shown = serde_json::to_value(&started.dto).expect("a DTO is JSON");
    let StartedCodeReviewDto::Started { attempt_id, .. } = started.dto else {
        panic!("a drill was ready");
    };
    Ok((pool, attempt_id, shown))
}

#[tokio::test]
async fn starting_a_drill_gives_the_attempt_and_the_drill_as_the_learner_may_see_it() -> Result<()>
{
    let pool = pool().await?;
    let program = stocked(&pool).await?;
    let started = review::start(
        &pool,
        &code(&program, "python"),
        AttemptMode::Interview,
        40,
        2,
    )
    .await?;
    let StartedCodeReviewDto::Started {
        attempt_id,
        mode,
        started_at,
        drill,
    } = started.dto
    else {
        panic!("a drill was ready");
    };
    assert_eq!(mode, AttemptMode::Interview);
    assert!(started_at > 0);
    assert_eq!(drill.title, "A sliding-window rate limiter");
    assert_eq!(
        drill.material,
        CodeReviewMaterialDto::Code {
            language: "python".into()
        }
    );
    assert_eq!(
        drill.files,
        proved()
            .files
            .iter()
            .map(CodeReviewFileDto::from)
            .collect::<Vec<_>>()
    );
    let (attempt, _) = attempts::get(&pool, &attempt_id).await?;
    assert_eq!(attempt.status, AttemptStatus::Open);
    assert_eq!(started.jobs.len(), 1, "the build of the next drill");
    let next = JobStore::new(pool.clone()).get(&started.jobs[0]).await?;
    assert_eq!(next.kind, DRILL_BUILD);
    Ok(())
}

#[tokio::test]
async fn the_learners_view_of_a_drill_holds_nothing_of_its_key() -> Result<()> {
    let (_, _, shown) = started().await?;
    assert_nothing_of_the_key(&shown);
    let object = shown.as_object().expect("an object");
    let mut keys: Vec<&str> = object.keys().map(String::as_str).collect();
    keys.sort_unstable();
    assert_eq!(keys, ["attemptId", "drill", "mode", "startedAt", "status"]);
    assert_eq!(object["status"], "started");
    let mut drill_keys: Vec<&str> = object["drill"]
        .as_object()
        .expect("an object")
        .keys()
        .map(String::as_str)
        .collect();
    drill_keys.sort_unstable();
    assert_eq!(drill_keys, ["files", "lesson", "material", "title"]);
    assert_eq!(
        object["drill"]["material"],
        json!({"kind": "code", "language": "python"})
    );
    assert_eq!(object["drill"]["lesson"]["title"], SPACING_LESSON);
    Ok(())
}

#[tokio::test]
async fn the_drill_names_the_lesson_it_was_built_from_while_the_lesson_is_there() -> Result<()> {
    let pool = pool().await?;
    let program = course(&pool).await?;
    let lessons = lessons_of(&pool, &program).await?;
    let second = DrillSpec {
        program_id: Some(program.clone()),
        lesson_id: Some(lessons[1].clone()),
        ..spec()
    };
    stock_on(&pool, "drill", &second).await?;
    let started = review::start(
        &pool,
        &lesson_of(&program, &lessons[1]),
        AttemptMode::Practice,
        40,
        2,
    )
    .await?;
    let StartedCodeReviewDto::Started {
        attempt_id, drill, ..
    } = started.dto
    else {
        panic!("a drill of the second lesson was ready");
    };
    let named = |title: &str| {
        Some(DrillLessonRefDto {
            program_id: program.clone(),
            lesson_id: lessons[1].clone(),
            title: title.to_owned(),
        })
    };
    assert_eq!(drill.lesson, named(SLEEP_LESSON));

    sqlx::query("UPDATE learning_lessons SET title='Sleep, renamed' WHERE id=?")
        .bind(&lessons[1])
        .execute(&pool)
        .await
        .map_err(db)?;
    assert_eq!(
        review::result(&pool, &attempt_id).await?.drill.lesson,
        named("Sleep, renamed")
    );

    sqlx::query("DELETE FROM learning_lessons WHERE program_id=? AND id=?")
        .bind(&program)
        .bind(&lessons[1])
        .execute(&pool)
        .await
        .map_err(db)?;
    assert_eq!(review::result(&pool, &attempt_id).await?.drill.lesson, None);
    Ok(())
}

#[tokio::test]
async fn a_drill_with_no_course_names_no_lesson() -> Result<()> {
    let pool = pool().await?;
    stock(&pool, "drill").await?;
    let attempt = opened(&pool, "drill", AttemptMode::Practice).await?;
    let shown = review::result(&pool, &attempt.id).await?;
    assert_eq!(shown.drill.lesson, None);
    assert!(serde_json::to_value(&shown).expect("a DTO is JSON")["drill"]["lesson"].is_null());
    Ok(())
}

#[test]
fn the_check_for_the_key_finds_it_where_it_is() {
    // The scan is only worth something if it finds what it looks for.
    let key = proved().planted_key(&unchecked());
    let text = json!({"note": key[0].explanation});
    assert!(!leaks(&text).is_empty(), "an explanation");
    let fix = json!({"more": [key[1].fix.after.trim()]});
    assert!(!leaks(&fix).is_empty(), "the text a fix restores");
    let test = json!({"note": KILLING_TEST});
    assert!(!leaks(&test).is_empty(), "a killing test");
    let field = json!({"proof": "x"});
    assert!(!leaks(&field).is_empty(), "a field of the key");
    assert!(leaks(&json!({"title": "A limiter", "files": []})).is_empty());
}

#[tokio::test]
async fn an_attempt_not_yet_graded_shows_the_learner_nothing_of_the_key() -> Result<()> {
    let (pool, attempt_id, _) = started().await?;
    let open = review::result(&pool, &attempt_id).await?;
    assert_eq!(open.status, AttemptStatus::Open);
    assert_eq!(open.reveal, None);
    assert!(open.findings.is_empty());
    let shown = serde_json::to_value(&open).expect("a DTO is JSON");
    assert!(shown["reveal"].is_null());
    assert_nothing_of_the_key(&shown);

    let acted = review::submit(&pool, &attempt_id, finding_dtos(), None, 61_000).await?;
    assert_eq!(acted.dto.status, AttemptStatus::Submitted);
    assert_eq!(acted.dto.reveal, None);
    assert_eq!(acted.dto.findings, finding_dtos());
    assert_nothing_of_the_key(&serde_json::to_value(&acted.dto).expect("a DTO is JSON"));

    let mut connection = pool.acquire().await.map_err(db)?;
    attempts::repository::record_ungraded_in(
        &mut connection,
        &attempt_id,
        Some(GRADER),
        "No answer.",
    )
    .await?;
    drop(connection);
    let ungraded = review::result(&pool, &attempt_id).await?;
    assert_eq!(ungraded.status, AttemptStatus::CouldntGrade);
    assert_eq!(ungraded.error.as_deref(), Some("No answer."));
    assert_eq!(ungraded.grader_model.as_deref(), Some(GRADER));
    assert_eq!(ungraded.reveal, None);
    assert_nothing_of_the_key(&serde_json::to_value(&ungraded).expect("a DTO is JSON"));
    Ok(())
}

#[tokio::test]
async fn with_no_drill_ready_the_lesson_next_up_is_built_and_the_learner_is_told() -> Result<()> {
    let pool = pool().await?;
    let program = course(&pool).await?;
    let lessons = lessons_of(&pool, &program).await?;
    let started = review::start(
        &pool,
        &code(&program, "python"),
        AttemptMode::Practice,
        50,
        2,
    )
    .await?;
    let StartedCodeReviewDto::Building { job_id } = &started.dto else {
        panic!("none was ready");
    };
    assert_eq!(started.jobs, std::slice::from_ref(job_id));
    assert_eq!(
        serde_json::to_value(&started.dto).expect("a DTO is JSON"),
        json!({"status": "building", "jobId": job_id})
    );
    let job = JobStore::new(pool.clone()).get(job_id).await?;
    assert_eq!(job.kind, DRILL_BUILD);
    let spec = spec_of(&job)?;
    assert_eq!(spec.material, Material::code("python"));
    assert_eq!(spec.lines, 50);
    assert_eq!(spec.program_id.as_deref(), Some(program.as_str()));
    assert_eq!(spec.lesson_id.as_deref(), Some(lessons[0].as_str()));
    let wanted: Vec<String> = targets(&Material::code("python"), &[], 0, 2)
        .iter()
        .map(|pattern| pattern.id.to_owned())
        .collect();
    assert_eq!(spec.patterns, wanted);
    assert_eq!(jobs_of(&pool, DRILL_BUILD).await?, 1);
    Ok(())
}

#[tokio::test]
async fn with_none_ready_the_build_queued_plants_a_pattern_the_schedule_has_due_first() -> Result<()>
{
    let pool = pool().await?;
    let program = course(&pool).await?;
    let due = "concurrency.check_then_act";
    schedule_pattern(&pool, "code:python", due, 1).await?;
    let started = review::start(
        &pool,
        &code(&program, "python"),
        AttemptMode::Practice,
        50,
        2,
    )
    .await?;
    let StartedCodeReviewDto::Building { job_id } = &started.dto else {
        panic!("none was ready");
    };
    let spec = spec_of(&JobStore::new(pool.clone()).get(job_id).await?)?;
    assert_eq!(spec.patterns.len(), 2);
    assert_eq!(spec.patterns.first().map(String::as_str), Some(due));
    Ok(())
}

#[tokio::test]
async fn asking_again_while_a_build_is_under_way_finds_that_build() -> Result<()> {
    let pool = pool().await?;
    let program = course(&pool).await?;
    let asked = code(&program, "python");
    let first = review::start(&pool, &asked, AttemptMode::Practice, 50, 2).await?;
    let again = review::start(&pool, &asked, AttemptMode::Practice, 50, 2).await?;
    assert_eq!(first.dto, again.dto);
    assert_eq!(jobs_of(&pool, DRILL_BUILD).await?, 1);
    Ok(())
}

#[tokio::test]
async fn the_build_queued_when_a_drill_was_taken_is_the_one_the_next_learner_waits_for(
) -> Result<()> {
    let pool = pool().await?;
    let program = stocked(&pool).await?;
    let asked = code(&program, "python");
    let first = review::start(&pool, &asked, AttemptMode::Practice, 50, 2).await?;
    let next = first.jobs[0].clone();
    let second = review::start(&pool, &asked, AttemptMode::Practice, 50, 2).await?;
    assert_eq!(
        second.dto,
        StartedCodeReviewDto::Building { job_id: next },
        "no second build for a language that has one under way"
    );
    assert_eq!(jobs_of(&pool, DRILL_BUILD).await?, 1);
    Ok(())
}

#[tokio::test]
async fn a_request_that_cannot_make_a_build_is_refused_and_takes_no_drill() -> Result<()> {
    let pool = pool().await?;
    let program = stocked(&pool).await?;
    let lessons = lessons_of(&pool, &program).await?;
    set_state(&pool, &lessons[2], "skipped").await?;
    set_state(&pool, &lessons[3], "replaced").await?;
    let other = course(&pool).await?;
    let others = lessons_of(&pool, &other).await?;
    let mut refused = Vec::new();
    for (language, lines, count) in [("python", 0, 2), ("python", 40, 0), ("klingon", 40, 2)] {
        refused.push((
            code(&program, language),
            lines,
            count,
            format!("{language} {lines} {count}"),
        ));
    }
    for (what, lesson) in [
        ("another course's lesson", &others[0]),
        ("a skipped lesson", &lessons[2]),
        ("a replaced lesson", &lessons[3]),
    ] {
        refused.push((lesson_of(&program, lesson), 40, 2, what.to_owned()));
    }
    for (asked, lines, count, what) in refused {
        let result = review::start(&pool, &asked, AttemptMode::Practice, lines, count).await;
        assert!(
            matches!(result, Err(AppError::InvalidInput(_))),
            "{what}: {result:?}"
        );
    }
    assert_eq!(jobs_of(&pool, DRILL_BUILD).await?, 0);
    let still_there = review::start(
        &pool,
        &code(&program, "python"),
        AttemptMode::Practice,
        40,
        2,
    )
    .await?;
    assert!(matches!(
        still_there.dto,
        StartedCodeReviewDto::Started { .. }
    ));
    Ok(())
}

#[tokio::test]
async fn asking_for_a_lesson_takes_its_ready_drill_or_builds_it() -> Result<()> {
    let pool = pool().await?;
    let program = course(&pool).await?;
    let lessons = lessons_of(&pool, &program).await?;
    let first = DrillSpec {
        program_id: Some(program.clone()),
        lesson_id: Some(lessons[0].clone()),
        ..spec()
    };
    stock_on(&pool, "drill", &first).await?;

    let building = review::start(
        &pool,
        &lesson_of(&program, &lessons[1]),
        AttemptMode::Practice,
        40,
        2,
    )
    .await?;
    let StartedCodeReviewDto::Building { job_id } = &building.dto else {
        panic!("no drill of the second lesson was ready");
    };
    let queued = spec_of(&JobStore::new(pool.clone()).get(job_id).await?)?;
    assert_eq!(queued.program_id.as_deref(), Some(program.as_str()));
    assert_eq!(queued.lesson_id.as_deref(), Some(lessons[1].as_str()));
    assert_eq!(status_of(&pool, "drill").await?, "ready");

    let started = review::start(
        &pool,
        &lesson_of(&program, &lessons[0]),
        AttemptMode::Practice,
        40,
        2,
    )
    .await?;
    let StartedCodeReviewDto::Started { attempt_id, .. } = started.dto else {
        panic!("a drill of the first lesson was ready");
    };
    let (_, stored) = attempts::get(&pool, &attempt_id).await?;
    assert_eq!(stored.drill.id, "drill");
    Ok(())
}

#[tokio::test]
async fn a_drill_from_before_lessons_is_never_served() -> Result<()> {
    let pool = pool().await?;
    let program = course(&pool).await?;
    stock(&pool, "no-course").await?;
    let no_lesson = DrillSpec {
        program_id: Some(program.clone()),
        lesson_id: None,
        ..spec()
    };
    stock_on(&pool, "no-lesson", &no_lesson).await?;

    let started = review::start(
        &pool,
        &code(&program, "python"),
        AttemptMode::Practice,
        40,
        2,
    )
    .await?;
    let StartedCodeReviewDto::Building { job_id } = &started.dto else {
        panic!("only drills from before lessons were ready");
    };
    let queued = spec_of(&JobStore::new(pool.clone()).get(job_id).await?)?;
    assert!(queued.lesson_id.is_some());
    assert_eq!(status_of(&pool, "no-course").await?, "ready");
    assert_eq!(status_of(&pool, "no-lesson").await?, "ready");
    Ok(())
}

#[tokio::test]
async fn a_course_with_no_lesson_to_drill_is_refused_and_queues_nothing() -> Result<()> {
    let pool = pool().await?;
    let program = course(&pool).await?;
    for lesson in lessons_of(&pool, &program).await? {
        record_unfit(&pool, &program, &lesson, &Material::code("python")).await?;
    }
    let result = review::start(
        &pool,
        &code(&program, "python"),
        AttemptMode::Practice,
        40,
        2,
    )
    .await;
    assert!(
        matches!(result, Err(AppError::InvalidInput(_))),
        "{result:?}"
    );
    assert_eq!(jobs_of(&pool, DRILL_BUILD).await?, 0);
    Ok(())
}

#[tokio::test]
async fn handing_in_a_review_queues_its_grading_and_reads_back_as_submitted() -> Result<()> {
    let (pool, attempt_id, _) = started().await?;
    let fix: Vec<CodeReviewFileDto> = operator_fixed()
        .iter()
        .map(CodeReviewFileDto::from)
        .collect();
    let acted = review::submit(
        &pool,
        &attempt_id,
        finding_dtos(),
        Some(fix.clone()),
        61_000,
    )
    .await?;
    assert_eq!(acted.dto.attempt_id, attempt_id);
    assert_eq!(acted.dto.status, AttemptStatus::Submitted);
    assert_eq!(acted.dto.time_spent_ms, Some(61_000));
    assert_eq!(acted.dto.fix_files, Some(fix));
    let job = JobStore::new(pool.clone())
        .get(&acted.job.clone().expect("a grading job"))
        .await?;
    assert_eq!(job.kind, DRILL_GRADING);
    assert_eq!(
        review::result(&pool, &attempt_id).await?,
        acted.dto,
        "what is read back is what was returned"
    );
    Ok(())
}

#[tokio::test]
async fn a_review_with_a_time_that_is_not_a_time_is_refused() -> Result<()> {
    let (pool, attempt_id, _) = started().await?;
    let result = review::submit(&pool, &attempt_id, Vec::new(), None, u64::MAX).await;
    assert!(
        matches!(result, Err(AppError::InvalidInput(_))),
        "{result:?}"
    );
    assert_eq!(
        review::result(&pool, &attempt_id).await?.status,
        AttemptStatus::Open
    );
    Ok(())
}

#[tokio::test]
async fn an_attempt_that_is_not_there_is_not_found() -> Result<()> {
    let pool = pool().await?;
    assert!(matches!(
        review::result(&pool, "nothing").await,
        Err(AppError::NotFound(_))
    ));
    assert!(matches!(
        review::regrade(&pool, "nothing").await,
        Err(AppError::NotFound(_))
    ));
    Ok(())
}

#[tokio::test]
async fn an_attempt_that_could_not_be_graded_is_regraded_through_the_command() -> Result<()> {
    let waiting = Waiting::new(vec![operator_finding(), extra_finding()], None).await?;
    let grader = FakeGrader::failing("The model is down.");
    waiting
        .grade(&ScriptedModel::new(Vec::<String>::new()), None, &grader)
        .await?;
    let acted = review::regrade(&waiting.pool, &waiting.attempt).await?;
    assert_eq!(acted.dto.status, AttemptStatus::Submitted);
    assert_eq!(acted.dto.error, None);
    assert!(acted.job.is_some());
    Ok(())
}

#[tokio::test]
async fn a_graded_review_reveals_every_issue_every_finding_the_score_the_ratings_the_fix_and_the_clean_code(
) -> Result<()> {
    let waiting = Waiting::new(
        vec![operator_finding(), extra_finding()],
        Some(operator_fixed()),
    )
    .await?;
    let model = ScriptedModel::new([fix_answer("not", "The literals are still written out.")]);
    let runner = FakeRunner::killing_the_operator();
    let grader = FakeGrader::new(review_assessment());
    waiting.grade(&model, Some(&runner), &grader).await?;

    let result = review::result(&waiting.pool, &waiting.attempt).await?;
    assert_eq!(result.status, AttemptStatus::Graded);
    assert_eq!(result.grader_model.as_deref(), Some(MODEL));
    assert!(result.graded_at.is_some());
    assert_eq!(result.time_spent_ms, Some(90_000));
    assert_eq!(result.error, None);
    assert_eq!(result.findings.len(), 2, "the learner's own comments");
    assert_eq!(
        result.drill.files,
        proved()
            .files
            .iter()
            .map(CodeReviewFileDto::from)
            .collect::<Vec<_>>()
    );
    let reveal = result.reveal.expect("a graded review is revealed");

    // Every key issue: its pattern name, severity, lines, consequence, explanation and fix as a diff, and what came of it.
    assert_eq!(reveal.issues.len(), 2);
    let first = &reveal.issues[0];
    assert_eq!(first.id, "planted-1");
    assert_eq!(first.pattern, OPERATOR);
    assert_eq!(
        first.pattern_name,
        pattern(OPERATOR).expect("a pattern").name
    );
    assert_eq!(first.severity, CodeReviewSeverity::Major);
    assert_eq!(
        first.span,
        CodeReviewSpanDto {
            path: "src/limiter.py".into(),
            start: 13,
            end: 13
        }
    );
    assert_eq!(
        first.consequence,
        "A caller gets one call more than the limit."
    );
    assert_eq!(
        first.explanation,
        "The comparison lets one call too many through. It should refuse at the limit."
    );
    assert_eq!(first.fix.path, "src/limiter.py");
    assert_eq!(
        first
            .fix
            .lines
            .iter()
            .map(|line| (line.kind, line.text.as_str()))
            .collect::<Vec<_>>(),
        [
            (DiffKind::Removed, "        if len(recent) > self.limit:"),
            (DiffKind::Added, "        if len(recent) >= self.limit:"),
        ]
    );
    assert_eq!(
        first.fix.lines,
        line_diff(
            "        if len(recent) > self.limit:\n",
            "        if len(recent) >= self.limit:\n"
        )
    );
    assert_eq!(first.outcome, IssueResultKind::Found);
    assert_eq!(first.finding, Some(0));
    assert_eq!(first.reason, "The finding names the operator.");
    assert_eq!(first.source_quote, None);
    assert_eq!(reveal.source, None);
    let second = &reveal.issues[1];
    assert_eq!(second.severity, CodeReviewSeverity::Nit);
    assert_eq!(second.outcome, IssueResultKind::Missed);
    assert_eq!(second.finding, None);
    assert_eq!(second.reason, "No finding mentions the literals.");

    // Every finding, with what came of it.
    assert_eq!(reveal.findings.len(), 2);
    assert_eq!(reveal.findings[0].text, operator_finding().text);
    assert_eq!(reveal.findings[0].severity, Some(CodeReviewSeverity::Major));
    assert_eq!(reveal.findings[0].outcome, FindingResultKind::Matches);
    assert_eq!(reveal.findings[0].issues, [0]);
    assert_eq!(reveal.findings[0].reason, "It is the changed operator.");
    assert_eq!(reveal.findings[1].span, None);
    assert_eq!(reveal.findings[1].outcome, FindingResultKind::ValidExtra);
    assert!(reveal.findings[1].issues.is_empty());
    assert_eq!(
        reveal.findings[1].reason,
        "The dictionary really does grow."
    );

    // The score's parts.
    let score = &reveal.score;
    assert_eq!(
        (
            score.found,
            score.half_found,
            score.missed,
            score.valid_extras,
            score.false_positives
        ),
        (1, 0, 1, 1, 0)
    );
    assert_eq!(score.weighted_recall, 0.75);
    assert_eq!(score.precision, Some(1.0));
    assert!(score.led_with_the_most_severe);
    assert_eq!(
        score
            .by_severity
            .iter()
            .map(|tally| (tally.severity, tally.found, tally.total))
            .collect::<Vec<_>>(),
        [
            (CodeReviewSeverity::Major, 1, 1),
            (CodeReviewSeverity::Nit, 0, 1)
        ]
    );

    // The ratings.
    assert_eq!(
        reveal
            .ratings
            .iter()
            .map(|rated| (rated.pattern.as_str(), rated.rating))
            .collect::<Vec<_>>(),
        [
            (OPERATOR, PatternRating::Easy),
            (LITERALS, PatternRating::Again)
        ]
    );
    assert_eq!(
        reveal.ratings[1].pattern_name,
        pattern(LITERALS).expect("a pattern").name
    );

    // The fix results.
    let fix = reveal.fix.expect("the learner wrote a fix");
    assert_eq!(fix.len(), 2);
    assert_eq!(
        (fix[0].issue, fix[0].outcome, fix[0].decided_by),
        (0, FixOutcome::Fixed, DecidedBy::Test)
    );
    assert_eq!(
        (fix[1].issue, fix[1].outcome, fix[1].decided_by),
        (1, FixOutcome::NotFixed, DecidedBy::Judge)
    );
    assert_eq!(fix[1].reason, "The literals are still written out.");

    // The clean code beside the drill's.
    assert_eq!(
        reveal.clean_files,
        files()
            .iter()
            .map(CodeReviewFileDto::from)
            .collect::<Vec<_>>()
    );
    Ok(())
}

/// What the first key issue of the drill "drill" breaks of its lesson: a sentence of a source and one of the lesson's
/// own.
fn lesson_link() -> LessonLink {
    LessonLink {
        lesson_id: "lesson".into(),
        sentences: vec![
            LessonSentence {
                text: "Spreading study over sessions helps recall.".into(),
                source: Some("A source".into()),
            },
            LessonSentence {
                text: "Space the sessions out.".into(),
                source: None,
            },
        ],
    }
}

/// Gives the first key issue of the drill "drill" [`lesson_link`].
async fn link_the_first_issue(pool: &SqlitePool) -> Result<()> {
    sqlx::query("UPDATE learning_drills SET key_json=json_set(key_json,'$[0].lesson',json(?)) WHERE id='drill'")
        .bind(serde_json::to_string(&lesson_link()).expect("a link is JSON"))
        .execute(pool)
        .await
        .map(drop)
        .map_err(db)
}

#[tokio::test]
async fn a_revealed_issue_carries_the_lesson_s_words_it_breaks() -> Result<()> {
    let waiting = Waiting::new(vec![operator_finding(), extra_finding()], None).await?;
    link_the_first_issue(&waiting.pool).await?;
    let model = ScriptedModel::new(Vec::<String>::new());
    let grader = FakeGrader::new(review_assessment());
    waiting.grade(&model, None, &grader).await?;

    let revealed = review::result(&waiting.pool, &waiting.attempt)
        .await?
        .reveal
        .expect("a graded review is revealed");
    assert_eq!(
        revealed.issues[0].lesson,
        Some(RevealedLessonLinkDto {
            lesson_id: "lesson".into(),
            sentences: vec![
                RevealedLessonSentenceDto {
                    text: "Spreading study over sessions helps recall.".into(),
                    source: Some("A source".into()),
                },
                RevealedLessonSentenceDto {
                    text: "Space the sessions out.".into(),
                    source: None,
                },
            ],
        })
    );
    assert_eq!(revealed.issues[1].lesson, None);
    Ok(())
}

#[tokio::test]
async fn the_reveal_is_read_in_camel_case_with_flat_outcomes() -> Result<()> {
    let waiting = Waiting::new(
        vec![operator_finding(), extra_finding()],
        Some(operator_fixed()),
    )
    .await?;
    link_the_first_issue(&waiting.pool).await?;
    let model = ScriptedModel::new([fix_answer("not", "The literals are still written out.")]);
    let runner = FakeRunner::killing_the_operator();
    let grader = FakeGrader::new(review_assessment());
    waiting.grade(&model, Some(&runner), &grader).await?;
    let shown = serde_json::to_value(review::result(&waiting.pool, &waiting.attempt).await?)
        .expect("a DTO is JSON");

    assert_eq!(shown["status"], "graded");
    assert!(shown["attemptId"].is_string());
    assert_eq!(shown["timeSpentMs"], 90_000);
    assert_eq!(shown["graderModel"], MODEL);
    assert!(shown["fixFiles"].is_array());
    let reveal = &shown["reveal"];
    assert!(reveal["cleanFiles"].is_array());
    assert_eq!(
        reveal["issues"][0]["patternName"],
        pattern(OPERATOR).expect("a pattern").name
    );
    assert_eq!(reveal["issues"][0]["outcome"], "found");
    assert_eq!(reveal["issues"][0]["finding"], 0);
    assert!(reveal["source"].is_null(), "code has no source passage");
    assert!(reveal["issues"][0]["sourceQuote"].is_null());
    assert_eq!(reveal["issues"][0]["fix"]["lines"][0]["kind"], "removed");
    assert_eq!(reveal["issues"][1]["outcome"], "missed");
    let lesson = &reveal["issues"][0]["lesson"];
    assert_eq!(lesson["lessonId"], "lesson");
    assert_eq!(
        lesson["sentences"][0]["text"],
        "Spreading study over sessions helps recall."
    );
    assert_eq!(lesson["sentences"][0]["source"], "A source");
    assert!(lesson["sentences"][1]["source"].is_null());
    assert!(reveal["issues"][1]["lesson"].is_null());
    assert_eq!(reveal["findings"][1]["outcome"], "valid_extra");
    assert_eq!(reveal["ratings"][0]["rating"], "easy");
    assert_eq!(reveal["score"]["halfFound"], 0);
    assert_eq!(reveal["score"]["ledWithTheMostSevere"], true);
    assert_eq!(reveal["fix"][0]["decidedBy"], "test");
    assert_eq!(reveal["fix"][0]["suite"]["status"], "passed");
    assert_eq!(
        reveal["fix"][0]["suite"]["ranBeside"],
        json!([1]),
        "the suite ran beside the fix of the literals, which the learner left"
    );
    assert_eq!(
        reveal["fix"][0]["gradedBeside"],
        json!([1]),
        "the operator's test ran beside the fix of the literals, which the learner left"
    );
    assert_eq!(reveal["fix"][1]["gradedBeside"], json!([]));
    Ok(())
}

/// The sizes of the screen's Short, Standard and Long: about this many lines, with this many patterns to plant.
const SCREEN_SIZES: [(u32, u32); 3] = [(40, 3), (80, 4), (140, 5)];

#[tokio::test]
async fn every_language_the_screen_offers_starts_at_every_size_it_offers() -> Result<()> {
    let pool = pool().await?;
    for language in LANGUAGES {
        for (lines, count) in SCREEN_SIZES {
            let program = course(&pool).await?;
            let started = review::start(
                &pool,
                &code(&program, language),
                AttemptMode::Practice,
                lines,
                count,
            )
            .await?;
            assert!(
                matches!(started.dto, StartedCodeReviewDto::Building { .. }),
                "{language} {lines} {count}: {started:?}"
            );
        }
    }
    Ok(())
}

#[tokio::test]
async fn an_issue_a_blind_review_raised_is_revealed_as_also_valid_and_a_planted_one_is_not(
) -> Result<()> {
    let waiting = Waiting::new(vec![operator_finding(), extra_finding()], None).await?;
    let model = ScriptedModel::new(Vec::<String>::new());
    let grader = FakeGrader::new(review_assessment());
    waiting.grade(&model, None, &grader).await?;
    let (attempt, mut stored) = attempts::get(&waiting.pool, &waiting.attempt).await?;
    stored.drill.key[1].origin = Origin::FoundInReview;

    let revealed = reveal::result_of(&attempt, &stored)?
        .reveal
        .expect("a graded review is revealed");
    assert!(!revealed.issues[0].also_valid);
    assert!(revealed.issues[1].also_valid);
    let shown = serde_json::to_value(&revealed.issues).expect("a DTO is JSON");
    assert_eq!(shown[0]["alsoValid"], false);
    assert_eq!(shown[1]["alsoValid"], true);
    Ok(())
}

#[tokio::test]
async fn ratings_are_revealed_as_stored_whatever_their_number_against_the_key() -> Result<()> {
    let waiting = Waiting::new(vec![operator_finding(), extra_finding()], None).await?;
    let model = ScriptedModel::new(Vec::<String>::new());
    let grader = FakeGrader::new(review_assessment());
    waiting.grade(&model, None, &grader).await?;
    let (mut attempt, mut stored) = attempts::get(&waiting.pool, &waiting.attempt).await?;
    stored.drill.key[1].origin = Origin::FoundInReview;
    let rating = |pattern: &str, rating| IssueRating {
        pattern: pattern.to_owned(),
        rating,
    };

    // An attempt graded before review-found issues stopped being rated holds one rating for each key issue.
    attempt.ratings = Some(vec![
        rating(OPERATOR, PatternRating::Easy),
        rating(LITERALS, PatternRating::Again),
    ]);
    let both = reveal::result_of(&attempt, &stored)?
        .reveal
        .expect("a graded review is revealed");
    assert_eq!(
        both.ratings
            .iter()
            .map(|rated| (rated.pattern.as_str(), rated.rating))
            .collect::<Vec<_>>(),
        [
            (OPERATOR, PatternRating::Easy),
            (LITERALS, PatternRating::Again)
        ]
    );

    // One that has fewer ratings than the key has issues names each rating's own pattern.
    attempt.ratings = Some(vec![rating(LITERALS, PatternRating::Good)]);
    let one = reveal::result_of(&attempt, &stored)?
        .reveal
        .expect("a graded review is revealed");
    assert_eq!(one.ratings.len(), 1);
    assert_eq!(one.ratings[0].pattern, LITERALS);
    assert_eq!(
        one.ratings[0].pattern_name,
        pattern(LITERALS).expect("a pattern").name
    );
    Ok(())
}

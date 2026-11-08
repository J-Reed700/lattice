//! A course's review drills as its Practice view reads them (docs/design/2026-11-07-drills-in-the-course.md section 8):
//! the language code drills are offered in, what a start with no lesson comes to, each lesson's standing by module, the
//! course's sources and its latest attempts, and the checks a start makes of the course and its sources.

use super::support::{
    add_blocks, course, course_of, extra_finding, lesson_titled, lessons_of, opened,
    operator_finding, prepare_lesson, record_unfit, review_assessment, set_state, spacing_passage,
    spec, stock_on, FakeGrader, ScriptedModel, Waiting, SLEEP_LESSON, SPACING_LESSON,
};
use crate::drills::attempts::{self, AttemptMode, AttemptStatus};
use crate::drills::dto::{
    CourseDrillAttemptDto, CourseDrillsDto, DrillLessonDto, DrillLessonRefDto, DrillMaterialDto,
    DrillModuleDto, DrillStandingDto, NextDrillDto, NextDrillStatus, StartedCodeReviewDto,
};
use crate::drills::jobs::{spec_of, DRILL_BUILD};
use crate::drills::review::{self, CourseDrill};
use crate::drills::spec::DrillSpec;
use crate::persistence::{db, encode, now};
use crate::tests::pool;
use lattice_core::domain::learning::drills::{Material, LANGUAGES};
use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::runtime::jobs::JobStore;
use sqlx::SqlitePool;

/// The course's drills with the language it suggests.
async fn drills_of(pool: &SqlitePool, program_id: &str) -> Result<CourseDrillsDto> {
    review::course_drills(pool, program_id, None).await
}

/// A request for a drill on `material` from the lesson `lesson_id` of the course `program_id`, or from any.
fn asking(program_id: &str, lesson_id: Option<&str>, material: Material) -> CourseDrill {
    CourseDrill {
        program_id: program_id.to_owned(),
        lesson_id: lesson_id.map(str::to_owned),
        material,
    }
}

/// Stocks the python drill `id`, built from the lesson `lesson_id` of the course `program_id`.
async fn stock_for(pool: &SqlitePool, id: &str, program_id: &str, lesson_id: &str) -> Result<()> {
    let spec = DrillSpec {
        program_id: Some(program_id.to_owned()),
        lesson_id: Some(lesson_id.to_owned()),
        ..spec()
    };
    stock_on(pool, id, &spec).await
}

/// Writes the language of the stocked drill `id`.
async fn in_language(pool: &SqlitePool, id: &str, language: &str) -> Result<()> {
    sqlx::query("UPDATE learning_drills SET language=? WHERE id=?")
        .bind(language)
        .bind(id)
        .execute(pool)
        .await
        .map(drop)
        .map_err(db)
}

/// Makes the stocked drill `id` a drill on a passage.
async fn on_a_passage(pool: &SqlitePool, id: &str) -> Result<()> {
    sqlx::query(
        "UPDATE learning_drills SET material='passage',language=NULL,source_json=? WHERE id=?",
    )
    .bind(encode(&spacing_passage())?)
    .bind(id)
    .execute(pool)
    .await
    .map(drop)
    .map_err(db)
}

/// Gives the lesson a schedule that makes it due at `due_at`.
async fn schedule_lesson(
    pool: &SqlitePool,
    program_id: &str,
    lesson_id: &str,
    due_at: i64,
) -> Result<()> {
    sqlx::query("INSERT INTO learning_drill_lesson_schedules(program_id,lesson_id,stability,difficulty,last_reviewed_at,due_at,interval_days,review_count,lapses) VALUES(?,?,1.0,5.0,0,?,1,1,0)")
        .bind(program_id)
        .bind(lesson_id)
        .bind(due_at)
        .execute(pool)
        .await
        .map(drop)
        .map_err(db)
}

async fn builds(pool: &SqlitePool) -> Result<i64> {
    sqlx::query_scalar("SELECT count(*) FROM jobs WHERE kind=?")
        .bind(DRILL_BUILD)
        .fetch_one(pool)
        .await
        .map_err(db)
}

/// What a start with no lesson comes to.
fn next(lesson_id: &str, status: NextDrillStatus) -> Option<NextDrillDto> {
    Some(NextDrillDto {
        lesson_id: lesson_id.to_owned(),
        status,
    })
}

/// A fenced block of code in `language`, `count` times.
fn fenced(language: &str, count: usize) -> String {
    (0..count)
        .map(|number| format!("```{language}\nstep_{number}()\n```\n"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// The lessons of the view in the standing of a course nothing has been drilled in.
fn unseen(lesson_id: &str, title: &str, current: bool) -> DrillLessonDto {
    DrillLessonDto {
        lesson_id: lesson_id.to_owned(),
        title: title.to_owned(),
        standing: DrillStandingDto::New,
        due_at: None,
        fits_code: true,
        fits_passage: true,
        current,
        completed: false,
    }
}

/// The view's lesson `lesson_id` of the `modules` the course lists.
fn lesson_in<'a>(shown: &'a CourseDrillsDto, lesson_id: &str) -> Option<&'a DrillLessonDto> {
    shown
        .modules
        .iter()
        .flat_map(|module| &module.lessons)
        .find(|lesson| lesson.lesson_id == lesson_id)
}

#[tokio::test]
async fn a_new_course_lists_every_lesson_as_new_by_module_with_the_current_lesson_next(
) -> Result<()> {
    let pool = pool().await?;
    let program = course(&pool).await?;
    let lessons = lessons_of(&pool, &program).await?;
    let modules: Vec<(String, String)> =
        sqlx::query_as("SELECT id,title FROM learning_modules WHERE program_id=? ORDER BY ordinal")
            .bind(&program)
            .fetch_all(&pool)
            .await
            .map_err(db)?;
    let titles = [SPACING_LESSON, SLEEP_LESSON, "Lesson 1-0", "Lesson 1-1"];
    let lesson = |position: usize| unseen(&lessons[position], titles[position], position == 0);

    let shown = drills_of(&pool, &program).await?;
    assert_eq!(
        shown.modules,
        [
            DrillModuleDto {
                module_id: modules[0].0.clone(),
                title: modules[0].1.clone(),
                lessons: vec![lesson(0), lesson(1)],
            },
            DrillModuleDto {
                module_id: modules[1].0.clone(),
                title: modules[1].1.clone(),
                lessons: vec![lesson(2), lesson(3)],
            },
        ]
    );
    assert_eq!(shown.next_code, next(&lessons[0], NextDrillStatus::ToBuild));
    assert_eq!(
        shown.next_passage,
        next(&lessons[0], NextDrillStatus::ToBuild)
    );
    assert!(shown.has_sources);
    assert!(shown.recent.is_empty());
    assert_eq!(shown.language, LANGUAGES[0]);
    Ok(())
}

#[tokio::test]
async fn the_language_is_the_one_most_of_the_prepared_lessons_code_is_in() -> Result<()> {
    let pool = pool().await?;
    let program = course(&pool).await?;
    let lessons = lessons_of(&pool, &program).await?;
    prepare_lesson(
        &pool,
        &lessons[0],
        &[
            ("explanation", &fenced("rust", 2), &[]),
            ("worked_example", &fenced("python", 1), &[]),
        ],
    )
    .await?;
    add_blocks(
        &pool,
        &lessons[1],
        &[("explanation", &fenced("go", 3), &[])],
    )
    .await?;
    prepare_lesson(
        &pool,
        &lessons[2],
        &[("explanation", &fenced("go", 3), &[])],
    )
    .await?;
    set_state(&pool, &lessons[2], "replaced").await?;

    assert_eq!(drills_of(&pool, &program).await?.language, "rust");
    Ok(())
}

#[tokio::test]
async fn the_course_keeps_the_language_of_its_latest_code_drill() -> Result<()> {
    let pool = pool().await?;
    let program = course(&pool).await?;
    let lessons = lessons_of(&pool, &program).await?;
    prepare_lesson(
        &pool,
        &lessons[0],
        &[("explanation", &fenced("rust", 2), &[])],
    )
    .await?;
    assert_eq!(drills_of(&pool, &program).await?.language, "rust");

    stock_for(&pool, "in-go", &program, &lessons[0]).await?;
    in_language(&pool, "in-go", "go").await?;
    opened(&pool, "in-go", AttemptMode::Practice).await?;
    assert_eq!(drills_of(&pool, &program).await?.language, "go");

    stock_for(&pool, "in-python", &program, &lessons[0]).await?;
    opened(&pool, "in-python", AttemptMode::Practice).await?;
    assert_eq!(drills_of(&pool, &program).await?.language, "python");
    Ok(())
}

#[tokio::test]
async fn with_nothing_to_go_on_the_language_is_the_one_last_drilled_in_any_course() -> Result<()> {
    let pool = pool().await?;
    let elsewhere = course(&pool).await?;
    let elsewhere_lessons = lessons_of(&pool, &elsewhere).await?;
    stock_for(&pool, "in-java", &elsewhere, &elsewhere_lessons[0]).await?;
    in_language(&pool, "in-java", "java").await?;
    opened(&pool, "in-java", AttemptMode::Practice).await?;

    let program = course(&pool).await?;
    assert_eq!(drills_of(&pool, &program).await?.language, "java");
    Ok(())
}

#[tokio::test]
async fn a_language_asked_for_is_the_one_offered_and_must_be_in_the_catalogue() -> Result<()> {
    let pool = pool().await?;
    let program = course(&pool).await?;
    let lessons = lessons_of(&pool, &program).await?;
    stock_for(&pool, "drill", &program, &lessons[1]).await?;

    let go = review::course_drills(&pool, &program, Some("go")).await?;
    assert_eq!(go.language, "go");
    assert_eq!(go.next_code, next(&lessons[0], NextDrillStatus::ToBuild));
    let python = review::course_drills(&pool, &program, Some("python")).await?;
    assert_eq!(python.language, "python");
    assert_eq!(python.next_code, next(&lessons[1], NextDrillStatus::Ready));
    let result = review::course_drills(&pool, &program, Some("klingon")).await;
    assert!(
        matches!(result, Err(AppError::InvalidInput(_))),
        "{result:?}"
    );
    Ok(())
}

#[tokio::test]
async fn a_start_with_no_lesson_opens_an_attempt_on_the_ready_drill_the_view_names() -> Result<()> {
    let pool = pool().await?;
    let program = course(&pool).await?;
    let lessons = lessons_of(&pool, &program).await?;
    stock_for(&pool, "drill", &program, &lessons[1]).await?;
    assert_eq!(
        drills_of(&pool, &program).await?.next_code,
        next(&lessons[1], NextDrillStatus::Ready)
    );

    let started = review::start(
        &pool,
        &asking(&program, None, Material::code("python")),
        AttemptMode::Practice,
        40,
        2,
    )
    .await?;
    let StartedCodeReviewDto::Started { attempt_id, .. } = started.dto else {
        panic!("a drill was ready");
    };
    let (_, stored) = attempts::get(&pool, &attempt_id).await?;
    assert_eq!(stored.drill.id, "drill");
    Ok(())
}

#[tokio::test]
async fn a_start_with_no_lesson_waits_for_the_build_the_view_names() -> Result<()> {
    let pool = pool().await?;
    let program = course(&pool).await?;
    let lessons = lessons_of(&pool, &program).await?;
    let python = Material::code("python");
    let building = review::start(
        &pool,
        &asking(&program, Some(&lessons[1]), python.clone()),
        AttemptMode::Practice,
        40,
        2,
    )
    .await?;
    let StartedCodeReviewDto::Building { job_id } = building.dto else {
        panic!("none was ready");
    };
    assert_eq!(
        drills_of(&pool, &program).await?.next_code,
        next(&lessons[1], NextDrillStatus::Building)
    );

    let again = review::start(
        &pool,
        &asking(&program, None, python),
        AttemptMode::Practice,
        40,
        2,
    )
    .await?;
    assert_eq!(again.dto, StartedCodeReviewDto::Building { job_id });
    assert_eq!(builds(&pool).await?, 1);
    Ok(())
}

#[tokio::test]
async fn a_start_with_no_lesson_builds_the_lesson_the_view_names() -> Result<()> {
    let pool = pool().await?;
    let program = course(&pool).await?;
    let lessons = lessons_of(&pool, &program).await?;
    assert_eq!(
        drills_of(&pool, &program).await?.next_code,
        next(&lessons[0], NextDrillStatus::ToBuild)
    );

    let started = review::start(
        &pool,
        &asking(&program, None, Material::code("python")),
        AttemptMode::Practice,
        40,
        2,
    )
    .await?;
    let StartedCodeReviewDto::Building { job_id } = &started.dto else {
        panic!("none was ready");
    };
    let queued = spec_of(&JobStore::new(pool.clone()).get(job_id).await?)?;
    assert_eq!(queued.lesson_id.as_deref(), Some(lessons[0].as_str()));
    Ok(())
}

#[tokio::test]
async fn each_lesson_stands_by_its_schedule_and_its_drills() -> Result<()> {
    let pool = pool().await?;
    let program = course(&pool).await?;
    let lessons = lessons_of(&pool, &program).await?;
    let due = now() - 10_000;
    let rested = now() + 86_400_000;
    schedule_lesson(&pool, &program, &lessons[0], due).await?;
    schedule_lesson(&pool, &program, &lessons[1], rested).await?;
    stock_for(&pool, "in-go", &program, &lessons[2]).await?;
    in_language(&pool, "in-go", "go").await?;

    let shown = review::course_drills(&pool, &program, Some("python")).await?;
    let standing = |position: usize| {
        lesson_in(&shown, &lessons[position]).map(|lesson| (lesson.standing, lesson.due_at))
    };
    assert_eq!(standing(0), Some((DrillStandingDto::Due, Some(due))));
    assert_eq!(standing(1), Some((DrillStandingDto::Rested, Some(rested))));
    assert_eq!(standing(2), Some((DrillStandingDto::Pending, None)));
    assert_eq!(standing(3), Some((DrillStandingDto::New, None)));
    assert_eq!(shown.next_code, next(&lessons[0], NextDrillStatus::ToBuild));
    Ok(())
}

#[tokio::test]
async fn a_lesson_that_does_not_fit_a_material_says_so_for_that_material_alone() -> Result<()> {
    let pool = pool().await?;
    let program = course(&pool).await?;
    let lessons = lessons_of(&pool, &program).await?;
    record_unfit(&pool, &program, &lessons[0], &Material::code("python")).await?;
    record_unfit(&pool, &program, &lessons[1], &Material::Passage).await?;

    let shown = review::course_drills(&pool, &program, Some("python")).await?;
    let fits = |position: usize| {
        lesson_in(&shown, &lessons[position]).map(|lesson| (lesson.fits_code, lesson.fits_passage))
    };
    assert_eq!(fits(0), Some((false, true)));
    assert_eq!(fits(1), Some((true, false)));
    assert_eq!(shown.next_code, next(&lessons[1], NextDrillStatus::ToBuild));
    assert_eq!(
        shown.next_passage,
        next(&lessons[0], NextDrillStatus::ToBuild)
    );
    Ok(())
}

#[tokio::test]
async fn a_course_with_no_source_offers_no_passage_to_build() -> Result<()> {
    let pool = pool().await?;
    let program = course_of(&pool, &[]).await?;
    let lessons = lessons_of(&pool, &program).await?;

    let shown = drills_of(&pool, &program).await?;
    assert!(!shown.has_sources);
    assert_eq!(shown.next_passage, None);
    assert_eq!(shown.next_code, next(&lessons[0], NextDrillStatus::ToBuild));

    stock_for(&pool, "drill", &program, &lessons[0]).await?;
    on_a_passage(&pool, "drill").await?;
    let shown = drills_of(&pool, &program).await?;
    assert!(!shown.has_sources);
    assert_eq!(
        shown.next_passage,
        next(&lessons[0], NextDrillStatus::Ready)
    );
    Ok(())
}

#[tokio::test]
async fn the_latest_attempts_of_the_course_are_listed_newest_first() -> Result<()> {
    let pool = pool().await?;
    let program = course(&pool).await?;
    let lessons = lessons_of(&pool, &program).await?;
    let other = course(&pool).await?;
    let other_lessons = lessons_of(&pool, &other).await?;
    let mut opened_here = Vec::new();
    for number in 0..12 {
        let id = format!("drill-{number:02}");
        stock_for(&pool, &id, &program, &lessons[0]).await?;
        let mode = if number % 2 == 0 {
            AttemptMode::Practice
        } else {
            AttemptMode::Interview
        };
        opened_here.push(opened(&pool, &id, mode).await?);
    }
    stock_for(&pool, "elsewhere", &other, &other_lessons[0]).await?;
    opened(&pool, "elsewhere", AttemptMode::Rapid).await?;

    let shown = drills_of(&pool, &program).await?;
    let expected: Vec<CourseDrillAttemptDto> = opened_here
        .iter()
        .rev()
        .take(10)
        .map(|attempt| CourseDrillAttemptDto {
            attempt_id: attempt.id.clone(),
            status: AttemptStatus::Open,
            mode: attempt.mode,
            started_at: attempt.started_at,
            title: "A sliding-window rate limiter".into(),
            material: DrillMaterialDto::Code {
                language: "python".into(),
            },
            lesson: Some(DrillLessonRefDto {
                program_id: program.clone(),
                lesson_id: lessons[0].clone(),
                title: SPACING_LESSON.into(),
            }),
            score: None,
        })
        .collect();
    assert_eq!(expected.len(), 10);
    assert_eq!(shown.recent, expected);
    Ok(())
}

#[tokio::test]
async fn a_graded_attempt_is_listed_with_its_score() -> Result<()> {
    let waiting = Waiting::new(vec![operator_finding(), extra_finding()], None).await?;
    let program = course(&waiting.pool).await?;
    let lessons = lessons_of(&waiting.pool, &program).await?;
    let model = ScriptedModel::new(Vec::<String>::new());
    let grader = FakeGrader::new(review_assessment());
    waiting.grade(&model, None, &grader).await?;
    sqlx::query("UPDATE learning_drills SET program_id=?,lesson_id=? WHERE id='drill'")
        .bind(&program)
        .bind(&lessons[0])
        .execute(&waiting.pool)
        .await
        .map_err(db)?;

    let revealed = review::result(&waiting.pool, &waiting.attempt)
        .await?
        .reveal
        .expect("a graded review is revealed");
    let attempt = waiting.attempt().await?;
    let shown = drills_of(&waiting.pool, &program).await?;
    assert_eq!(
        shown.recent,
        [CourseDrillAttemptDto {
            attempt_id: waiting.attempt.clone(),
            status: AttemptStatus::Graded,
            mode: AttemptMode::Practice,
            started_at: attempt.started_at,
            title: "A sliding-window rate limiter".into(),
            material: DrillMaterialDto::Code {
                language: "python".into()
            },
            lesson: Some(DrillLessonRefDto {
                program_id: program.clone(),
                lesson_id: lessons[0].clone(),
                title: SPACING_LESSON.into(),
            }),
            score: Some(revealed.score),
        }]
    );
    Ok(())
}

#[tokio::test]
async fn a_course_that_is_not_there_is_not_found() -> Result<()> {
    let pool = pool().await?;
    let result = review::course_drills(&pool, "nothing", None).await;
    assert!(matches!(result, Err(AppError::NotFound(_))), "{result:?}");
    for material in [Material::code("python"), Material::Passage] {
        let result = review::start(
            &pool,
            &asking("nothing", None, material.clone()),
            AttemptMode::Practice,
            40,
            2,
        )
        .await;
        assert!(
            matches!(result, Err(AppError::NotFound(_))),
            "{material:?}: {result:?}"
        );
    }
    assert_eq!(builds(&pool).await?, 0);
    Ok(())
}

#[tokio::test]
async fn a_passage_drill_of_a_course_with_no_source_is_refused_unless_one_is_ready() -> Result<()> {
    let pool = pool().await?;
    let program = course_of(&pool, &[]).await?;
    let lessons = lessons_of(&pool, &program).await?;
    let first = lesson_titled(&pool, &program, SPACING_LESSON).await?;
    assert_eq!(first, lessons[0]);
    for lesson in [Some(first.as_str()), None] {
        let result = review::start(
            &pool,
            &asking(&program, lesson, Material::Passage),
            AttemptMode::Practice,
            4,
            2,
        )
        .await;
        assert!(
            matches!(result, Err(AppError::InvalidInput(_))),
            "{lesson:?}: {result:?}"
        );
    }
    assert_eq!(builds(&pool).await?, 0);

    stock_for(&pool, "drill", &program, &first).await?;
    on_a_passage(&pool, "drill").await?;
    let started = review::start(
        &pool,
        &asking(&program, None, Material::Passage),
        AttemptMode::Practice,
        4,
        2,
    )
    .await?;
    let StartedCodeReviewDto::Started { attempt_id, .. } = started.dto else {
        panic!("a drill was ready");
    };
    let (_, stored) = attempts::get(&pool, &attempt_id).await?;
    assert_eq!(stored.drill.id, "drill");
    Ok(())
}

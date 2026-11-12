//! The evidence a lesson brief carries: a prepared lesson's own teaching, what the course's sources say of an outline
//! lesson, and how much of either the request's room and the lesson's minutes allow.

use super::support::{
    course, course_of, lesson_titled, prepare_lesson, ScriptedModel, SLEEP, SPACING, SPACING_LESSON,
};
use crate::drills::brief::lesson_brief;
use crate::drills::lesson::LessonContext;
use crate::tests::pool;
use lattice_core::application::ports::LLMPort;
use lattice_core::domain::learning::drills::{evidence_units, LessonSentence};
use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::persistence::Database;
use sqlx::SqlitePool;

fn none() -> ScriptedModel {
    ScriptedModel::new(Vec::<String>::new())
}

fn unit(text: &str, source: Option<&str>) -> LessonSentence {
    LessonSentence {
        text: text.to_owned(),
        source: source.map(str::to_owned),
    }
}

/// The units of `text`, each under the source titled `title`.
fn units_under(text: &str, title: &str) -> Vec<LessonSentence> {
    evidence_units(text)
        .into_iter()
        .map(|text| unit(&text, Some(title)))
        .collect()
}

/// What the model counts the lesson's brief with `evidence` at.
fn tokens(model: &ScriptedModel, context: &LessonContext, evidence: &[LessonSentence]) -> usize {
    model.count_tokens(&context.brief(evidence.to_vec()).render())
}

/// The spacing lesson of a new course from [`course`], prepared with blocks of four kinds, and its context.
async fn prepared(pool: &SqlitePool, database: &Database) -> Result<LessonContext> {
    let program = course(database).await?;
    let lesson = lesson_titled(pool, &program, SPACING_LESSON).await?;
    prepare_lesson(
        pool,
        &lesson,
        &[
            (
                "explanation",
                "Spacing raises recall. Gaps matter most.",
                &[],
            ),
            ("check", "Why does it?", &[]),
            (
                "worked_example",
                "A learner reviews on day one.\n\n```python\nschedule(3)\n```\n",
                &[],
            ),
            ("recap", "Space your study.", &[]),
        ],
    )
    .await?;
    LessonContext::load(database, &program, &lesson).await
}

/// The units of [`prepared`]'s teaching.
fn teaching_units() -> Vec<LessonSentence> {
    [
        "Spacing raises recall.",
        "Gaps matter most.",
        "A learner reviews on day one.",
        "```python\nschedule(3)\n```",
        "Space your study.",
    ]
    .map(|text| unit(text, None))
    .to_vec()
}

#[tokio::test]
async fn a_prepared_lessons_evidence_is_the_units_of_its_teaching_in_order_with_no_source(
) -> Result<()> {
    let pool = pool().await?;
    let database = Database::new(pool.clone());
    let context = prepared(&pool, &database).await?;
    let brief = lesson_brief(&none(), &database, &context, usize::MAX).await?;
    assert_eq!(brief.evidence, teaching_units());
    assert_eq!(brief.lesson_id, context.lesson_id);
    assert_eq!(brief.course, "A generic learning program");
    assert_eq!(brief.lesson, SPACING_LESSON);
    Ok(())
}

#[tokio::test]
async fn a_prepared_lesson_with_room_for_its_first_two_units_only_takes_exactly_those_two(
) -> Result<()> {
    let pool = pool().await?;
    let database = Database::new(pool.clone());
    let context = prepared(&pool, &database).await?;
    let model = none();
    let all = teaching_units();
    let room = tokens(&model, &context, &all[..2]);
    assert!(room < tokens(&model, &context, &all[..3]));
    let brief = lesson_brief(&model, &database, &context, room).await?;
    assert_eq!(brief.evidence, all[..2].to_vec());
    assert!(model.count_tokens(&brief.render()) <= room);
    Ok(())
}

#[tokio::test]
async fn a_room_of_nothing_and_a_room_that_holds_the_brief_but_not_its_first_unit_are_a_window_too_small(
) -> Result<()> {
    let pool = pool().await?;
    let database = Database::new(pool.clone());
    let context = prepared(&pool, &database).await?;
    let model = none();
    let all = teaching_units();
    let bare = tokens(&model, &context, &[]);
    assert!(bare < tokens(&model, &context, &all[..1]));
    for room in [0, bare - 1, bare] {
        let error = lesson_brief(&model, &database, &context, room)
            .await
            .unwrap_err();
        assert!(
            matches!(&error, AppError::WindowTooSmall(_)),
            "{room}: {error:?}"
        );
    }
    Ok(())
}

/// The spacing lesson of a new course from [`course`], an outline, and its context.
async fn outline(pool: &SqlitePool, database: &Database) -> Result<LessonContext> {
    let program = course(database).await?;
    let lesson = lesson_titled(pool, &program, SPACING_LESSON).await?;
    LessonContext::load(database, &program, &lesson).await
}

#[tokio::test]
async fn an_outline_lessons_evidence_is_what_the_sources_say_of_it_best_first_under_their_titles(
) -> Result<()> {
    let pool = pool().await?;
    let database = Database::new(pool.clone());
    let context = outline(&pool, &database).await?;
    let brief = lesson_brief(&none(), &database, &context, usize::MAX).await?;
    let spacing = units_under(SPACING, "Spacing and memory");
    assert_eq!(
        brief.evidence.first(),
        Some(&unit(
            "In a 2006 study by Cepeda and colleagues, learners who spread their study over several sessions recalled 14% more a week later than learners who studied in one session.",
            Some("Spacing and memory")
        ))
    );
    assert_eq!(brief.evidence.get(..spacing.len()), Some(&spacing[..]));
    Ok(())
}

#[tokio::test]
async fn an_outline_lesson_takes_no_more_than_its_planned_minutes_take_to_read_but_always_its_first_passage(
) -> Result<()> {
    let pool = pool().await?;
    let database = Database::new(pool.clone());
    let context = outline(&pool, &database).await?;
    let spacing = units_under(SPACING, "Spacing and memory");
    let sleep = units_under(SLEEP, "Sleep and memory");
    assert!(
        !spacing.is_empty() && !sleep.is_empty(),
        "each source has units to read"
    );
    let long = lesson_brief(&none(), &database, &context, usize::MAX).await?;
    assert_eq!(long.evidence.get(..spacing.len()), Some(&spacing[..]));
    assert_eq!(
        long.evidence.get(spacing.len()..),
        Some(&sleep[..]),
        "twenty minutes read both passages"
    );

    sqlx::query("UPDATE learning_lessons SET estimated_minutes=0 WHERE id=?")
        .bind(&context.lesson_id)
        .execute(&pool)
        .await?;
    let context = LessonContext::load(&database, &context.program_id, &context.lesson_id).await?;
    assert_eq!(context.estimated_minutes, 0);
    let none_planned = lesson_brief(&none(), &database, &context, usize::MAX).await?;
    assert_eq!(none_planned.evidence, spacing, "one passage only");
    Ok(())
}

#[tokio::test]
async fn an_outline_lesson_the_sources_say_nothing_of_has_no_evidence_and_is_not_an_error(
) -> Result<()> {
    let pool = pool().await?;
    let database = Database::new(pool.clone());
    let context = outline(&pool, &database).await?;
    sqlx::query(
        "UPDATE learning_lessons SET title='Xylophone tuning',objective='Tune a xylophone' WHERE id=?",
    )
    .bind(&context.lesson_id)
    .execute(&pool)
    .await?;
    let context = LessonContext::load(&database, &context.program_id, &context.lesson_id).await?;
    let brief = lesson_brief(&none(), &database, &context, usize::MAX).await?;
    assert!(brief.evidence.is_empty());
    assert_eq!(brief.lesson, "Xylophone tuning");
    assert_eq!(brief.course, "A generic learning program");
    Ok(())
}

#[tokio::test]
async fn a_course_without_sources_gives_an_outline_lesson_no_evidence() -> Result<()> {
    let pool = pool().await?;
    let database = Database::new(pool.clone());
    let program = course_of(&database, &[]).await?;
    let lesson = lesson_titled(&pool, &program, SPACING_LESSON).await?;
    let context = LessonContext::load(&database, &program, &lesson).await?;
    let brief = lesson_brief(&none(), &database, &context, usize::MAX).await?;
    assert!(brief.evidence.is_empty());
    Ok(())
}

#[tokio::test]
async fn an_outline_lesson_whose_first_passage_does_not_fit_the_room_is_a_window_too_small(
) -> Result<()> {
    let pool = pool().await?;
    let database = Database::new(pool.clone());
    let context = outline(&pool, &database).await?;
    let model = none();
    let bare = tokens(&model, &context, &[]);
    let error = lesson_brief(&model, &database, &context, bare)
        .await
        .unwrap_err();
    assert!(
        matches!(&error, AppError::WindowTooSmall(message) if message.contains("first passage")),
        "{error:?}"
    );
    Ok(())
}

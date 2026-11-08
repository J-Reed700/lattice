//! The lesson a drill is built from: what is read of its course, its module and its blocks, and the query that finds
//! what the course's sources say of it.

use super::support::{
    add_blocks, course, lesson_titled, prepare_lesson, SLEEP_LESSON, SPACING_LESSON,
};
use crate::features::learning::drills::lesson::LessonContext;
use crate::features::learning::tests::pool;
use lattice_core::shared::error::{AppError, Result};

#[tokio::test]
async fn a_lesson_is_read_with_its_course_its_module_and_its_own_details() -> Result<()> {
    let pool = pool().await?;
    let program = course(&pool).await?;
    let lesson = lesson_titled(&pool, &program, SPACING_LESSON).await?;
    let context = LessonContext::load(&pool, &program, &lesson).await?;
    assert_eq!(context.program_id, program);
    assert_eq!(context.lesson_id, lesson);
    assert_eq!(context.course, "A generic learning program");
    assert_eq!(context.goal, "Learn a subject");
    assert_eq!(context.prior_knowledge, "Some familiarity");
    assert_eq!(context.module, "Module 0");
    assert_eq!(context.module_summary, "A subject-neutral module");
    assert_eq!(context.outcomes, ["Describe a concept"]);
    assert_eq!(context.lesson, SPACING_LESSON);
    assert_eq!(
        context.objective,
        "Explain why spreading study over several sessions improves recall"
    );
    assert_eq!(context.estimated_minutes, 20);
    assert!(!context.prepared, "the fixture's lesson is an outline");
    assert!(context.teaching.is_empty());
    assert!(context.cited_versions.is_empty());
    Ok(())
}

#[tokio::test]
async fn a_prepared_lessons_teaching_is_its_explanation_example_contrast_and_recap_in_block_order(
) -> Result<()> {
    let pool = pool().await?;
    let program = course(&pool).await?;
    let lesson = lesson_titled(&pool, &program, SPACING_LESSON).await?;
    prepare_lesson(
        &pool,
        &lesson,
        &[
            ("explanation", "Explained.", &[]),
            ("check", "Checked?", &[]),
            ("worked_example", "Worked.", &[]),
            ("guided_practice", "Guided.", &[]),
            ("contrast", "Contrasted.", &[]),
            ("reflection", "Reflected?", &[]),
            ("recap", "Recapped.", &[]),
            ("independent_practice", "Practised.", &[]),
        ],
    )
    .await?;
    let context = LessonContext::load(&pool, &program, &lesson).await?;
    assert!(context.prepared);
    assert_eq!(
        context.teaching,
        ["Explained.", "Worked.", "Contrasted.", "Recapped."]
    );
    Ok(())
}

#[tokio::test]
async fn a_lesson_that_is_not_prepared_has_no_teaching_even_with_blocks() -> Result<()> {
    let pool = pool().await?;
    let program = course(&pool).await?;
    let lesson = lesson_titled(&pool, &program, SPACING_LESSON).await?;
    add_blocks(
        &pool,
        &lesson,
        &[
            ("explanation", "Explained.", &[]),
            ("recap", "Recapped.", &[]),
        ],
    )
    .await?;
    let context = LessonContext::load(&pool, &program, &lesson).await?;
    assert!(!context.prepared);
    assert!(context.teaching.is_empty());
    Ok(())
}

#[tokio::test]
async fn the_versions_the_blocks_cite_are_listed_once_each_in_block_order_from_blocks_of_every_kind(
) -> Result<()> {
    let pool = pool().await?;
    let program = course(&pool).await?;
    let lesson = lesson_titled(&pool, &program, SPACING_LESSON).await?;
    prepare_lesson(
        &pool,
        &lesson,
        &[
            ("check", "Checked?", &["v2"]),
            ("explanation", "Explained.", &["v1", "v2"]),
            ("reflection", "Reflected?", &["v3", "v1"]),
            ("recap", "Recapped.", &[]),
        ],
    )
    .await?;
    let context = LessonContext::load(&pool, &program, &lesson).await?;
    assert_eq!(context.cited_versions, ["v2", "v1", "v3"]);
    Ok(())
}

#[tokio::test]
async fn a_lesson_asked_for_under_another_course_is_invalid_input() -> Result<()> {
    let pool = pool().await?;
    let program = course(&pool).await?;
    let other = course(&pool).await?;
    let lesson = lesson_titled(&pool, &program, SPACING_LESSON).await?;
    let error = LessonContext::load(&pool, &other, &lesson)
        .await
        .unwrap_err();
    assert!(
        matches!(&error, AppError::InvalidInput(message) if message.contains("not one of the course's")),
        "{error:?}"
    );
    let missing = LessonContext::load(&pool, &program, "no-such-lesson")
        .await
        .unwrap_err();
    assert!(matches!(missing, AppError::InvalidInput(_)), "{missing:?}");
    Ok(())
}

#[tokio::test]
async fn the_query_is_the_lessons_title_and_objective_then_the_modules_title_and_outcomes(
) -> Result<()> {
    let pool = pool().await?;
    let program = course(&pool).await?;
    let lesson = lesson_titled(&pool, &program, SLEEP_LESSON).await?;
    let context = LessonContext::load(&pool, &program, &lesson).await?;
    assert_eq!(
        context.query(),
        "Sleep after learning\nExplain how sleep after learning helps memory\nModule 0\nDescribe a concept"
    );
    Ok(())
}

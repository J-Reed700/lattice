//! The published lesson's units line up with the report's.
use super::*;
use crate::assessment::assessment_tests::{placed_questions, PLACED_PROMPTS};
use crate::dto::AcceptLearningProgramRequestDto;

#[tokio::test]
async fn the_published_lessons_units_keep_each_questions_place() -> Result<()> {
    let pool = crate::tests::pool().await?;
    sqlx::migrate!("../lattice-desktop/migrations")
        .run(&pool)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;
    let program = crate::tests::fixture();
    let programs = LearningRepository::new(pool.clone());
    programs.create(&program).await?;
    programs
        .accept(&AcceptLearningProgramRequestDto {
            program_id: program.summary.id.clone(),
            expected_revision: 0,
            title: program.summary.title.clone(),
        })
        .await?;
    let program_id = program.summary.id.clone();
    let lesson_id = program.modules[0].lessons[0].id.clone();
    programs
        .prepare(
            &program_id,
            &lesson_id,
            1,
            &crate::tests::prepared_with_questions(&lesson_id, &placed_questions()),
        )
        .await?;
    let draft = published(&pool, &program_id, &lesson_id).await?;
    let sections = draft["blocks"].as_array().map_or(0, Vec::len);
    let units = units(&draft);
    let title = |unit: usize| units.get(unit).map(|u| u.title.as_str());
    assert_eq!(title(sections), Some(PLACED_PROMPTS[0]));
    assert_eq!(title(sections + 1), Some(PLACED_PROMPTS[1]));
    assert_eq!(title(sections + 2), Some(PLACED_PROMPTS[2]));
    Ok(())
}

//! Module tests over partly prepared modules: refusal, coverage and completion.
use super::{
    dto::*,
    repository::LearningRepository,
    tests::{fixture, pool, prepared, ready_program},
};
use lattice_core::shared::error::{AppError, Result};

async fn accepted(repo: &LearningRepository) -> Result<LearningProgramDto> {
    let p = fixture();
    repo.create(&p).await?;
    repo.accept(&AcceptLearningProgramRequestDto {
        program_id: p.summary.id.clone(),
        expected_revision: 0,
        title: p.summary.title.clone(),
    })
    .await?;
    repo.get(&p.summary.id).await
}

/// Answers every test question of the module correctly: a passing test.
fn passing_test(
    p: &LearningProgramDto,
    module: &LearningModuleDto,
) -> SubmitLearningAttemptRequestDto {
    SubmitLearningAttemptRequestDto {
        attempt_id: uuid::Uuid::new_v4().to_string(),
        program_id: p.summary.id.clone(),
        expected_revision: p.summary.revision,
        module_id: module.id.clone(),
        lesson_id: None,
        kind: LearningAssessmentKind::Test,
        answers: module
            .lessons
            .iter()
            .flat_map(|l| l.questions.iter())
            .filter(|q| q.kind == LearningAssessmentKind::Test)
            .map(|q| LearningAnswerDto {
                question_id: q.id.clone(),
                selected_index: 0,
            })
            .collect(),
    }
}

#[tokio::test]
async fn module_test_without_a_prepared_lesson_is_refused() -> Result<()> {
    let repo = LearningRepository::new(pool().await?);
    let p = accepted(&repo).await?;
    let refused = repo.submit(&passing_test(&p, &p.modules[0])).await;
    assert!(matches!(
        refused,
        Err(AppError::InvalidInput(message))
            if message == "Prepare at least one lesson in this module before taking its test"
    ));
    assert!(repo.get(&p.summary.id).await?.attempts.is_empty());
    Ok(())
}

#[tokio::test]
async fn partial_module_test_records_coverage_and_completes_nothing() -> Result<()> {
    let repo = LearningRepository::new(pool().await?);
    let p = accepted(&repo).await?;
    let first = &p.modules[0].lessons[0].id;
    repo.prepare(&p.summary.id, first, p.summary.revision, &prepared(first))
        .await?;
    let p = repo.get(&p.summary.id).await?;
    let module = &p.modules[0];
    let request = passing_test(&p, module);
    assert_eq!(request.answers.len(), 1);

    let attempt = repo.submit(&request).await?;
    assert_eq!(attempt.correct, attempt.total);
    let partial = Some(LearningModuleTestCoverageDto {
        covered_lessons: 1,
        module_lessons: 2,
    });
    assert_eq!(attempt.coverage, partial);

    let saved = repo.get(&p.summary.id).await?;
    assert_eq!(saved.attempts[0].coverage, partial);
    assert_eq!(saved.summary.completed_lessons, 0);
    assert!(saved.modules[0].lessons.iter().all(|l| !l.completed));
    assert_eq!(saved.summary.current_lesson_id, p.summary.current_lesson_id);
    let mut replay = request;
    replay.expected_revision = saved.summary.revision;
    assert_eq!(repo.submit(&replay).await?.coverage, partial);
    Ok(())
}

#[tokio::test]
async fn full_module_test_covers_every_lesson() -> Result<()> {
    let repo = LearningRepository::new(pool().await?);
    let p = ready_program(&repo).await?;
    let attempt = repo.submit(&passing_test(&p, &p.modules[0])).await?;
    assert_eq!(
        attempt.coverage,
        Some(LearningModuleTestCoverageDto {
            covered_lessons: 2,
            module_lessons: 2,
        })
    );
    assert_eq!(repo.get(&p.summary.id).await?.summary.completed_lessons, 0);
    Ok(())
}

#[tokio::test]
async fn lesson_checks_carry_no_module_coverage() -> Result<()> {
    let repo = LearningRepository::new(pool().await?);
    let p = ready_program(&repo).await?;
    let module = &p.modules[0];
    let lesson = &module.lessons[0];
    let quiz = lesson
        .questions
        .iter()
        .find(|q| q.kind == LearningAssessmentKind::Quiz)
        .expect("prepared lessons carry a quiz question");
    let attempt = repo
        .submit(&SubmitLearningAttemptRequestDto {
            attempt_id: uuid::Uuid::new_v4().to_string(),
            program_id: p.summary.id.clone(),
            expected_revision: p.summary.revision,
            module_id: module.id.clone(),
            lesson_id: Some(lesson.id.clone()),
            kind: LearningAssessmentKind::Quiz,
            answers: vec![LearningAnswerDto {
                question_id: quiz.id.clone(),
                selected_index: 0,
            }],
        })
        .await?;
    assert_eq!(attempt.coverage, None);
    Ok(())
}

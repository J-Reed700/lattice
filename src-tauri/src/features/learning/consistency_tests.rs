use super::{
    curriculum::LearningCurriculumOperation,
    curriculum_repository::LearningCurriculumRepository,
    dto::{CompleteLearningLessonRequestDto, LearningPreparation, LearningProgramStatus},
    plan_dto::{
        LearningCurriculumRevisionActionRequestDto, PreviewLearningCurriculumRevisionRequestDto,
    },
    repository::LearningRepository,
    tests::{fixture, pool},
};
use crate::shared::error::Result;

#[tokio::test]
async fn completed_lesson_does_not_prevent_editing_and_accepting_the_curriculum() -> Result<()> {
    let pool = pool().await?;
    let repo = LearningRepository::new(pool.clone());
    let curriculum = LearningCurriculumRepository::new(pool);
    let mut program = fixture();
    program.summary.status = LearningProgramStatus::Active;
    program.summary.revision = 1;
    program.modules[0].lessons[0].preparation = LearningPreparation::Ready;
    repo.create(&program).await?;
    let initial = curriculum.plan(&program.summary.id).await?;
    assert_eq!(initial.program_revision, 1);
    assert_eq!(initial.accepted_revision.unwrap().revision_number, 1);

    repo.complete(&CompleteLearningLessonRequestDto {
        program_id: program.summary.id.clone(),
        lesson_id: program.modules[0].lessons[0].id.clone(),
        expected_revision: initial.program_revision,
    })
    .await?;
    let plan = curriculum.plan(&program.summary.id).await?;
    assert_eq!(plan.program_revision, 2);
    assert_eq!(plan.accepted_revision.unwrap().revision_number, 1);
    let draft = curriculum
        .preview(&PreviewLearningCurriculumRevisionRequestDto {
            operation_id: uuid::Uuid::new_v4().to_string(),
            program_id: program.summary.id.clone(),
            expected_revision: plan.program_revision,
            reason: "Adapt upcoming work after completing a lesson".into(),
            operations: vec![LearningCurriculumOperation::EditLesson {
                lesson_id: program.modules[0].lessons[1].id.clone(),
                title: "Updated lesson".into(),
                objective: "Apply the evidence".into(),
                estimated_minutes: 25,
            }],
        })
        .await?;
    let accepted = curriculum
        .accept_revision(&LearningCurriculumRevisionActionRequestDto {
            operation_id: uuid::Uuid::new_v4().to_string(),
            program_id: program.summary.id.clone(),
            revision_id: draft.draft_revision.unwrap().id,
            expected_revision: draft.program_revision,
        })
        .await?;
    assert_eq!(accepted.program_revision, 3);
    assert_eq!(accepted.accepted_revision.unwrap().revision_number, 2);
    let updated = repo.get(&program.summary.id).await?;
    assert_eq!(updated.summary.revision, accepted.program_revision);
    assert!(updated.modules[0].lessons[0].completed);
    assert_eq!(updated.modules[0].lessons[1].title, "Updated lesson");
    Ok(())
}

#[tokio::test]
async fn program_read_keeps_header_and_children_in_the_same_snapshot() -> Result<()> {
    let pool = pool().await?;
    let repo = LearningRepository::new(pool.clone());
    let program = fixture();
    repo.create(&program).await?;
    // Queue the reader, then a writer. A pool read per query used to let the
    // writer commit between the program header and its lesson rows.
    let held = pool.acquire().await?;
    let mut read = Box::pin(repo.get(&program.summary.id));
    assert!(futures::poll!(&mut read).is_pending());
    let mut writer = Box::pin(pool.acquire());
    assert!(futures::poll!(&mut writer).is_pending());
    drop(held);
    let write = async {
        let mut connection = writer.await?;
        use sqlx::Connection;
        let mut tx = connection.begin().await?;
        sqlx::query("UPDATE learning_programs SET revision=revision+1 WHERE id=?")
            .bind(&program.summary.id)
            .execute(&mut *tx)
            .await?;
        sqlx::query("UPDATE learning_lessons SET completed=1 WHERE id=?")
            .bind(&program.modules[0].lessons[0].id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await
    };
    let (read, written) = tokio::join!(read, write);
    written?;
    let read = read?;
    assert_eq!(read.summary.revision, program.summary.revision);
    assert_eq!(read.summary.completed_lessons, 0);
    assert!(!read.modules[0].lessons[0].completed);
    let after = repo.get(&program.summary.id).await?;
    assert_eq!(after.summary.revision, program.summary.revision + 1);
    assert_eq!(after.summary.completed_lessons, 1);
    Ok(())
}

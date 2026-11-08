//! The lesson job the content verification tests run under, shared with lattice-desktop's live evals.

#![allow(clippy::indexing_slicing)]

use crate::{
    curriculum::LearningGenerationJobKind, curriculum_repository::LearningCurriculumRepository,
    dto::AcceptLearningProgramRequestDto, plan_dto::StartLearningGenerationJobRequestDto,
    repository::LearningRepository,
};
use lattice_core::shared::error::Result;
use lattice_core::shared::runtime::jobs::JobStore;
use serde_json::json;

pub async fn setup(
    pool: &sqlx::SqlitePool,
) -> Result<(LearningCurriculumRepository, String, String)> {
    let programs = LearningRepository::new(pool.clone());
    let program = crate::test_support::fixture();
    programs.create(&program).await?;
    programs
        .accept(&AcceptLearningProgramRequestDto {
            program_id: program.summary.id.clone(),
            expected_revision: 0,
            title: program.summary.title.clone(),
        })
        .await?;
    let program = programs.get(&program.summary.id).await?;
    let lesson_id = program.modules[0].lessons[0].id.clone();
    let repo = LearningCurriculumRepository::new(pool.clone());
    let job = repo
        .start_job(&StartLearningGenerationJobRequestDto {
            operation_id: uuid::Uuid::new_v4().to_string(),
            program_id: program.summary.id,
            expected_revision: program.summary.revision,
            kind: LearningGenerationJobKind::LessonPreparation,
            request_json: json!({"lessonIds":[lesson_id]}).to_string(),
            progress_total: 1,
        })
        .await?;
    assert!(JobStore::new(pool.clone()).claim(&job.id).await?.is_some());
    Ok((repo, job.id, lesson_id))
}

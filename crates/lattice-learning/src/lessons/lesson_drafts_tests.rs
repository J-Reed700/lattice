use super::*;
use serde_json::{json, Value};

/// A claimed lesson preparation job: its database, repository, id and lesson.
pub(crate) async fn lesson_job() -> Result<(
    Database,
    crate::curriculum_repository::LearningCurriculumRepository,
    String,
    String,
)> {
    use crate::{
        curriculum::LearningGenerationJobKind, curriculum_repository::LearningCurriculumRepository,
        dto::AcceptLearningProgramRequestDto, plan_dto::StartLearningGenerationJobRequestDto,
        repository::LearningRepository,
    };
    use lattice_core::shared::error::AppError;
    use lattice_core::shared::runtime::jobs::JobStore;
    let pool = crate::tests::pool().await?;
    let db = Database::new(pool.clone());
    let programs = LearningRepository::new(db.clone());
    let program = crate::tests::fixture();
    programs.create(&program).await?;
    programs
        .accept(&AcceptLearningProgramRequestDto {
            program_id: program.summary.id.clone(),
            expected_revision: 0,
            title: program.summary.title.clone(),
        })
        .await?;
    let program = programs.get(&program.summary.id).await?;
    let lesson = program
        .modules
        .first()
        .and_then(|module| module.lessons.first())
        .map(|lesson| lesson.id.clone())
        .ok_or_else(|| AppError::InternalError("fixture has no lesson".into()))?;
    let jobs = LearningCurriculumRepository::new(db.clone());
    let job = jobs
        .start_job(&StartLearningGenerationJobRequestDto {
            operation_id: uuid::Uuid::new_v4().to_string(),
            program_id: program.summary.id.clone(),
            expected_revision: program.summary.revision,
            kind: LearningGenerationJobKind::LessonPreparation,
            request_json: json!({ "lessonIds": [lesson] }).to_string(),
            progress_total: 1,
        })
        .await?;
    JobStore::new(db.clone()).claim(&job.id).await?;
    Ok((db.clone(), jobs, job.id, lesson))
}

#[tokio::test]
async fn a_draft_saved_in_other_whitespace_keeps_its_claim_inventory() -> Result<()> {
    let (database, jobs, job_id, lesson) = lesson_job().await?;
    let draft = r#"{"blocks":[{"body":"Unpublished lesson draft"}]}"#;
    let pretty = serde_json::to_string_pretty(&serde_json::from_str::<Value>(draft)?)?;
    run(&database, &jobs, &job_id, &lesson, async {
        resume("inputs".into()).await?;
        save(draft).await?;
        record_claim_inventory(draft, "audited-inventory".into()).await?;
        save(&pretty).await?;
        assert_eq!(
            claim_inventory(&pretty).await?.as_deref(),
            Some("audited-inventory")
        );
        Ok(())
    })
    .await
}

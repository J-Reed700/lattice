//! Tests for course practice profiles ("Preparing for:").

use super::*;
use crate::dto::*;
use crate::practice_generation::generate_written_practice_prompt;
use crate::practice_repository::LearningPracticeRepository;
use lattice_core::shared::error::{AppError, Result};

#[tokio::test]
async fn reading_a_course_default_empty_profile_when_no_row_exists() -> Result<()> {
    let pool = active_memory_pool().await?;
    let (program, _lesson) = setup(&pool).await?;
    let repo = LearningPracticeRepository::new(pool);

    let profile = repo.get_profile(&program.summary.id).await?;
    assert_eq!(profile.program_id, program.summary.id);
    assert_eq!(profile.details, "");
    assert_eq!(profile.language, None);
    assert_eq!(profile.interview_at, None);
    assert_eq!(profile.company, None);
    assert_eq!(profile.role, None);
    assert_eq!(profile.revision, 0);

    Ok(())
}

#[tokio::test]
async fn saving_profile_fields_and_incrementing_revision() -> Result<()> {
    let pool = active_memory_pool().await?;
    let (program, _lesson) = setup(&pool).await?;
    let repo = LearningPracticeRepository::new(pool);

    let request = SaveLearningPracticeProfileRequestDto {
        program_id: program.summary.id.clone(),
        details: "Practicing algorithms for technical interviews".into(),
        language: Some("python".into()),
        interview_at: Some(1732060800),
        company: Some("the team".into()),
        role: Some("Software Engineer".into()),
        expected_revision: Some(0),
    };

    let saved = repo.save_profile(&request).await?;
    assert_eq!(saved.program_id, program.summary.id);
    assert_eq!(
        saved.details,
        "Practicing algorithms for technical interviews"
    );
    assert_eq!(saved.language.as_deref(), Some("python"));
    assert_eq!(saved.interview_at, Some(1732060800));
    assert_eq!(saved.company.as_deref(), Some("the team"));
    assert_eq!(saved.role.as_deref(), Some("Software Engineer"));
    assert_eq!(saved.revision, 1);

    Ok(())
}

#[tokio::test]
async fn rejecting_stale_expected_revision_with_conflict_error() -> Result<()> {
    let pool = active_memory_pool().await?;
    let (program, _lesson) = setup(&pool).await?;
    let repo = LearningPracticeRepository::new(pool);

    let stale_request = SaveLearningPracticeProfileRequestDto {
        program_id: program.summary.id.clone(),
        details: "New details".into(),
        language: None,
        interview_at: None,
        company: None,
        role: None,
        expected_revision: Some(999),
    };

    let result = repo.save_profile(&stale_request).await;
    assert!(
        matches!(result, Err(AppError::Conflict(_))),
        "expected Conflict error on stale revision, got {result:?}"
    );

    Ok(())
}

#[tokio::test]
async fn feeding_profile_language_and_details_into_written_practice_generation() -> Result<()> {
    let profile = LearningPracticeProfileDto {
        program_id: id(),
        details: "Practicing algorithms for technical interviews".into(),
        language: Some("python".into()),
        interview_at: Some(1732060800),
        company: Some("the team".into()),
        role: Some("Software Engineer".into()),
        revision: 1,
    };

    let prompt = generate_written_practice_prompt("Explain hash tables", Some(&profile));
    assert!(
        prompt.contains("Practicing algorithms for technical interviews"),
        "written practice prompt must feed profile details"
    );
    assert!(
        prompt.contains("python"),
        "written practice prompt must feed profile language"
    );

    Ok(())
}

#[tokio::test]
async fn feeding_profile_language_into_drill_language_selection_when_no_override() -> Result<()> {
    let pool = active_memory_pool().await?;
    let (program, _lesson) = setup(&pool).await?;

    // Directly insert the profile into the database with language = "rust"
    sqlx::query(
        "INSERT INTO learning_practice_profiles (program_id, details, language, revision, updated_at) VALUES (?, ?, ?, ?, ?)"
    )
    .bind(&program.summary.id)
    .bind("Practicing systems programming")
    .bind("rust")
    .bind(1)
    .bind(1000)
    .execute(&pool)
    .await
    .map_err(db)?;

    let drills = crate::drills::review::course_drills(&pool, &program.summary.id, None).await?;
    assert_eq!(
        drills.language, "rust",
        "drill language selection must feed the profile's language when no explicit language override is passed"
    );

    Ok(())
}

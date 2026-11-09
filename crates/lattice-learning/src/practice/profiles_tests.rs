//! Tests for course practice profiles ("Preparing for:").

use super::*;
use crate::drills::lesson::LessonContext;
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
async fn reading_nonexistent_program_profile_returns_not_found() -> Result<()> {
    let pool = active_memory_pool().await?;
    let repo = LearningPracticeRepository::new(pool);

    let result = repo.get_profile(&id()).await;
    assert!(matches!(result, Err(AppError::NotFound(_))));

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

    // Verify persisted state reads back identically
    let read_back = repo.get_profile(&program.summary.id).await?;
    assert_eq!(read_back, saved);

    Ok(())
}

#[tokio::test]
async fn sequential_profile_saves_increment_revision_each_time() -> Result<()> {
    let pool = active_memory_pool().await?;
    let (program, _lesson) = setup(&pool).await?;
    let repo = LearningPracticeRepository::new(pool);

    let first_req = SaveLearningPracticeProfileRequestDto {
        program_id: program.summary.id.clone(),
        details: "First details".into(),
        language: Some("python".into()),
        interview_at: None,
        company: None,
        role: None,
        expected_revision: Some(0),
    };
    let first_saved = repo.save_profile(&first_req).await?;
    assert_eq!(first_saved.revision, 1);

    let second_req = SaveLearningPracticeProfileRequestDto {
        program_id: program.summary.id.clone(),
        details: "Updated details".into(),
        language: Some("rust".into()),
        interview_at: Some(1732060800),
        company: Some("New Team".into()),
        role: Some("Lead".into()),
        expected_revision: Some(1),
    };
    let second_saved = repo.save_profile(&second_req).await?;
    assert_eq!(second_saved.revision, 2);
    assert_eq!(second_saved.details, "Updated details");
    assert_eq!(second_saved.language.as_deref(), Some("rust"));

    // Stale revision 1 fails with Conflict
    let stale_req = SaveLearningPracticeProfileRequestDto {
        program_id: program.summary.id.clone(),
        details: "Stale update".into(),
        language: None,
        interview_at: None,
        company: None,
        role: None,
        expected_revision: Some(1),
    };
    let stale_res = repo.save_profile(&stale_req).await;
    assert!(matches!(stale_res, Err(AppError::Conflict(_))));

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
async fn written_practice_session_starts_with_contextualized_task_prompt_from_profile() -> Result<()>
{
    let pool = active_memory_pool().await?;
    let (program, lesson) = setup(&pool).await?;
    let repo = LearningPracticeRepository::new(pool);

    let save_req = SaveLearningPracticeProfileRequestDto {
        program_id: program.summary.id.clone(),
        details: "Specialized systems engineering".into(),
        language: Some("rust".into()),
        interview_at: None,
        company: None,
        role: None,
        expected_revision: Some(0),
    };
    repo.save_profile(&save_req).await?;

    let session_req = start_request(&program, &lesson);
    repo.start(&session_req, "test_op").await?;

    let session = repo.get_session(&session_req.session_id).await?;
    assert!(
        session
            .task_prompt
            .contains("Specialized systems engineering"),
        "practice session task_prompt must reflect profile details"
    );
    assert!(
        session.task_prompt.contains("rust"),
        "practice session task_prompt must reflect profile language"
    );

    Ok(())
}

#[tokio::test]
async fn feeding_profile_language_into_drill_language_selection_when_no_override() -> Result<()> {
    let pool = active_memory_pool().await?;
    let (program, _lesson) = setup(&pool).await?;
    let repo = LearningPracticeRepository::new(pool.clone());

    // Save profile with language = "rust"
    let save_req = SaveLearningPracticeProfileRequestDto {
        program_id: program.summary.id.clone(),
        details: "Practicing systems programming".into(),
        language: Some("rust".into()),
        interview_at: None,
        company: None,
        role: None,
        expected_revision: Some(0),
    };
    repo.save_profile(&save_req).await?;

    let drills = crate::drills::review::course_drills(&pool, &program.summary.id, None).await?;
    assert_eq!(
        drills.language, "rust",
        "drill language selection must feed the profile's language when no explicit language override is passed"
    );

    Ok(())
}

#[tokio::test]
async fn drill_lesson_brief_context_includes_non_empty_profile_details() -> Result<()> {
    let pool = active_memory_pool().await?;
    let (program, lesson) = setup(&pool).await?;
    let repo = LearningPracticeRepository::new(pool.clone());

    let save_req = SaveLearningPracticeProfileRequestDto {
        program_id: program.summary.id.clone(),
        details: "Preparing for cloud architecture review".into(),
        language: Some("go".into()),
        interview_at: None,
        company: None,
        role: None,
        expected_revision: Some(0),
    };
    repo.save_profile(&save_req).await?;

    let context = LessonContext::load(&pool, &program.summary.id, &lesson).await?;
    assert!(
        context
            .goal
            .contains("Preparing for cloud architecture review"),
        "LessonContext goal must include non-empty profile details"
    );

    Ok(())
}

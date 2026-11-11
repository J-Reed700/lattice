//! Tests for course practice profiles ("Preparing for").

use super::*;
use crate::recall::{
    study_dto::CreateInterviewStudyDeckRequestDto, study_repository::StudyRepository,
    study_service::create_interview_deck,
};
use lattice_core::shared::persistence::Database;

fn save_request(program_id: &str, revision: i64) -> SaveLearningPracticeProfileRequestDto {
    SaveLearningPracticeProfileRequestDto {
        program_id: program_id.into(),
        details: "Preparing for a backend interview".into(),
        language: None,
        interview_at: None,
        company: Some("a company".into()),
        role: Some("Backend engineer".into()),
        expected_revision: revision,
    }
}

#[tokio::test]
async fn a_course_with_no_saved_profile_reads_as_the_empty_one() -> Result<()> {
    let pool = active_memory_pool().await?;
    let (program, _lesson) = setup(&pool).await?;
    let repo = LearningPracticeRepository::new(pool);

    let profile = repo.get_profile(&program.summary.id).await?;
    assert_eq!(
        profile,
        LearningPracticeProfileDto {
            program_id: program.summary.id.clone(),
            details: String::new(),
            language: None,
            interview_at: None,
            company: None,
            role: None,
            revision: 0,
        }
    );
    Ok(())
}

#[tokio::test]
async fn a_profile_of_a_course_that_does_not_exist_is_not_found() -> Result<()> {
    let repo = LearningPracticeRepository::new(active_memory_pool().await?);
    assert!(matches!(
        repo.get_profile(&id()).await,
        Err(AppError::NotFound(_))
    ));
    assert!(matches!(
        repo.save_profile(&save_request(&id(), 0)).await,
        Err(AppError::NotFound(_))
    ));
    Ok(())
}

#[tokio::test]
async fn a_saved_profile_reads_back_and_each_save_raises_the_revision() -> Result<()> {
    let pool = active_memory_pool().await?;
    let (program, _lesson) = setup(&pool).await?;
    let repo = LearningPracticeRepository::new(pool);
    let program_id = program.summary.id.clone();

    let mut request = save_request(&program_id, 0);
    request.language = Some("python".into());
    let first = repo.save_profile(&request).await?;
    assert_eq!(first.revision, 1);
    assert_eq!(first.details, "Preparing for a backend interview");
    assert_eq!(first.language.as_deref(), Some("python"));
    assert_eq!(first.company.as_deref(), Some("a company"));
    assert_eq!(first.role.as_deref(), Some("Backend engineer"));
    assert_eq!(repo.get_profile(&program_id).await?, first);

    request.expected_revision = 1;
    request.details = "Preparing for a systems interview".into();
    request.language = None;
    let second = repo.save_profile(&request).await?;
    assert_eq!(second.revision, 2);
    assert_eq!(second.language, None);
    assert_eq!(repo.get_profile(&program_id).await?, second);
    Ok(())
}

#[tokio::test]
async fn a_stale_revision_is_a_conflict_that_reaches_the_frontend_as_conflict() -> Result<()> {
    let pool = active_memory_pool().await?;
    let (program, _lesson) = setup(&pool).await?;
    let repo = LearningPracticeRepository::new(pool);
    let program_id = program.summary.id.clone();

    repo.save_profile(&save_request(&program_id, 0)).await?;

    let mut stale = save_request(&program_id, 0);
    stale.details = "Written from an older read".into();
    let Err(error) = repo.save_profile(&stale).await else {
        return Err(AppError::InvalidState("a stale save went through".into()));
    };
    assert!(matches!(error, AppError::Conflict(_)), "{error:?}");

    let sent = serde_json::to_value(lattice_core::shared::ipc::ApiError::from(error))
        .map_err(|e| AppError::Serialization(e.to_string()))?;
    assert_eq!(sent["code"], "CONFLICT");

    let kept = repo.get_profile(&program_id).await?;
    assert_eq!(kept.details, "Preparing for a backend interview");
    assert_eq!(kept.revision, 1);
    Ok(())
}

#[tokio::test]
async fn a_new_profile_is_saved_only_from_revision_zero() -> Result<()> {
    let pool = active_memory_pool().await?;
    let (program, _lesson) = setup(&pool).await?;
    let repo = LearningPracticeRepository::new(pool);

    assert!(matches!(
        repo.save_profile(&save_request(&program.summary.id, 3))
            .await,
        Err(AppError::Conflict(_))
    ));
    assert_eq!(repo.get_profile(&program.summary.id).await?.revision, 0);
    Ok(())
}

#[tokio::test]
async fn profile_in_reads_what_was_saved_and_nothing_for_a_course_without_one() -> Result<()> {
    let pool = active_memory_pool().await?;
    let (program, _lesson) = setup(&pool).await?;
    let repo = LearningPracticeRepository::new(pool.clone());
    let program_id = program.summary.id.clone();

    let mut connection = pool.acquire().await.map_err(db)?;
    assert_eq!(
        crate::practice_repository::profile_in(&mut connection, &program_id).await?,
        None
    );
    drop(connection);

    let saved = repo.save_profile(&save_request(&program_id, 0)).await?;
    let mut connection = pool.acquire().await.map_err(db)?;
    assert_eq!(
        crate::practice_repository::profile_in(&mut connection, &program_id).await?,
        Some(saved)
    );
    Ok(())
}

/// Saves a ready interview activity of the course with a plan of two questions.
async fn save_interview(
    pool: &SqlitePool,
    program: &LearningProgramDto,
    lesson: &str,
) -> Result<String> {
    let activity_id = id();
    sqlx::query("INSERT INTO learning_practical_activities(id,program_id,lesson_id,kind,title,brief,status,practice_mode,allowed_aids_json,outcome_ids_json,source_version_ids_json,rubric_json,runtime_kind,generator_model,created_at,updated_at) VALUES(?,?,?,'interview','A loop','','ready','practice','[]','[]','[]','[]','none','model',0,0)")
        .bind(&activity_id)
        .bind(&program.summary.id)
        .bind(lesson)
        .execute(pool)
        .await
        .map_err(db)?;
    let plan = json!({
        "companyName": "A company",
        "targetRole": "Backend engineer",
        "roundKind": "full_loop",
        "companySummary": "Streaming pipelines.",
        "learnerRole": "Candidate",
        "counterpartRole": "Interviewer",
        "questions": [
            {"id": "q1", "round": "technical_deep_dive", "question": "How do you bound a queue?",
             "companyRationale": "Pipelines.", "followUpProbes": [], "evaluationSignals": ["Bounds it"]},
            {"id": "q2", "round": "behavioral", "question": "Tell me about a disagreement.",
             "companyRationale": "Teams.", "followUpProbes": [], "evaluationSignals": ["Owns it"]}
        ]
    });
    sqlx::query("INSERT INTO learning_practical_files(activity_id,ordinal,path,role,content,content_sha256,editable) VALUES(?,0,'interview/plan.json','reference',?,'sha',0)")
        .bind(&activity_id)
        .bind(plan.to_string())
        .execute(pool)
        .await
        .map_err(db)?;
    Ok(activity_id)
}

#[tokio::test]
async fn a_new_interview_date_becomes_the_target_of_the_courses_interview_decks() -> Result<()> {
    let pool = active_memory_pool().await?;
    let (program, lesson) = setup(&pool).await?;
    let repo = LearningPracticeRepository::new(pool.clone());
    let program_id = program.summary.id.clone();
    let activity_id = save_interview(&pool, &program, &lesson).await?;
    let study = StudyRepository::new(pool.clone());
    let deck = create_interview_deck(
        &study,
        &Database::from(pool.clone()),
        CreateInterviewStudyDeckRequestDto {
            program_id: program_id.clone(),
            activity_id,
            target_at: None,
        },
    )
    .await?;
    assert_eq!(deck.target_at, None);

    let day = 24 * 60 * 60 * 1000;
    let interview_at = chrono::Utc::now().timestamp_millis() + 3 * day;
    let mut request = save_request(&program_id, 0);
    request.interview_at = Some(interview_at);
    repo.save_profile(&request).await?;
    assert_eq!(study.get(&deck.id).await?.target_at, Some(interview_at));

    request.expected_revision = 1;
    request.interview_at = None;
    repo.save_profile(&request).await?;
    assert_eq!(study.get(&deck.id).await?.target_at, None);
    Ok(())
}

#[tokio::test]
async fn a_conflicting_save_leaves_the_decks_target_alone() -> Result<()> {
    let pool = active_memory_pool().await?;
    let (program, lesson) = setup(&pool).await?;
    let repo = LearningPracticeRepository::new(pool.clone());
    let program_id = program.summary.id.clone();
    let activity_id = save_interview(&pool, &program, &lesson).await?;
    let study = StudyRepository::new(pool.clone());
    let deck = create_interview_deck(
        &study,
        &Database::from(pool.clone()),
        CreateInterviewStudyDeckRequestDto {
            program_id: program_id.clone(),
            activity_id,
            target_at: None,
        },
    )
    .await?;

    let mut request = save_request(&program_id, 5);
    request.interview_at = Some(chrono::Utc::now().timestamp_millis() + 24 * 60 * 60 * 1000);
    assert!(matches!(
        repo.save_profile(&request).await,
        Err(AppError::Conflict(_))
    ));
    assert_eq!(study.get(&deck.id).await?.target_at, None);
    Ok(())
}

#[tokio::test]
async fn a_new_interview_date_with_no_interview_deck_is_just_saved() -> Result<()> {
    let pool = active_memory_pool().await?;
    let (program, _lesson) = setup(&pool).await?;
    let repo = LearningPracticeRepository::new(pool);

    let mut request = save_request(&program.summary.id, 0);
    request.interview_at = Some(chrono::Utc::now().timestamp_millis() + 24 * 60 * 60 * 1000);
    let saved = repo.save_profile(&request).await?;
    assert_eq!(saved.interview_at, request.interview_at);
    Ok(())
}

#[tokio::test]
async fn written_practice_starts_from_the_learners_details() -> Result<()> {
    let pool = active_memory_pool().await?;
    let (program, lesson) = setup(&pool).await?;
    let repo = LearningPracticeRepository::new(pool);

    let before = start_request(&program, &lesson);
    repo.start(&before, "before").await?;
    let plain = repo.get_session(&before.session_id).await?.task_prompt;
    assert!(!plain.contains("backend interview"));

    repo.save_profile(&save_request(&program.summary.id, 0))
        .await?;
    let after = start_request(&program, &lesson);
    repo.start(&after, "after").await?;
    let prompt = repo.get_session(&after.session_id).await?.task_prompt;
    assert!(prompt.starts_with(&plain), "{prompt}");
    assert!(
        prompt.contains("Preparing for a backend interview"),
        "{prompt}"
    );
    Ok(())
}

#[test]
fn the_fallback_task_speaks_to_the_learner_and_adds_their_details_as_a_last_line() {
    use crate::practice_generation::fallback_task_prompt;
    let plain = fallback_task_prompt("Explain hash maps", None);
    assert!(plain.starts_with("Explain hash maps"));
    assert_eq!(fallback_task_prompt("Explain hash maps", Some("  ")), plain);
    assert_eq!(fallback_task_prompt("Explain hash maps", Some("")), plain);
    let with = fallback_task_prompt("Explain hash maps", Some(" a backend interview "));
    assert_eq!(
        with,
        format!("{plain}\n\nYou are preparing for: a backend interview")
    );
}

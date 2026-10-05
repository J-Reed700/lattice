use crate::features::learning::{
    dto::*,
    practice_generation::validate_citations,
    practice_repository::{default_rubric, LearningPracticeRepository, SubmissionWrite},
    repository::LearningRepository,
};
use crate::shared::error::{AppError, Result};
use sqlx::{
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
    SqlitePool,
};

fn id() -> String {
    uuid::Uuid::new_v4().to_string()
}
fn db(error: sqlx::Error) -> AppError {
    AppError::Database(error.to_string())
}

async fn setup(pool: &SqlitePool) -> Result<(LearningProgramDto, String)> {
    sqlx::query("PRAGMA foreign_keys=ON")
        .execute(pool)
        .await
        .map_err(db)?;
    let repo = LearningRepository::new(pool.clone());
    let p = crate::features::learning::tests::fixture();
    repo.create(&p).await?;
    repo.accept(&AcceptLearningProgramRequestDto {
        program_id: p.summary.id.clone(),
        expected_revision: 0,
        title: p.summary.title.clone(),
    })
    .await?;
    sqlx::query("UPDATE learning_lessons SET preparation='ready' WHERE program_id=?")
        .bind(&p.summary.id)
        .execute(pool)
        .await
        .map_err(db)?;
    let ready = repo.get(&p.summary.id).await?;
    let lesson = ready
        .modules
        .first()
        .and_then(|m| m.lessons.first())
        .map(|l| l.id.clone())
        .ok_or_else(|| AppError::Database("fixture lesson missing".into()))?;
    Ok((ready, lesson))
}

async fn active_memory_pool() -> Result<SqlitePool> {
    crate::features::learning::tests::pool().await
}
async fn file_pool(path: std::path::PathBuf) -> Result<SqlitePool> {
    let options = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true)
        .foreign_keys(true);
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .map_err(db)?;
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;
    Ok(pool)
}

fn start_request(p: &LearningProgramDto, lesson: &str) -> StartLearningPracticeSessionRequestDto {
    StartLearningPracticeSessionRequestDto {
        operation_id: id(),
        session_id: id(),
        program_id: p.summary.id.clone(),
        lesson_id: lesson.into(),
        expected_program_revision: p.summary.revision,
        mode: LearningPracticeMode::Practice,
        task_kind: LearningPracticeTaskKind::Independent,
        revises_session_id: None,
    }
}
fn save_request(
    program_id: &str,
    session_id: &str,
    operation_id: &str,
    expected_revision: i64,
    text: &str,
) -> SaveLearningPracticeArtifactRequestDto {
    SaveLearningPracticeArtifactRequestDto {
        operation_id: operation_id.into(),
        program_id: program_id.into(),
        session_id: session_id.into(),
        expected_revision,
        text: text.into(),
    }
}

#[tokio::test]
async fn attempt_session_and_artifact_survive_file_backed_reopen() -> Result<()> {
    let dir = tempfile::tempdir().map_err(|e| AppError::Database(e.to_string()))?;
    let path = dir.path().join("practice.sqlite");
    let pool = file_pool(path.clone()).await?;
    let (p, lesson) = setup(&pool).await?;
    let repo = LearningPracticeRepository::new(pool.clone());
    let start = start_request(&p, &lesson);
    repo.start(&start, "start-hash").await?;
    let save = save_request(
        &p.summary.id,
        &start.session_id,
        &id(),
        0,
        "I can explain the idea with a new example.",
    );
    repo.save_artifact(&save, "save-hash").await?;
    pool.close().await;
    let reopened = file_pool(path).await?;
    let session = LearningPracticeRepository::new(reopened.clone())
        .get_session(&start.session_id)
        .await?;
    assert_eq!(
        session.artifact.text,
        "I can explain the idea with a new example."
    );
    assert_eq!(session.artifact.revision, 1);
    assert_eq!(session.summary.source_version_ids, Vec::<String>::new());
    Ok(())
}

#[tokio::test]
async fn artifact_replay_is_idempotent_payload_checked_and_cas_protected() -> Result<()> {
    let pool = active_memory_pool().await?;
    let (p, lesson) = setup(&pool).await?;
    let repo = LearningPracticeRepository::new(pool);
    let start = start_request(&p, &lesson);
    repo.start(&start, "start").await?;
    let op = id();
    let original = save_request(&p.summary.id, &start.session_id, &op, 0, "durable answer");
    repo.save_artifact(&original, "payload-a").await?;
    let mut retry = original.clone();
    retry.expected_revision = 99;
    let replay = repo.save_artifact(&retry, "payload-a").await?;
    assert_eq!(replay.sessions.first().map(|s| s.revision), Some(1));
    assert!(repo
        .save_artifact(&original, "payload-different")
        .await
        .is_err());
    let stale = save_request(&p.summary.id, &start.session_id, &id(), 0, "stale answer");
    assert!(repo.save_artifact(&stale, "stale").await.is_err());
    assert_eq!(
        repo.get_session(&start.session_id).await?.artifact.text,
        "durable answer"
    );
    Ok(())
}

#[tokio::test]
async fn demonstrate_mode_blocks_all_selected_aids_and_session_stays_frozen() -> Result<()> {
    let pool = active_memory_pool().await?;
    let (p, lesson) = setup(&pool).await?;
    let repo = LearningPracticeRepository::new(pool);
    let mut start = start_request(&p, &lesson);
    start.mode = LearningPracticeMode::Demonstrate;
    repo.start(&start, "start").await?;
    for _aid in ["source_open", "tutor", "solution"] {
        assert!(repo
            .validate_live_session(&p.summary.id, &start.session_id, 0, false)
            .await
            .is_err());
    }
    let open = OpenLearningPracticeSourceRequestDto {
        operation_id: id(),
        program_id: p.summary.id.clone(),
        session_id: start.session_id.clone(),
        expected_revision: 0,
        source_id: id(),
        version_id: id(),
    };
    assert!(repo.open_source(&open, "open").await.is_err());
    let before = repo.get_session(&start.session_id).await?;
    assert_eq!(before.summary.mode, LearningPracticeMode::Demonstrate);
    assert_eq!(before.summary.revision, 0);
    Ok(())
}

#[tokio::test]
async fn submitted_result_is_an_immutable_artifact_snapshot() -> Result<()> {
    let pool = active_memory_pool().await?;
    let (p, lesson) = setup(&pool).await?;
    let repo = LearningPracticeRepository::new(pool);
    let start = start_request(&p, &lesson);
    repo.start(&start, "start").await?;
    let save = save_request(
        &p.summary.id,
        &start.session_id,
        &id(),
        0,
        "submitted wording",
    );
    repo.save_artifact(&save, "save").await?;
    let session = repo.get_session(&start.session_id).await?;
    let criteria = session
        .rubric
        .iter()
        .map(|c| LearningPracticeCriterionResultDto {
            criterion_id: c.id.clone(),
            dimension: c.dimension.clone(),
            score: None,
            max_points: c.max_points,
            observation: "Insufficient evidence for a reliable score.".into(),
            evidence_quote: None,
        })
        .collect::<Vec<_>>();
    let evidence = session
        .rubric
        .iter()
        .map(|c| LearningPracticeEvidenceEventDto {
            dimension: c.dimension.clone(),
            observed: false,
            observation: "Insufficient evidence.".into(),
            evidence_quote: None,
            assistance_kinds: vec![],
        })
        .collect();
    let req = SubmitLearningPracticeAttemptRequestDto {
        operation_id: id(),
        program_id: p.summary.id.clone(),
        session_id: start.session_id.clone(),
        expected_revision: 1,
    };
    repo.submit(
        &req,
        SubmissionWrite {
            payload_hash: "submit".into(),
            operation_id: req.operation_id.clone(),
            grade_status: LearningPracticeGradeStatus::Uncertain,
            criteria,
            evidence,
            grader_model: "fixture-model".into(),
        },
        &session.rubric,
    )
    .await?;
    assert!(repo
        .save_artifact(
            &save_request(&p.summary.id, &start.session_id, &id(), 2, "rewrite"),
            "rewrite"
        )
        .await
        .is_err());
    let after = repo.get_session(&start.session_id).await?;
    let result = after
        .result
        .ok_or_else(|| AppError::Database("missing submitted result".into()))?;
    assert_eq!(result.artifact_text, "submitted wording");
    assert_eq!(result.grade_status, LearningPracticeGradeStatus::Uncertain);
    assert_eq!(result.evidence.len(), 4);
    let revision = StartLearningPracticeSessionRequestDto {
        operation_id: id(),
        session_id: id(),
        program_id: p.summary.id.clone(),
        lesson_id: lesson.clone(),
        expected_program_revision: p.summary.revision,
        mode: LearningPracticeMode::Practice,
        task_kind: LearningPracticeTaskKind::Independent,
        revises_session_id: Some(start.session_id.clone()),
    };
    repo.start(&revision, "revision-start").await?;
    let revised = repo.get_session(&revision.session_id).await?;
    assert_eq!(revised.artifact.text, "submitted wording");
    assert_eq!(
        revised.summary.revises_session_id.as_deref(),
        Some(start.session_id.as_str())
    );
    assert_eq!(revised.assistance.len(), 1);
    assert!(revised.assistance[0]
        .details
        .get("previousFeedback")
        .is_some());
    assert_eq!(
        repo.get_session(&start.session_id).await?.summary.status,
        LearningPracticeSessionStatus::Submitted
    );
    assert!(repo
        .change_mode(
            &ChangeLearningPracticeModeRequestDto {
                operation_id: id(),
                program_id: p.summary.id,
                session_id: revision.session_id,
                expected_revision: 0,
                mode: LearningPracticeMode::Demonstrate
            },
            "revision-mode"
        )
        .await
        .is_err());
    Ok(())
}

#[test]
fn citations_must_be_exact_quotes_from_the_frozen_owned_version() {
    let source = LearningSourceVersionDto {
        source_id: "logical-source".into(),
        version: LearningSourceVersionSummaryDto {
            id: "version-1".into(),
            version_number: 1,
            title: "Stored source".into(),
            publisher: None,
            resolved_url: None,
            excerpt: "café passage".into(),
            content_sha256: "0".repeat(64),
            word_count: 2,
            truncated: false,
            extraction_version: "test".into(),
            acquired_at: 1,
        },
        full_text: "The café passage supports this.".into(),
        usage: vec![],
    };
    let valid = LearningPracticeCitationDto {
        source_id: "logical-source".into(),
        version_id: "version-1".into(),
        quote: "café passage".into(),
    };
    assert!(
        validate_citations(std::slice::from_ref(&valid), std::slice::from_ref(&source)).is_ok()
    );
    let wrong = LearningPracticeCitationDto {
        quote: "cafe passage".into(),
        ..valid.clone()
    };
    assert!(validate_citations(&[wrong], std::slice::from_ref(&source)).is_err());
    let cross = LearningPracticeCitationDto {
        version_id: "new-version".into(),
        ..valid
    };
    assert!(validate_citations(&[cross], std::slice::from_ref(&source)).is_err());
    assert_eq!(default_rubric().len(), 4);
}

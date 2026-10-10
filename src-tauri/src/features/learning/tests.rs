use super::{dto::*, repository::LearningRepository};
use crate::shared::error::Result;
use sqlx::{
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
    SqlitePool,
};

pub(super) async fn pool() -> Result<SqlitePool> {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        // A long model call must not retire the only in-memory database connection.
        .idle_timeout(None)
        .max_lifetime(None)
        .connect(":memory:")
        .await
        .map_err(|e| crate::shared::error::AppError::Database(e.to_string()))?;
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .map_err(|e| crate::shared::error::AppError::Database(e.to_string()))?;
    Ok(pool)
}
fn id() -> String {
    uuid::Uuid::new_v4().to_string()
}
pub(super) fn fixture() -> LearningProgramDto {
    let mut modules = Vec::new();
    for mi in 0..2 {
        let mid = id();
        let mut lessons = Vec::new();
        for li in 0..2 {
            lessons.push(LearningLessonDto {
                id: id(),
                title: format!("Lesson {mi}-{li}"),
                objective: "Explain the central idea".into(),
                estimated_minutes: 20,
                preparation: LearningPreparation::Outline,
                blocks: vec![],
                questions: vec![],
                completed: false,
            });
        }
        modules.push(LearningModuleDto {
            prerequisite_module_ids: vec![],
            project: None,
            id: mid,
            title: format!("Module {mi}"),
            summary: "A subject-neutral module".into(),
            outcomes: vec!["Describe a concept".into()],
            lessons,
        });
    }
    let current = modules[0].lessons[0].id.clone();
    LearningProgramDto {
        outline_review: None,
        summary: LearningProgramSummaryDto {
            id: id(),
            title: "A generic learning program".into(),
            goal: "Learn a subject".into(),
            status: LearningProgramStatus::Draft,
            revision: 0,
            module_count: 2,
            lesson_count: 4,
            completed_lessons: 0,
            current_lesson_id: Some(current),
            created_at: chrono::Utc::now().timestamp_millis(),
        },
        prior_knowledge: "Some familiarity".into(),
        minutes_per_session: 30,
        model_name: "fixture-model".into(),
        modules,
        sources: vec![LearningSourceDto {
            id: id(),
            title: "A selected source".into(),
            url: None,
            excerpt: "A bounded passage used by this generic course.".into(),
            acquired_at: 1,
        }],
        attempts: vec![],
    }
}
fn prepared(lesson_id: &str) -> PreparedLearningLesson {
    let mut questions = Vec::new();
    let mut keys = Vec::new();
    for kind in [
        LearningAssessmentKind::Practice,
        LearningAssessmentKind::Quiz,
        LearningAssessmentKind::Test,
    ] {
        let qid = id();
        questions.push(LearningQuestionDto {
            id: qid.clone(),
            kind,
            prompt: "Which option is supported?".into(),
            options: vec!["Supported".into(), "Unsupported".into()],
            source_ids: vec![],
        });
        keys.push(LearningAnswerKey {
            question_id: qid,
            correct_index: 0,
            explanation: "The source supports the first option.".into(),
        });
    }
    let mut prepared = PreparedLearningLesson {
        verification: None,
        blocks: vec![LearningBlockDto {
            rubric: vec![],
            kind: LearningBlockKind::Explanation,
            title: "Core idea".into(),
            body: "A concise explanation.".into(),
            source_ids: vec![],
        }],
        questions,
        keys,
    };
    prepared.verification = Some(super::content_verification::tests::attest(
        lesson_id, &prepared,
    ));
    prepared
}

async fn ready_program(repo: &LearningRepository) -> Result<LearningProgramDto> {
    let mut p = fixture();
    repo.create(&p).await?;
    repo.accept(&AcceptLearningProgramRequestDto {
        program_id: p.summary.id.clone(),
        expected_revision: 0,
        title: p.summary.title.clone(),
    })
    .await?;
    p = repo.get(&p.summary.id).await?;
    for m in &p.modules {
        for l in &m.lessons {
            let d = repo.get(&p.summary.id).await?;
            repo.prepare(&p.summary.id, &l.id, d.summary.revision, &prepared(&l.id))
                .await?;
        }
    }
    repo.get(&p.summary.id).await
}

#[tokio::test]
async fn memory_links_are_canonical_isolated_and_survive_program_deletion() -> Result<()> {
    let pool = pool().await?;
    sqlx::query("PRAGMA foreign_keys=ON")
        .execute(&pool)
        .await
        .map_err(|e| crate::shared::error::AppError::Database(e.to_string()))?;
    let repo = LearningRepository::new(pool.clone());
    let program = ready_program(&repo).await?;
    let lesson = &program.modules[0].lessons[0];
    let source_id = program.sources[0].id.clone();
    sqlx::query("UPDATE learning_blocks SET source_ids_json=? WHERE lesson_id=? AND ordinal=0")
        .bind(
            serde_json::to_string(&vec![source_id.clone()])
                .map_err(|e| crate::shared::error::AppError::Serialization(e.to_string()))?,
        )
        .bind(&lesson.id)
        .execute(&pool)
        .await
        .map_err(|e| crate::shared::error::AppError::Database(e.to_string()))?;
    let (_, created) = repo
        .ensure_lesson_note(
            &program.summary.id,
            &lesson.id,
            "Lesson notes",
            "## Sources\n",
        )
        .await?;
    assert!(created);
    let (note_id, created_again) = repo
        .ensure_lesson_note(&program.summary.id, &lesson.id, "Should not replace", "")
        .await?;
    assert!(!created_again);
    let note_count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM daily_notes_workspace WHERE id=?")
            .bind(&note_id)
            .fetch_one(&pool)
            .await
            .map_err(|e| crate::shared::error::AppError::Database(e.to_string()))?;
    assert_eq!(note_count, 1);

    let generated = super::generation::GeneratedLearningCardDraft {
        question: "What does the source explain?".into(),
        answer: "A stable record supports later comparison.".into(),
        explanation: "This is grounded in the selected excerpt.".into(),
        source_ids: vec![source_id.clone()],
    };
    repo.save_generated_drafts(&program.summary.id, &lesson.id, &[generated])
        .await?;
    let generated_id: String = sqlx::query_scalar(
        "SELECT id FROM learning_card_drafts WHERE program_id=? AND origin='generated'",
    )
    .bind(&program.summary.id)
    .fetch_one(&pool)
    .await
    .map_err(|e| crate::shared::error::AppError::Database(e.to_string()))?;
    let cleared = SaveLearningCardDraftRequestDto {
        draft_id: Some(generated_id.clone()),
        program_id: program.summary.id.clone(),
        lesson_id: lesson.id.clone(),
        question: "Edited question".into(),
        answer: "Edited answer".into(),
        explanation: "Edited explanation".into(),
        source_ids: vec![],
    };
    assert!(repo.save_manual_draft(&cleared).await.is_err());
    let other_source = id();
    sqlx::query("INSERT INTO learning_sources(id,program_id,title,url,excerpt,acquired_at) VALUES(?,?,?,NULL,?,1)")
        .bind(&other_source).bind(&program.summary.id).bind("Different source").bind("Not referenced by this lesson")
        .execute(&pool).await.map_err(|e| crate::shared::error::AppError::Database(e.to_string()))?;
    let swapped = SaveLearningCardDraftRequestDto {
        source_ids: vec![other_source],
        ..cleared
    };
    assert!(repo.save_manual_draft(&swapped).await.is_err());

    let manual = SaveLearningCardDraftRequestDto {
        draft_id: None,
        program_id: program.summary.id.clone(),
        lesson_id: lesson.id.clone(),
        question: "My mnemonic?".into(),
        answer: "A personal reminder.".into(),
        explanation: "Written by me.".into(),
        source_ids: vec![],
    };
    let manual_id = repo.save_manual_draft(&manual).await?;
    let card_id = repo
        .accept_card_draft(&program.summary.id, &manual_id)
        .await?;
    assert_eq!(
        repo.accept_card_draft(&program.summary.id, &manual_id)
            .await?,
        card_id
    );
    let study =
        crate::features::learning::recall::study_repository::StudyRepository::new(pool.clone());
    let state = repo.memory_state(&program.summary.id).await?;
    let deck = study
        .get(state.deck_id.as_deref().ok_or_else(|| {
            crate::shared::error::AppError::InvalidState("missing Learning deck".into())
        })?)
        .await?;
    let card = deck
        .cards
        .iter()
        .find(|c| c.id == card_id)
        .ok_or_else(|| crate::shared::error::AppError::NotFound("accepted card".into()))?;
    assert_eq!(
        card.format,
        crate::features::learning::recall::study_dto::StudyCardFormat::QuestionAnswer
    );
    assert!(card.options.is_empty());
    let reviewed = study
        .review(
            &crate::features::learning::recall::study_dto::ReviewStudyCardRequestDto {
                review_id: id(),
                card_id: card_id.clone(),
                expected_reviews: 0,
                selected_option: None,
                rating: crate::features::learning::recall::study_dto::StudyRating::Good,
            },
            chrono::Utc::now().timestamp_millis(),
        )
        .await?;
    assert_eq!(reviewed.review_count, 1);

    let original_journal_id = state.journal_id.ok_or_else(|| {
        crate::shared::error::AppError::InvalidState("missing Learning notebook".into())
    })?;
    study.delete_deck(&deck.id).await?;
    assert!(repo
        .memory_state(&program.summary.id)
        .await?
        .deck_id
        .is_none());
    let notes_repo =
        crate::features::daily_notes::repository::DailyNotesRepository::new(pool.clone());
    notes_repo.delete(&note_id).await?;
    sqlx::query("DELETE FROM journals WHERE id=?")
        .bind(&original_journal_id)
        .execute(&pool)
        .await
        .map_err(|e| crate::shared::error::AppError::Database(e.to_string()))?;
    let detached = repo.memory_state(&program.summary.id).await?;
    assert!(detached.journal_id.is_none());
    assert!(detached.lesson_notes.is_empty());
    let (_, created_again) = repo
        .ensure_lesson_note(
            &program.summary.id,
            &lesson.id,
            "Lesson notes",
            "## Recreated\n",
        )
        .await?;
    assert!(created_again);
    let preserved_state = repo.memory_state(&program.summary.id).await?;
    let journal_id = preserved_state.journal_id.ok_or_else(|| {
        crate::shared::error::AppError::InvalidState("missing recreated notebook".into())
    })?;
    let deck_id = preserved_state.deck_id.ok_or_else(|| {
        crate::shared::error::AppError::InvalidState("missing recreated deck".into())
    })?;
    let recreated_note_id = preserved_state
        .lesson_notes
        .first()
        .map(|(_, id)| id.clone())
        .ok_or_else(|| {
            crate::shared::error::AppError::InvalidState("missing recreated note".into())
        })?;
    repo.delete(&program.summary.id).await?;
    assert!(study.get(&deck_id).await.is_ok());
    assert!(
        crate::features::daily_notes::repository::DailyNotesRepository::new(pool.clone())
            .get(&recreated_note_id)
            .await
            .is_ok()
    );
    let journal_count: i64 = sqlx::query_scalar("SELECT count(*) FROM journals WHERE id=?")
        .bind(journal_id)
        .fetch_one(&pool)
        .await
        .map_err(|e| crate::shared::error::AppError::Database(e.to_string()))?;
    assert_eq!(journal_count, 1);
    Ok(())
}

#[tokio::test]
async fn notebook_and_lesson_links_survive_database_pool_restart() -> Result<()> {
    let dir =
        tempfile::tempdir().map_err(|e| crate::shared::error::AppError::Database(e.to_string()))?;
    let path = dir.path().join("learning-memory.db");
    let options = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true);
    let first = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options.clone())
        .await
        .map_err(|e| crate::shared::error::AppError::Database(e.to_string()))?;
    sqlx::migrate!("./migrations")
        .run(&first)
        .await
        .map_err(|e| crate::shared::error::AppError::Database(e.to_string()))?;
    let repo = LearningRepository::new(first.clone());
    let mut program = fixture();
    repo.create(&program).await?;
    repo.accept(&AcceptLearningProgramRequestDto {
        program_id: program.summary.id.clone(),
        expected_revision: 0,
        title: program.summary.title.clone(),
    })
    .await?;
    program = repo.get(&program.summary.id).await?;
    let lesson_id = program.modules[0].lessons[0].id.clone();
    let (note_id, _) = repo
        .ensure_lesson_note(&program.summary.id, &lesson_id, "Notes", "## Reflection\n")
        .await?;
    let before = repo.memory_state(&program.summary.id).await?;
    first.close().await;
    let second = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .map_err(|e| crate::shared::error::AppError::Database(e.to_string()))?;
    let restarted = LearningRepository::new(second.clone())
        .memory_state(&program.summary.id)
        .await?;
    assert_eq!(restarted.journal_id, before.journal_id);
    assert_eq!(restarted.deck_id, before.deck_id);
    assert_eq!(restarted.lesson_notes, vec![(lesson_id, note_id)]);
    second.close().await;
    Ok(())
}

#[tokio::test]
async fn memory_resources_and_lesson_ids_are_isolated_between_programs() -> Result<()> {
    let pool = pool().await?;
    let repo = LearningRepository::new(pool.clone());
    let first = ready_program(&repo).await?;
    let second = ready_program(&repo).await?;
    let first_lesson = first.modules[0].lessons[0].id.clone();
    let other_lesson = second.modules[0].lessons[0].id.clone();
    assert!(repo
        .ensure_lesson_note(&first.summary.id, &other_lesson, "Invalid", "")
        .await
        .is_err());
    repo.ensure_lesson_note(&first.summary.id, &first_lesson, "First program", "Notes")
        .await?;
    let second_state = repo.memory_state(&second.summary.id).await?;
    assert!(second_state.journal_id.is_none());
    assert!(second_state.deck_id.is_none());
    assert!(second_state.lesson_notes.is_empty());
    let cross_program = SaveLearningCardDraftRequestDto {
        draft_id: None,
        program_id: first.summary.id.clone(),
        lesson_id: other_lesson,
        question: "A question".into(),
        answer: "An answer".into(),
        explanation: "An explanation".into(),
        source_ids: vec![],
    };
    assert!(repo.save_manual_draft(&cross_program).await.is_err());
    let drafts: i64 =
        sqlx::query_scalar("SELECT count(*) FROM learning_card_drafts WHERE program_id=?")
            .bind(&first.summary.id)
            .fetch_one(&pool)
            .await
            .map_err(|e| crate::shared::error::AppError::Database(e.to_string()))?;
    assert_eq!(drafts, 0);
    Ok(())
}

#[tokio::test]
async fn normalized_course_survives_pool_restart_and_hides_answer_keys() -> Result<()> {
    let path = std::env::temp_dir().join(format!("learning-{}.db", id()));
    let options = SqliteConnectOptions::new()
        .filename(&path)
        .create_if_missing(true);
    let first = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options.clone())
        .await
        .map_err(|e| crate::shared::error::AppError::Database(e.to_string()))?;
    sqlx::migrate!("./migrations")
        .run(&first)
        .await
        .map_err(|e| crate::shared::error::AppError::Database(e.to_string()))?;
    let repo = LearningRepository::new(first.clone());
    let p = ready_program(&repo).await?;
    first.close().await;
    let second = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .map_err(|e| crate::shared::error::AppError::Database(e.to_string()))?;
    let saved = LearningRepository::new(second.clone())
        .get(&p.summary.id)
        .await?;
    assert_eq!(saved.modules.len(), 2);
    assert_eq!(
        saved.modules[0].lessons[0].preparation,
        LearningPreparation::Ready
    );
    let wire = serde_json::to_string(&saved).unwrap();
    assert!(!wire.contains("correctIndex"));
    assert!(!wire.contains("explanation\":\"The source supports"));
    second.close().await;
    let _ = std::fs::remove_file(path);
    Ok(())
}

#[tokio::test]
async fn revision_conflicts_and_attempt_validation_are_transactional() -> Result<()> {
    let pool = pool().await?;
    let repo = LearningRepository::new(pool.clone());
    let p = ready_program(&repo).await?;
    let module = &p.modules[0];
    let lesson = &module.lessons[0];
    let q = lesson
        .questions
        .iter()
        .find(|q| q.kind == LearningAssessmentKind::Practice)
        .unwrap();
    let revision = p.summary.revision;
    let conflict = repo
        .complete(&CompleteLearningLessonRequestDto {
            program_id: p.summary.id.clone(),
            lesson_id: lesson.id.clone(),
            expected_revision: revision - 1,
        })
        .await;
    assert!(conflict.is_err());
    let base = SubmitLearningAttemptRequestDto {
        attempt_id: id(),
        program_id: p.summary.id.clone(),
        expected_revision: revision,
        module_id: module.id.clone(),
        lesson_id: Some(lesson.id.clone()),
        kind: LearningAssessmentKind::Practice,
        answers: vec![LearningAnswerDto {
            question_id: q.id.clone(),
            selected_index: 0,
        }],
    };
    let mut partial = base.clone();
    partial.answers.clear();
    assert!(repo.submit(&partial).await.is_err());
    let mut duplicate = base.clone();
    duplicate.answers.push(duplicate.answers[0].clone());
    assert!(repo.submit(&duplicate).await.is_err());
    let mut out_of_bounds = base.clone();
    out_of_bounds.answers[0].selected_index = 2;
    assert!(repo.submit(&out_of_bounds).await.is_err());
    let submitted = repo.submit(&base).await?;
    assert_eq!(submitted.total, 1);
    assert_eq!(submitted.correct, 1);
    let retry = repo.submit(&base).await?;
    assert_eq!(retry.id, submitted.id);
    assert_eq!(
        serde_json::to_value(&retry.results).unwrap(),
        serde_json::to_value(&submitted.results).unwrap()
    );
    let mut replay_after_refetch = base.clone();
    replay_after_refetch.expected_revision += 17;
    assert_eq!(repo.submit(&replay_after_refetch).await?.id, submitted.id);
    let mut changed = base.clone();
    changed.answers[0].selected_index = 1;
    assert!(repo.submit(&changed).await.is_err());
    let saved = repo.get(&p.summary.id).await?;
    assert_eq!(saved.attempts.len(), 1);
    Ok(())
}

#[tokio::test]
async fn module_test_requires_every_ready_lesson_question() -> Result<()> {
    let pool = pool().await?;
    let repo = LearningRepository::new(pool);
    let p = ready_program(&repo).await?;
    let module = &p.modules[0];
    let test_questions: Vec<_> = module
        .lessons
        .iter()
        .flat_map(|l| l.questions.iter())
        .filter(|q| q.kind == LearningAssessmentKind::Test)
        .collect();
    assert_eq!(test_questions.len(), 2);
    let req = SubmitLearningAttemptRequestDto {
        attempt_id: id(),
        program_id: p.summary.id.clone(),
        expected_revision: p.summary.revision,
        module_id: module.id.clone(),
        lesson_id: None,
        kind: LearningAssessmentKind::Test,
        answers: vec![LearningAnswerDto {
            question_id: test_questions[0].id.clone(),
            selected_index: 0,
        }],
    };
    assert!(repo.submit(&req).await.is_err());
    let mut complete = req.clone();
    complete.answers = test_questions
        .iter()
        .map(|q| LearningAnswerDto {
            question_id: q.id.clone(),
            selected_index: 0,
        })
        .collect();
    let submitted = repo.submit(&complete).await?;
    complete.answers.reverse();
    complete.expected_revision += 1;
    assert_eq!(repo.submit(&complete).await?.id, submitted.id);
    Ok(())
}

#[tokio::test]
async fn module_test_rejects_a_partially_prepared_module() -> Result<()> {
    let pool = pool().await?;
    let repo = LearningRepository::new(pool);
    let p = fixture();
    let program_id = p.summary.id.clone();
    let module = p.modules[0].clone();
    repo.create(&p).await?;
    repo.accept(&AcceptLearningProgramRequestDto {
        program_id: program_id.clone(),
        expected_revision: 0,
        title: p.summary.title,
    })
    .await?;
    repo.prepare(
        &program_id,
        &module.lessons[0].id,
        1,
        &prepared(&module.lessons[0].id),
    )
    .await?;
    let test = repo.get(&program_id).await?;
    let question = &test.modules[0].lessons[0]
        .questions
        .iter()
        .find(|q| q.kind == LearningAssessmentKind::Test)
        .unwrap()
        .id;
    let req = SubmitLearningAttemptRequestDto {
        attempt_id: id(),
        program_id,
        expected_revision: test.summary.revision,
        module_id: module.id,
        lesson_id: None,
        kind: LearningAssessmentKind::Test,
        answers: vec![LearningAnswerDto {
            question_id: question.clone(),
            selected_index: 0,
        }],
    };
    assert!(repo.submit(&req).await.is_err());
    Ok(())
}

#[tokio::test]
async fn completion_advances_resume_to_first_incomplete_lesson() -> Result<()> {
    let pool = pool().await?;
    let repo = LearningRepository::new(pool);
    let p = ready_program(&repo).await?;
    let first = &p.modules[0].lessons[0];
    let expected_next = p.modules[0].lessons[1].id.clone();
    repo.complete(&CompleteLearningLessonRequestDto {
        program_id: p.summary.id.clone(),
        lesson_id: first.id.clone(),
        expected_revision: p.summary.revision,
    })
    .await?;
    let completed = repo.get(&p.summary.id).await?;
    assert_eq!(completed.summary.current_lesson_id, Some(expected_next));
    assert_eq!(completed.summary.completed_lessons, 1);
    assert!(repo
        .complete(&CompleteLearningLessonRequestDto {
            program_id: p.summary.id.clone(),
            lesson_id: first.id.clone(),
            expected_revision: completed.summary.revision,
        })
        .await
        .is_err());
    assert_eq!(
        repo.get(&p.summary.id).await?.summary.revision,
        completed.summary.revision
    );
    Ok(())
}

fn source_capture(title: &str, text: &str) -> super::source_library::CapturedLearningSource {
    super::source_library::CapturedLearningSource {
        title: title.into(),
        publisher: None,
        requested_url: Some("https://example.org/source".into()),
        resolved_url: Some("https://example.org/source".into()),
        text: text.into(),
        truncated: false,
        extraction_version: "test_capture_v1".into(),
    }
}

#[tokio::test]
async fn deleted_source_snapshot_keeps_tombstone_versions_and_refresh_checks() -> Result<()> {
    let pool = pool().await?;
    let repo = LearningRepository::new(pool.clone());
    let program = ready_program(&repo).await?;
    let library = super::source_library::LearningSourceLibraryRepository::new(pool);
    let source_id = id();
    let initial_version_id = id();
    library
        .add(
            &id(),
            &source_id,
            &initial_version_id,
            &program.summary.id,
            LearningSourceKind::Web,
            "add_web",
            "https://example.org/learning-source",
            Some("https://example.org/learning-source"),
            LearningSourcePolicy::Manual,
            source_capture("Snapshot source", "original immutable text"),
            "initial-source-payload".into(),
        )
        .await?;
    library
        .refresh(
            &RefreshLearningSourceRequestDto {
                operation_id: id(),
                program_id: program.summary.id.clone(),
                source_id: source_id.clone(),
                expected_revision: 0,
            },
            Some(source_capture(
                "Snapshot source",
                "refreshed immutable text",
            )),
            None,
        )
        .await?;
    let reason = "Removed from the learner's source list";
    library
        .delete_source(&super::portability_dto::DeleteLearningSourceRequestDto {
            operation_id: id(),
            program_id: program.summary.id.clone(),
            source_id: source_id.clone(),
            expected_revision: 1,
            reason: reason.into(),
        })
        .await?;

    let ordinary = library.workspace(&program.summary.id).await?;
    assert!(!ordinary.sources.iter().any(|source| source.id == source_id));

    let snapshot = library
        .workspace_including_deleted(&program.summary.id)
        .await?;
    let source = snapshot
        .sources
        .iter()
        .find(|source| source.id == source_id)
        .ok_or_else(|| {
            crate::shared::error::AppError::NotFound("deleted source missing from snapshot".into())
        })?;
    assert!(source.deleted_at.is_some());
    assert_eq!(source.deletion_reason.as_deref(), Some(reason));
    assert_eq!(source.versions.len(), 2);
    assert!(source
        .versions
        .iter()
        .any(|version| version.id == initial_version_id));
    assert_eq!(source.checks.len(), 1);
    assert_eq!(
        source.checks[0].status,
        LearningSourceCheckStatus::UpdateAvailable
    );
    Ok(())
}

#[tokio::test]
async fn source_versions_replay_refresh_adopt_and_unicode_search_are_durable() -> Result<()> {
    let pool = pool().await?;
    let repo = LearningRepository::new(pool.clone());
    let program = ready_program(&repo).await?;
    let library = super::source_library::LearningSourceLibraryRepository::new(pool.clone());
    assert!(
        super::source_library::LearningSourceLibraryRepository::before_use_allows(
            &LearningSourceCheckStatus::Unchanged
        )
        .is_ok()
    );
    assert!(
        super::source_library::LearningSourceLibraryRepository::before_use_allows(
            &LearningSourceCheckStatus::UpdateAvailable
        )
        .is_err()
    );
    assert!(
        super::source_library::LearningSourceLibraryRepository::before_use_allows(
            &LearningSourceCheckStatus::Failed
        )
        .is_err()
    );
    let source_id = id();
    let version_id = id();
    let op = id();
    let add = AddLearningTextSourceRequestDto {
        operation_id: op.clone(),
        source_id: source_id.clone(),
        version_id: version_id.clone(),
        program_id: program.summary.id.clone(),
        title: "Reference".into(),
        publisher: Some("Publisher".into()),
        text: "  Introductory text.\r\nCafé precedes München, which has a river.  ".into(),
    };
    let hash = super::plugin::source_request_hash(&add)?;
    library
        .add(
            &op,
            &source_id,
            &version_id,
            &program.summary.id,
            LearningSourceKind::Pasted,
            "add_text",
            "Reference",
            None,
            LearningSourcePolicy::Fixed,
            source_capture("Reference", &add.text),
            hash.clone(),
        )
        .await?;
    // Repeating the receipt after the response is lost does not create another version.
    library
        .add(
            &op,
            &source_id,
            &version_id,
            &program.summary.id,
            LearningSourceKind::Pasted,
            "add_text",
            "Reference",
            None,
            LearningSourcePolicy::Fixed,
            source_capture("Reference", "changed retry text"),
            hash.clone(),
        )
        .await?;
    assert!(
        library
            .preflight_replay(&op, &program.summary.id, &source_id, "add_text", &hash)
            .await?
    );
    assert!(library
        .preflight_replay(
            &op,
            &program.summary.id,
            &source_id,
            "add_text",
            "changed-payload"
        )
        .await
        .is_err());
    assert_eq!(
        library
            .workspace(&program.summary.id)
            .await?
            .sources
            .iter()
            .filter(|s| s.id == source_id)
            .count(),
        1
    );
    let search = library
        .search(&SearchLearningSourcesRequestDto {
            program_id: program.summary.id.clone(),
            query: "MÜNCHEN".into(),
            limit: 10,
        })
        .await?;
    assert_eq!(search.len(), 1);
    assert!(search[0].excerpt.contains("München"));
    let version = library
        .get_version(&GetLearningSourceVersionRequestDto {
            program_id: program.summary.id.clone(),
            source_id: source_id.clone(),
            version_id: version_id.clone(),
        })
        .await?;
    assert_eq!(
        version.full_text,
        "Introductory text.\nCafé precedes München, which has a river."
    );
    assert_eq!(version.version.word_count, 9);

    let document_source_id = id();
    let document_version_id = id();
    library
        .add(
            &id(),
            &document_source_id,
            &document_version_id,
            &program.summary.id,
            LearningSourceKind::Document,
            "add_document",
            "fixture-document",
            None,
            LearningSourcePolicy::Fixed,
            source_capture("Document text", "Indexed document material"),
            "document-operation".into(),
        )
        .await?;
    let document_version = library
        .get_version(&GetLearningSourceVersionRequestDto {
            program_id: program.summary.id.clone(),
            source_id: document_source_id.clone(),
            version_id: document_version_id,
        })
        .await?;
    assert_eq!(document_version.full_text, "Indexed document material");
    assert!(library
        .workspace(&program.summary.id)
        .await?
        .sources
        .iter()
        .any(|s| s.id == document_source_id && s.kind == LearningSourceKind::Document));

    let web_id = id();
    let web_version = id();
    let web_op = id();
    library
        .add(
            &web_op,
            &web_id,
            &web_version,
            &program.summary.id,
            LearningSourceKind::Web,
            "add_web",
            "https://example.org/source",
            Some("https://example.org/source"),
            LearningSourcePolicy::Manual,
            source_capture("Web source", "stable words"),
            "same-payload".into(),
        )
        .await?;
    let refresh_op = id();
    let refresh = RefreshLearningSourceRequestDto {
        operation_id: refresh_op.clone(),
        program_id: program.summary.id.clone(),
        source_id: web_id.clone(),
        expected_revision: 0,
    };
    let stale = RefreshLearningSourceRequestDto {
        operation_id: id(),
        program_id: program.summary.id.clone(),
        source_id: web_id.clone(),
        expected_revision: 9,
    };
    assert!(library
        .refresh(
            &stale,
            Some(source_capture("Web source", "stale write")),
            None
        )
        .await
        .is_err());
    library
        .refresh(
            &refresh,
            Some(source_capture("Web source", "new captured information")),
            None,
        )
        .await?;
    let workspace = library.workspace(&program.summary.id).await?;
    let item = workspace
        .sources
        .iter()
        .find(|s| s.id == web_id)
        .ok_or_else(|| crate::shared::error::AppError::NotFound("source missing".into()))?;
    assert_eq!(item.revision, 1);
    assert_eq!(
        item.latest_check.as_ref().map(|c| c.status.clone()),
        Some(LearningSourceCheckStatus::UpdateAvailable)
    );
    let pending = item
        .pending_version_id
        .clone()
        .ok_or_else(|| crate::shared::error::AppError::NotFound("pending missing".into()))?;
    assert!(!repo
        .get(&program.summary.id)
        .await?
        .sources
        .iter()
        .any(|s| s.id == pending));
    // A lost refresh response replays its receipt without another version.
    library
        .refresh(
            &refresh,
            Some(source_capture(
                "Changed retry title",
                "different retry result",
            )),
            None,
        )
        .await?;
    let workspace = library.workspace(&program.summary.id).await?;
    let item = workspace
        .sources
        .iter()
        .find(|s| s.id == web_id)
        .ok_or_else(|| crate::shared::error::AppError::NotFound("source missing".into()))?;
    assert_eq!(item.versions.len(), 2);
    assert_eq!(item.pending_version_id.as_deref(), Some(pending.as_str()));
    // Matching active content clears a stale update pointer and advances CAS.
    let unchanged = RefreshLearningSourceRequestDto {
        operation_id: id(),
        program_id: program.summary.id.clone(),
        source_id: web_id.clone(),
        expected_revision: 1,
    };
    library
        .refresh(
            &unchanged,
            Some(source_capture("Web source", "stable words")),
            None,
        )
        .await?;
    let workspace = library.workspace(&program.summary.id).await?;
    let item = workspace
        .sources
        .iter()
        .find(|s| s.id == web_id)
        .ok_or_else(|| crate::shared::error::AppError::NotFound("source missing".into()))?;
    assert_eq!(item.revision, 2);
    assert!(item.pending_version_id.is_none());
    assert_eq!(
        item.latest_check.as_ref().map(|c| c.status.clone()),
        Some(LearningSourceCheckStatus::Unchanged)
    );
    // Rechecking the prior changed digest reuses its immutable version ID.
    let changed_again = RefreshLearningSourceRequestDto {
        operation_id: id(),
        program_id: program.summary.id.clone(),
        source_id: web_id.clone(),
        expected_revision: 2,
    };
    library
        .refresh(
            &changed_again,
            Some(source_capture("Web source", "new captured information")),
            None,
        )
        .await?;
    let workspace = library.workspace(&program.summary.id).await?;
    let item = workspace
        .sources
        .iter()
        .find(|s| s.id == web_id)
        .ok_or_else(|| crate::shared::error::AppError::NotFound("source missing".into()))?;
    assert_eq!(item.revision, 3);
    assert_eq!(item.versions.len(), 2);
    assert_eq!(item.pending_version_id.as_deref(), Some(pending.as_str()));
    let failed = RefreshLearningSourceRequestDto {
        operation_id: id(),
        program_id: program.summary.id.clone(),
        source_id: web_id.clone(),
        expected_revision: 3,
    };
    library
        .refresh(&failed, None, Some("offline".into()))
        .await?;
    let workspace = library.workspace(&program.summary.id).await?;
    let item = workspace
        .sources
        .iter()
        .find(|s| s.id == web_id)
        .ok_or_else(|| crate::shared::error::AppError::NotFound("source missing".into()))?;
    assert_eq!(
        item.active_version_id.as_deref(),
        Some(web_version.as_str())
    );
    assert_eq!(item.pending_version_id.as_deref(), Some(pending.as_str()));
    assert_eq!(
        item.latest_check.as_ref().map(|c| c.status.clone()),
        Some(LearningSourceCheckStatus::Failed)
    );
    let adopt = AdoptLearningSourceVersionRequestDto {
        operation_id: id(),
        program_id: program.summary.id.clone(),
        source_id: web_id.clone(),
        version_id: pending.clone(),
        expected_revision: 3,
    };
    library.adopt(&adopt).await?;
    let workspace = library.workspace(&program.summary.id).await?;
    let item = workspace
        .sources
        .iter()
        .find(|s| s.id == web_id)
        .ok_or_else(|| crate::shared::error::AppError::NotFound("source missing".into()))?;
    assert_eq!(item.active_version_id.as_deref(), Some(pending.as_str()));
    assert!(item.pending_version_id.is_none());
    assert_eq!(item.versions.len(), 2);
    assert_eq!(item.revision, 4);
    assert!(repo
        .get(&program.summary.id)
        .await?
        .sources
        .iter()
        .any(|s| s.id == pending));
    Ok(())
}

#[tokio::test]
async fn source_versions_survive_pool_restart_are_program_isolated_and_cascade_on_delete(
) -> Result<()> {
    let directory =
        tempfile::tempdir().map_err(|e| crate::shared::error::AppError::Database(e.to_string()))?;
    let path = directory.path().join("learning-sources.sqlite");
    let options = SqliteConnectOptions::new()
        .filename(&path)
        .create_if_missing(true)
        .foreign_keys(true)
        .busy_timeout(std::time::Duration::from_secs(5));
    let pool1 = SqlitePoolOptions::new()
        .max_connections(2)
        .connect_with(options.clone())
        .await
        .map_err(|e| crate::shared::error::AppError::Database(e.to_string()))?;
    sqlx::migrate!("./migrations")
        .run(&pool1)
        .await
        .map_err(|e| crate::shared::error::AppError::Database(e.to_string()))?;
    let repo1 = LearningRepository::new(pool1.clone());
    let program1 = ready_program(&repo1).await?;
    let program2 = ready_program(&repo1).await?;
    let library1 = super::source_library::LearningSourceLibraryRepository::new(pool1.clone());
    let source_id = id();
    let version_id = id();
    let operation_id = id();
    let capture = source_capture("Persisted source", "A durable stored source body.");
    library1
        .add(
            &operation_id,
            &source_id,
            &version_id,
            &program1.summary.id,
            LearningSourceKind::Pasted,
            "add_text",
            "Persisted",
            None,
            LearningSourcePolicy::Fixed,
            capture.clone(),
            "source-payload".into(),
        )
        .await?;
    drop(library1);
    drop(repo1);
    pool1.close().await;
    let pool2 = SqlitePoolOptions::new()
        .max_connections(2)
        .connect_with(options)
        .await
        .map_err(|e| crate::shared::error::AppError::Database(e.to_string()))?;
    sqlx::migrate!("./migrations")
        .run(&pool2)
        .await
        .map_err(|e| crate::shared::error::AppError::Database(e.to_string()))?;
    let library2 = super::source_library::LearningSourceLibraryRepository::new(pool2.clone());
    let reopened = library2
        .get_version(&GetLearningSourceVersionRequestDto {
            program_id: program1.summary.id.clone(),
            source_id: source_id.clone(),
            version_id: version_id.clone(),
        })
        .await?;
    assert_eq!(reopened.full_text, "A durable stored source body.");
    assert!(library2
        .get_version(&GetLearningSourceVersionRequestDto {
            program_id: program2.summary.id.clone(),
            source_id: source_id.clone(),
            version_id: version_id.clone()
        })
        .await
        .is_err());
    assert!(library2
        .search(&SearchLearningSourcesRequestDto {
            program_id: program2.summary.id.clone(),
            query: "durable stored source".into(),
            limit: 10
        })
        .await?
        .is_empty());
    let repo2 = LearningRepository::new(pool2.clone());
    repo2.delete(&program1.summary.id).await?;
    let logical: i64 =
        sqlx::query_scalar("SELECT count(*) FROM learning_source_library WHERE program_id=?")
            .bind(&program1.summary.id)
            .fetch_one(&pool2)
            .await
            .map_err(|e| crate::shared::error::AppError::Database(e.to_string()))?;
    let versions: i64 =
        sqlx::query_scalar("SELECT count(*) FROM learning_source_versions WHERE program_id=?")
            .bind(&program1.summary.id)
            .fetch_one(&pool2)
            .await
            .map_err(|e| crate::shared::error::AppError::Database(e.to_string()))?;
    assert_eq!(logical, 0);
    assert_eq!(versions, 0);
    pool2.close().await;
    Ok(())
}

#[tokio::test]
async fn legacy_migration_metadata_is_backfilled_from_the_captured_text() -> Result<()> {
    let pool = pool().await?;
    let repo = LearningRepository::new(pool.clone());
    let program = ready_program(&repo).await?;
    let program2 = ready_program(&repo).await?;
    // Indexed document passage IDs are deterministic and may be shared across programs.
    let legacy_source = format!("{}:{}", id(), id());
    for program_id in [&program.summary.id, &program2.summary.id] {
        sqlx::query("INSERT INTO learning_sources(id,program_id,title,url,excerpt,acquired_at) VALUES(?,?,?,NULL,?,7)")
            .bind(&legacy_source).bind(program_id).bind("Legacy excerpt").bind("migration text has four words")
            .execute(&pool).await.map_err(|e|crate::shared::error::AppError::Database(e.to_string()))?;
        sqlx::query("INSERT INTO learning_source_library(id,program_id,kind,origin,requested_url,freshness_policy,active_version_id,pending_version_id,revision,created_at,updated_at) VALUES(?,?, 'document',?,NULL,'fixed',?,NULL,0,7,7)")
            .bind(&legacy_source).bind(program_id).bind(&legacy_source).bind(&legacy_source)
            .execute(&pool).await.map_err(|e|crate::shared::error::AppError::Database(e.to_string()))?;
        sqlx::query("INSERT INTO learning_source_versions(id,program_id,source_id,version_number,title,publisher,requested_url,resolved_url,full_text,excerpt,content_sha256,word_count,truncated,extraction_version,acquired_at) VALUES(?,?,?,1,'Legacy excerpt',NULL,NULL,NULL,'migration text has four words','migration text has four words','',0,1,'legacy_bounded_extraction_v1',7)")
            .bind(&legacy_source).bind(program_id).bind(&legacy_source)
            .execute(&pool).await.map_err(|e|crate::shared::error::AppError::Database(e.to_string()))?;
    }
    let library = super::source_library::LearningSourceLibraryRepository::new(pool.clone());
    let workspace = library.workspace(&program.summary.id).await?;
    let version = workspace
        .sources
        .iter()
        .find(|s| s.id == legacy_source)
        .and_then(|s| s.active_version.as_ref())
        .ok_or_else(|| crate::shared::error::AppError::NotFound("legacy version missing".into()))?;
    assert_eq!(version.content_sha256.len(), 64);
    assert!(version
        .content_sha256
        .chars()
        .all(|c| c.is_ascii_hexdigit()));
    assert_eq!(version.word_count, 5);
    let other_workspace = library.workspace(&program2.summary.id).await?;
    assert!(other_workspace
        .sources
        .iter()
        .any(|s| s.id == legacy_source));
    let reopened = library
        .get_version(&GetLearningSourceVersionRequestDto {
            program_id: program.summary.id,
            source_id: legacy_source.clone(),
            version_id: legacy_source,
        })
        .await?;
    assert_eq!(reopened.version.word_count, 5);
    Ok(())
}

#[tokio::test]
async fn initially_generated_sources_are_normalized_and_persisted_as_active_versions() -> Result<()>
{
    let pool = pool().await?;
    let repo = LearningRepository::new(pool);
    let mut program = fixture();
    let source = program.sources.first_mut().ok_or_else(|| {
        crate::shared::error::AppError::InvalidInput("missing fixture source".into())
    })?;
    source.title = "  Captured title  ".into();
    source.excerpt = "  First line.\r\nSecond line.  ".into();
    repo.create(&program).await?;
    let stored = repo.get(&program.summary.id).await?;
    let stored_source = stored
        .sources
        .first()
        .ok_or_else(|| crate::shared::error::AppError::NotFound("source missing".into()))?;
    assert_eq!(stored_source.title, "Captured title");
    assert_eq!(stored_source.excerpt, "First line.\nSecond line.");
    let active = repo.active_sources(&program.summary.id).await?;
    assert_eq!(active.len(), program.sources.len());
    assert_eq!(
        active.first().map(|s| s.excerpt.as_str()),
        Some("First line.\nSecond line.")
    );
    Ok(())
}

#[tokio::test]
async fn source_refresh_and_reimport_continue_past_one_hundred_versions() -> Result<()> {
    let pool = pool().await?;
    let repo = LearningRepository::new(pool.clone());
    let program = ready_program(&repo).await?;
    let library = super::source_library::LearningSourceLibraryRepository::new(pool);
    let source_id = id();
    library
        .add(
            &id(),
            &source_id,
            &id(),
            &program.summary.id,
            LearningSourceKind::Web,
            "add_web",
            "https://example.org/versioned",
            Some("https://example.org/versioned"),
            LearningSourcePolicy::Manual,
            source_capture("Versioned source", "Initial captured reference."),
            "initial".into(),
        )
        .await?;
    for revision in 0..101 {
        library
            .refresh(
                &RefreshLearningSourceRequestDto {
                    operation_id: id(),
                    program_id: program.summary.id.clone(),
                    source_id: source_id.clone(),
                    expected_revision: revision,
                },
                Some(source_capture(
                    "Versioned source",
                    &format!("Captured reference revision {revision}."),
                )),
                None,
            )
            .await?;
    }
    let text = format!(
        "{}COMPLETE_SOURCE_TAIL",
        "large captured text ".repeat(110_000)
    );
    assert!(text.len() > 2_000_000);
    library
        .delete_source(&super::portability_dto::DeleteLearningSourceRequestDto {
            operation_id: id(),
            program_id: program.summary.id.clone(),
            source_id: source_id.clone(),
            expected_revision: 101,
            reason: "Exercise re-import after many saved versions".into(),
        })
        .await?;
    let version_id = id();
    let request = super::portability_dto::ReimportLearningSourceRequestDto {
        operation_id: id(),
        program_id: program.summary.id.clone(),
        source_id: source_id.clone(),
        version_id: version_id.clone(),
        expected_revision: 102,
        replacement_text: Some(text.clone()),
    };
    library.reimport_source(&request).await?;
    library.reimport_source(&request).await?;
    let workspace = library.workspace(&program.summary.id).await?;
    let source = workspace
        .sources
        .iter()
        .find(|source| source.id == source_id)
        .expect("source");
    assert_eq!(source.versions.len(), 103);
    assert!(
        !source
            .active_version
            .as_ref()
            .expect("active version")
            .truncated
    );
    let saved = library
        .get_version(&GetLearningSourceVersionRequestDto {
            program_id: program.summary.id,
            source_id,
            version_id,
        })
        .await?;
    assert_eq!(saved.full_text, text);
    Ok(())
}

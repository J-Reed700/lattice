//! Tests for `study.card`, `study.duplicates`, and single-writer card text updates.
use super::{
    recall_repository::LearningRecallRepository, study_dto::*, study_repository::StudyRepository,
};
use crate::{
    dto::{
        AcceptLearningProgramRequestDto, LearningProgramDto, LearningSourceKind,
        LearningSourcePolicy,
    },
    edits::{content_edits, keys::card_ref},
    portability_dto::{
        LearningRecallCardFormat, LearningRecallContentDto, LearningRecallDuplicateStatus,
        ReviewLearningRecallCardRequestDto, SaveLearningRecallCardRequestDto,
    },
    repository::LearningRepository,
    source_library::{CapturedLearningSource, NewLearningSource},
    tests,
};
use lattice_core::shared::edits::{
    ContentEditRequest, EditOp, EditSource, RevertContentEditRequest,
};
use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::persistence::Database;
use serde_json::json;
use sqlx::Row;

fn id() -> String {
    uuid::Uuid::new_v4().to_string()
}

async fn active_program(db: &Database) -> Result<LearningProgramDto> {
    let repo = LearningRepository::new(db.clone());
    let program = tests::fixture();
    repo.create(&program).await?;
    repo.accept(&AcceptLearningProgramRequestDto {
        program_id: program.summary.id.clone(),
        expected_revision: 0,
        title: program.summary.title.clone(),
    })
    .await?;
    repo.get(&program.summary.id).await
}

fn qa_content(prompt: &str, answer: &str, explanation: &str) -> LearningRecallContentDto {
    LearningRecallContentDto {
        prompt: prompt.into(),
        answer: answer.into(),
        explanation: explanation.into(),
        options: vec![],
        correct_option_index: None,
        language: None,
        cloze_deletions: vec![],
    }
}

fn save_request(
    program_id: &str,
    format: LearningRecallCardFormat,
    content: LearningRecallContentDto,
) -> SaveLearningRecallCardRequestDto {
    SaveLearningRecallCardRequestDto {
        operation_id: id(),
        program_id: program_id.into(),
        card_id: id(),
        expected_content_revision: None,
        format,
        content,
        source_version_ids: vec![],
        change_reason: "Initial draft".into(),
    }
}

fn edit_request(card_id: &str, expected_revision: String, ops: Vec<EditOp>) -> ContentEditRequest {
    ContentEditRequest {
        edit_id: id(),
        content: card_ref(card_id),
        expected_revision,
        ops,
        source: EditSource::Learner,
        target: None,
    }
}

async fn add_source_version(db: &Database, program_id: &str, marker: &str) -> Result<String> {
    let library = crate::source_library::LearningSourceLibraryRepository::new(db.clone());
    let (operation_id, source_id, version_id) = (id(), id(), id());
    let url = format!("https://example.com/{marker}");
    library
        .add(
            NewLearningSource {
                operation_id: &operation_id,
                source_id: &source_id,
                version_id: &version_id,
                program_id,
                kind: LearningSourceKind::Web,
                operation_kind: "add_web",
                origin: &url,
                requested_url: Some(&url),
                freshness: LearningSourcePolicy::BeforeUse,
            },
            CapturedLearningSource {
                title: format!("Source {marker}"),
                publisher: None,
                requested_url: Some(url.clone()),
                resolved_url: Some(url.clone()),
                text: format!("Passage {marker} explaining the topic."),
                truncated: false,
                extraction_version: "test_v1".into(),
                signals: crate::references::trust::capture::not_extracted("test fixture"),
            },
            id(),
        )
        .await?;
    Ok(version_id)
}

#[tokio::test]
async fn a_card_edit_reaches_the_recall_profile_and_adds_a_version() -> Result<()> {
    let pool = tests::pool().await?;
    let db = Database::new(pool.clone());
    let program = active_program(&db).await?;
    let pid = program.summary.id.clone();
    let recall = LearningRecallRepository::new(db.clone());

    let created = save_request(
        &pid,
        LearningRecallCardFormat::QuestionAnswer,
        qa_content("Initial prompt?", "Initial answer.", "Initial explanation."),
    );
    let card_id = created.card_id.clone();
    recall.save_card(&created).await?;

    let edits = content_edits(&db)?;
    let opened = edits.open(&card_ref(&card_id)).await;
    assert!(opened.is_ok(), "{opened:?}");
    let applied = edits
        .apply(&edit_request(
            &card_id,
            opened.unwrap().revision,
            vec![
                EditOp::Replace {
                    path: "/question".into(),
                    value: json!("Updated prompt?"),
                },
                EditOp::Replace {
                    path: "/answer".into(),
                    value: json!("Updated answer."),
                },
            ],
        ))
        .await;
    assert!(applied.is_ok(), "{applied:?}");

    let row = sqlx::query("SELECT question, answer FROM study_cards WHERE id=?")
        .bind(&card_id)
        .fetch_one(&pool)
        .await?;
    assert_eq!(row.get::<String, _>("question"), "Updated prompt?");
    assert_eq!(row.get::<String, _>("answer"), "Updated answer.");

    let workspace = recall.workspace(&pid).await?;
    let card = workspace.cards.iter().find(|c| c.id == card_id).unwrap();
    assert_eq!(card.content.prompt, "Updated prompt?");
    assert_eq!(card.content.answer, "Updated answer.");
    assert_eq!(card.content_revision, 2);
    assert_eq!(card.versions.len(), 2);
    assert_eq!(card.versions[1].change_reason, "Corrected in place.");
    Ok(())
}

#[tokio::test]
async fn a_card_edit_leaves_its_schedule() -> Result<()> {
    let pool = tests::pool().await?;
    let db = Database::new(pool.clone());
    let program = active_program(&db).await?;
    let pid = program.summary.id.clone();
    let recall = LearningRecallRepository::new(db.clone());

    let created = save_request(
        &pid,
        LearningRecallCardFormat::QuestionAnswer,
        qa_content("Initial prompt?", "Initial answer.", "Initial explanation."),
    );
    let card_id = created.card_id.clone();
    recall.save_card(&created).await?;
    recall
        .review_card(&ReviewLearningRecallCardRequestDto {
            review_id: id(),
            program_id: pid.clone(),
            card_id: card_id.clone(),
            expected_review_count: 0,
            rating: StudyRating::Good,
            selected_option: None,
        })
        .await?;

    let before_study = sqlx::query(
        "SELECT stability, difficulty, last_reviewed_at, due_at, interval_days, review_count, lapses, scheduler_version FROM study_cards WHERE id=?",
    )
    .bind(&card_id)
    .fetch_one(&pool)
    .await?;
    let before_profile = sqlx::query(
        "SELECT scheduler_version, scheduler_state_json FROM learning_recall_card_profiles WHERE card_id=?",
    )
    .bind(&card_id)
    .fetch_one(&pool)
    .await?;

    let edits = content_edits(&db)?;
    let opened = edits.open(&card_ref(&card_id)).await;
    assert!(opened.is_ok(), "{opened:?}");
    let opened = opened.unwrap();
    let applied = edits
        .apply(&edit_request(
            &card_id,
            opened.revision,
            vec![
                EditOp::Replace {
                    path: "/question".into(),
                    value: json!("Edited prompt?"),
                },
                EditOp::Replace {
                    path: "/answer".into(),
                    value: json!("Edited answer."),
                },
            ],
        ))
        .await;
    assert!(applied.is_ok(), "{applied:?}");

    let after_study = sqlx::query(
        "SELECT stability, difficulty, last_reviewed_at, due_at, interval_days, review_count, lapses, scheduler_version FROM study_cards WHERE id=?",
    )
    .bind(&card_id)
    .fetch_one(&pool)
    .await?;
    assert_eq!(
        after_study.get::<Option<f64>, _>("stability"),
        before_study.get::<Option<f64>, _>("stability")
    );
    assert_eq!(
        after_study.get::<Option<f64>, _>("difficulty"),
        before_study.get::<Option<f64>, _>("difficulty")
    );
    assert_eq!(
        after_study.get::<Option<i64>, _>("last_reviewed_at"),
        before_study.get::<Option<i64>, _>("last_reviewed_at")
    );
    assert_eq!(
        after_study.get::<i64, _>("due_at"),
        before_study.get::<i64, _>("due_at")
    );
    assert_eq!(
        after_study.get::<i64, _>("interval_days"),
        before_study.get::<i64, _>("interval_days")
    );
    assert_eq!(
        after_study.get::<i64, _>("review_count"),
        before_study.get::<i64, _>("review_count")
    );
    assert_eq!(
        after_study.get::<i64, _>("lapses"),
        before_study.get::<i64, _>("lapses")
    );
    assert_eq!(
        after_study.get::<String, _>("scheduler_version"),
        before_study.get::<String, _>("scheduler_version")
    );

    let after_profile = sqlx::query(
        "SELECT scheduler_version, scheduler_state_json FROM learning_recall_card_profiles WHERE card_id=?",
    )
    .bind(&card_id)
    .fetch_one(&pool)
    .await?;
    assert_eq!(
        after_profile.get::<String, _>("scheduler_version"),
        before_profile.get::<String, _>("scheduler_version")
    );
    assert_eq!(
        after_profile.get::<String, _>("scheduler_state_json"),
        before_profile.get::<String, _>("scheduler_state_json")
    );
    Ok(())
}

#[tokio::test]
async fn a_multiple_choice_card_keeps_its_answer_on_the_right_choice() -> Result<()> {
    let pool = tests::pool().await?;
    let db = Database::new(pool.clone());
    let program = active_program(&db).await?;
    let pid = program.summary.id.clone();
    let recall = LearningRecallRepository::new(db.clone());

    let created = save_request(
        &pid,
        LearningRecallCardFormat::MultipleChoice,
        LearningRecallContentDto {
            prompt: "Which option is right?".into(),
            answer: "Right B".into(),
            explanation: "Choice B is correct.".into(),
            options: vec!["Wrong A".into(), "Right B".into(), "Wrong C".into()],
            correct_option_index: Some(1),
            language: None,
            cloze_deletions: vec![],
        },
    );
    let card_id = created.card_id.clone();
    recall.save_card(&created).await?;

    let edits = content_edits(&db)?;
    let opened = edits.open(&card_ref(&card_id)).await;
    assert!(opened.is_ok(), "{opened:?}");
    let opened = opened.unwrap();
    let applied = edits
        .apply(&edit_request(
            &card_id,
            opened.revision,
            vec![EditOp::Remove {
                path: "/choices/0".into(),
            }],
        ))
        .await;
    assert!(applied.is_ok(), "{applied:?}");

    let row = sqlx::query("SELECT answer, options_json, correct_index FROM study_cards WHERE id=?")
        .bind(&card_id)
        .fetch_one(&pool)
        .await?;
    assert_eq!(row.get::<String, _>("answer"), "Right B");
    assert_eq!(row.get::<i64, _>("correct_index"), 0);
    let stored_options: Vec<String> = serde_json::from_str(row.get("options_json")).unwrap();
    assert_eq!(stored_options, vec!["Right B", "Wrong C"]);

    let workspace = recall.workspace(&pid).await?;
    let card = workspace.cards.iter().find(|c| c.id == card_id).unwrap();
    assert_eq!(card.content.answer, "Right B");
    assert_eq!(card.content.correct_option_index, Some(0));
    assert_eq!(card.content.options, vec!["Right B", "Wrong C"]);
    Ok(())
}

#[tokio::test]
async fn a_card_without_a_profile_is_edited_in_study_cards_alone() -> Result<()> {
    let pool = tests::pool().await?;
    let db = Database::new(pool.clone());
    let deck_id = id();
    let card_id = id();
    StudyRepository::new(db.clone())
        .save(&StudyDeckDto {
            id: deck_id.clone(),
            title: "Standalone deck".into(),
            focus: "basics".into(),
            study_goal: "Recall facts".into(),
            model_name: "fixture".into(),
            created_at: 100,
            target_at: None,
            cards: vec![StudyCardDto {
                id: card_id.clone(),
                format: StudyCardFormat::QuestionAnswer,
                deck_id,
                question: "Original standalone prompt?".into(),
                answer: "Original standalone answer.".into(),
                options: vec![],
                correct_index: 0,
                explanation: "Original standalone explanation.".into(),
                source: StudySourceDto {
                    chunk_id: "c1".into(),
                    document_id: "d1".into(),
                    file_name: "notes.md".into(),
                    file_path: "/notes.md".into(),
                    excerpt: "An excerpt.".into(),
                    url: None,
                },
                citations: vec![],
                topic: "Basics".into(),
                due_at: 100,
                interval_days: 0,
                review_count: 0,
                lapses: 0,
            }],
        })
        .await?;

    let edits = content_edits(&db)?;
    let opened = edits.open(&card_ref(&card_id)).await;
    assert!(opened.is_ok(), "{opened:?}");
    let opened = opened.unwrap();
    let applied = edits
        .apply(&edit_request(
            &card_id,
            opened.revision,
            vec![
                EditOp::Replace {
                    path: "/question".into(),
                    value: json!("Edited standalone prompt?"),
                },
                EditOp::Replace {
                    path: "/answer".into(),
                    value: json!("Edited standalone answer."),
                },
            ],
        ))
        .await;
    assert!(applied.is_ok(), "{applied:?}");

    let row = sqlx::query("SELECT question, answer FROM study_cards WHERE id=?")
        .bind(&card_id)
        .fetch_one(&pool)
        .await?;
    assert_eq!(
        row.get::<String, _>("question"),
        "Edited standalone prompt?"
    );
    assert_eq!(row.get::<String, _>("answer"), "Edited standalone answer.");

    let profile_count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM learning_recall_card_profiles WHERE card_id=?")
            .bind(&card_id)
            .fetch_one(&pool)
            .await?;
    assert_eq!(profile_count, 0);
    let version_count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM learning_recall_card_versions WHERE card_id=?")
            .bind(&card_id)
            .fetch_one(&pool)
            .await?;
    assert_eq!(version_count, 0);
    Ok(())
}

#[tokio::test]
async fn undoing_a_card_edit_restores_both_stores_and_says_so() -> Result<()> {
    let pool = tests::pool().await?;
    let db = Database::new(pool.clone());
    let program = active_program(&db).await?;
    let pid = program.summary.id.clone();
    let recall = LearningRecallRepository::new(db.clone());

    let created = save_request(
        &pid,
        LearningRecallCardFormat::QuestionAnswer,
        qa_content(
            "Original prompt?",
            "Original answer.",
            "Original explanation.",
        ),
    );
    let card_id = created.card_id.clone();
    recall.save_card(&created).await?;

    let edits = content_edits(&db)?;
    let opened = edits.open(&card_ref(&card_id)).await;
    assert!(opened.is_ok(), "{opened:?}");
    let opened = opened.unwrap();
    let applied = edits
        .apply(&edit_request(
            &card_id,
            opened.revision,
            vec![
                EditOp::Replace {
                    path: "/question".into(),
                    value: json!("Changed prompt?"),
                },
                EditOp::Replace {
                    path: "/answer".into(),
                    value: json!("Changed answer."),
                },
            ],
        ))
        .await;
    assert!(applied.is_ok(), "{applied:?}");
    let applied = applied.unwrap();

    let reverted = edits
        .revert(&RevertContentEditRequest {
            revert_id: id(),
            edit_id: applied.edit_id,
            expected_revision: applied.revision,
            and_later: false,
        })
        .await;
    assert!(reverted.is_ok(), "{reverted:?}");

    let row = sqlx::query("SELECT question, answer FROM study_cards WHERE id=?")
        .bind(&card_id)
        .fetch_one(&pool)
        .await?;
    assert_eq!(row.get::<String, _>("question"), "Original prompt?");
    assert_eq!(row.get::<String, _>("answer"), "Original answer.");

    let workspace = recall.workspace(&pid).await?;
    let card = workspace.cards.iter().find(|c| c.id == card_id).unwrap();
    assert_eq!(card.content.prompt, "Original prompt?");
    assert_eq!(card.content.answer, "Original answer.");
    assert_eq!(card.versions.len(), 3);
    assert_eq!(card.versions[2].change_reason, "Undid a correction.");
    Ok(())
}

#[tokio::test]
async fn a_changed_question_is_checked_for_duplicates_again() -> Result<()> {
    let pool = tests::pool().await?;
    let db = Database::new(pool.clone());
    let program = active_program(&db).await?;
    let pid = program.summary.id.clone();
    let recall = LearningRecallRepository::new(db.clone());

    let first = save_request(
        &pid,
        LearningRecallCardFormat::QuestionAnswer,
        qa_content(
            "How do rivers carve deep valleys over centuries?",
            "Erosion removes sediment.",
            "Flowing water wears rock.",
        ),
    );
    let second = save_request(
        &pid,
        LearningRecallCardFormat::QuestionAnswer,
        qa_content(
            "Why do glaciers deposit terminal moraines?",
            "Melting drops debris.",
            "Ice retreats at the snout.",
        ),
    );
    let second_id = second.card_id.clone();
    recall.save_card(&first).await?;
    recall.save_card(&second).await?;
    assert!(recall.workspace(&pid).await?.duplicates.is_empty());

    let edits = content_edits(&db)?;
    let opened = edits.open(&card_ref(&second_id)).await;
    assert!(opened.is_ok(), "{opened:?}");
    let opened = opened.unwrap();

    let answer_only = edits
        .apply(&edit_request(
            &second_id,
            opened.revision,
            vec![EditOp::Replace {
                path: "/answer".into(),
                value: json!("Melting drops rock debris."),
            }],
        ))
        .await;
    assert!(answer_only.is_ok(), "{answer_only:?}");
    let answer_only = answer_only.unwrap();
    assert!(recall.workspace(&pid).await?.duplicates.is_empty());

    let prompt_edit = edits
        .apply(&edit_request(
            &second_id,
            answer_only.revision,
            vec![EditOp::Replace {
                path: "/question".into(),
                value: json!("How do rivers carve wide valleys over centuries?"),
            }],
        ))
        .await;
    assert!(prompt_edit.is_ok(), "{prompt_edit:?}");

    let duplicates = recall.workspace(&pid).await?.duplicates;
    assert!(duplicates.iter().any(|item| {
        item.status == LearningRecallDuplicateStatus::Pending
            && ((item.card_id == first.card_id && item.possible_duplicate_card_id == second_id)
                || (item.card_id == second_id && item.possible_duplicate_card_id == first.card_id))
    }));
    Ok(())
}

#[tokio::test]
async fn a_save_that_keeps_the_format_cannot_change_the_text() -> Result<()> {
    let pool = tests::pool().await?;
    let db = Database::new(pool.clone());
    let program = active_program(&db).await?;
    let pid = program.summary.id.clone();
    let recall = LearningRecallRepository::new(db.clone());
    let v1 = add_source_version(&db, &pid, "v1").await?;
    let v2 = add_source_version(&db, &pid, "v2").await?;

    let mut first = save_request(
        &pid,
        LearningRecallCardFormat::QuestionAnswer,
        qa_content(
            "Original prompt?",
            "Original answer.",
            "Original explanation.",
        ),
    );
    first.source_version_ids = vec![v1.clone()];
    recall.store_card(&first).await?;

    let mut text_edit = first.clone();
    text_edit.operation_id = id();
    text_edit.expected_content_revision = Some(1);
    text_edit.content.prompt = "Changed prompt via save?".into();
    text_edit.change_reason = "Tried to edit text via save".into();
    let refused = recall.store_card(&text_edit).await;
    assert!(
        matches!(refused, Err(AppError::InvalidInput(ref msg)) if msg == "Edit this card in place to change its text."),
        "{refused:?}"
    );

    let mut sources_only = first.clone();
    sources_only.operation_id = id();
    sources_only.expected_content_revision = Some(1);
    sources_only.source_version_ids = vec![v1.clone(), v2.clone()];
    sources_only.change_reason = "Added another source".into();
    recall.store_card(&sources_only).await?;

    let workspace = recall.workspace(&pid).await?;
    let card = workspace
        .cards
        .iter()
        .find(|c| c.id == first.card_id)
        .unwrap();
    assert_eq!(card.source_version_ids, vec![v1, v2]);
    assert_eq!(card.content.prompt, "Original prompt?");
    assert_eq!(card.content.answer, "Original answer.");
    Ok(())
}

#[tokio::test]
async fn converting_a_card_ends_its_undo_chain() -> Result<()> {
    let pool = tests::pool().await?;
    let db = Database::new(pool.clone());
    let program = active_program(&db).await?;
    let pid = program.summary.id.clone();
    let recall = LearningRecallRepository::new(db.clone());

    let first = save_request(
        &pid,
        LearningRecallCardFormat::QuestionAnswer,
        qa_content("Initial prompt?", "Initial answer.", "Initial explanation."),
    );
    let card_id = first.card_id.clone();
    recall.save_card(&first).await?;

    let edits = content_edits(&db)?;
    let opened = edits.open(&card_ref(&card_id)).await;
    assert!(opened.is_ok(), "{opened:?}");
    let opened = opened.unwrap();
    let applied = edits
        .apply(&edit_request(
            &card_id,
            opened.revision,
            vec![EditOp::Replace {
                path: "/answer".into(),
                value: json!("Corrected answer."),
            }],
        ))
        .await;
    assert!(applied.is_ok(), "{applied:?}");
    let applied = applied.unwrap();

    recall
        .store_card(&SaveLearningRecallCardRequestDto {
            operation_id: id(),
            program_id: pid.clone(),
            card_id: card_id.clone(),
            expected_content_revision: Some(2),
            format: LearningRecallCardFormat::MultipleChoice,
            content: LearningRecallContentDto {
                prompt: "Initial prompt?".into(),
                answer: "Choice A".into(),
                explanation: "Initial explanation.".into(),
                options: vec!["Choice A".into(), "Choice B".into()],
                correct_option_index: Some(0),
                language: None,
                cloze_deletions: vec![],
            },
            source_version_ids: vec![],
            change_reason: "Converted to multiple choice".into(),
        })
        .await?;

    let reopened = edits.open(&card_ref(&card_id)).await;
    assert!(reopened.is_ok(), "{reopened:?}");
    let refused = edits
        .revert(&RevertContentEditRequest {
            revert_id: id(),
            edit_id: applied.edit_id,
            expected_revision: reopened.unwrap().revision,
            and_later: false,
        })
        .await;
    assert!(
        matches!(refused, Err(AppError::InvalidState(ref msg)) if msg == "This was replaced after that change, so the change can't be undone."),
        "{refused:?}"
    );
    Ok(())
}

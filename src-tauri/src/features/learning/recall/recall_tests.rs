use crate::features::learning::source_selector::LearningQuoteMatchStatus;
use crate::features::learning::{
    dto::GetLearningSourceVersionRequestDto,
    portability_dto::*,
    recall_repository::LearningRecallRepository,
    repository::LearningRepository,
    source_library::{CapturedLearningSource, LearningSourceLibraryRepository},
    tests,
};
use crate::{
    features::study::{dto::*, repository::StudyRepository},
    shared::error::Result,
};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::Row;
use std::str::FromStr;

fn id() -> String {
    uuid::Uuid::new_v4().to_string()
}

async fn file_pool(path: &std::path::Path) -> Result<sqlx::SqlitePool> {
    let options = SqliteConnectOptions::from_str(&format!("sqlite://{}", path.display()))
        .map_err(|error| crate::shared::error::AppError::Database(error.to_string()))?
        .create_if_missing(true)
        .foreign_keys(true);
    SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .map_err(|error| crate::shared::error::AppError::Database(error.to_string()))
}

async fn active_program(
    pool: &sqlx::SqlitePool,
) -> Result<crate::features::learning::dto::LearningProgramDto> {
    let repo = LearningRepository::new(pool.clone());
    let program = tests::fixture();
    repo.create(&program).await?;
    repo.accept(
        &crate::features::learning::dto::AcceptLearningProgramRequestDto {
            program_id: program.summary.id.clone(),
            expected_revision: 0,
            title: program.summary.title.clone(),
        },
    )
    .await?;
    repo.get(&program.summary.id).await
}

#[tokio::test]
async fn recall_versions_scheduler_replay_and_study_guard_are_durable() -> Result<()> {
    let pool = tests::pool().await?;
    let program = active_program(&pool).await?;
    let memory = LearningRepository::new(pool.clone());
    let lesson = program
        .modules
        .first()
        .and_then(|module| module.lessons.first())
        .ok_or_else(|| {
            crate::shared::error::AppError::InvalidState("Fixture has no lesson".into())
        })?;
    memory
        .ensure_lesson_note(&program.summary.id, &lesson.id, "Notes", "")
        .await?;
    let recall = LearningRecallRepository::new(pool.clone());
    let card_id = id();
    let save = SaveLearningRecallCardRequestDto {
        operation_id: id(),
        program_id: program.summary.id.clone(),
        card_id: card_id.clone(),
        expected_content_revision: None,
        format: LearningRecallCardFormat::MultipleChoice,
        content: LearningRecallContentDto {
            prompt: "Which option is correct?".into(),
            answer: "Choice B".into(),
            explanation: "The second choice is supported.".into(),
            options: vec!["Choice A".into(), "Choice B".into()],
            correct_option_index: Some(1),
            language: None,
            cloze_deletions: vec![],
        },
        source_version_ids: vec![],
        change_reason: "Initial card".into(),
    };
    recall.save_card(&save).await?;
    let review = ReviewLearningRecallCardRequestDto {
        review_id: id(),
        program_id: program.summary.id.clone(),
        card_id: card_id.clone(),
        expected_review_count: 0,
        rating: StudyRating::Hard,
        selected_option: Some(1),
    };
    let reviewed = recall.review_card(&review).await?;
    assert_eq!(
        reviewed
            .cards
            .first()
            .map(|card| card.scheduler.review_count),
        Some(1)
    );
    let replay = recall.review_card(&review).await?;
    assert_eq!(
        replay.cards.first().map(|card| card.scheduler.review_count),
        Some(1)
    );
    let mut changed = review.clone();
    changed.expected_review_count = 1;
    assert!(recall.review_card(&changed).await.is_err());
    let mut changed_rating = review.clone();
    changed_rating.rating = StudyRating::Easy;
    assert!(recall.review_card(&changed_rating).await.is_err());

    let fsrs = ChangeLearningRecallSchedulerRequestDto {
        operation_id: id(),
        program_id: program.summary.id.clone(),
        card_id: card_id.clone(),
        expected_review_count: 1,
        scheduler_version: LearningRecallSchedulerVersion::Fsrs6V1,
    };
    let fsrs_replay = recall.change_scheduler(&fsrs).await?;
    assert_eq!(
        fsrs_replay
            .cards
            .first()
            .map(|card| card.scheduler.scheduler_version),
        Some(LearningRecallSchedulerVersion::Fsrs6V1)
    );
    let mut changed_scheduler_payload = fsrs.clone();
    changed_scheduler_payload.expected_review_count = 0;
    assert!(recall
        .change_scheduler(&changed_scheduler_payload)
        .await
        .is_err());
    let switched = recall.change_scheduler(&fsrs).await?;
    assert_eq!(
        switched
            .cards
            .first()
            .map(|card| card.scheduler.scheduler_version),
        Some(LearningRecallSchedulerVersion::Fsrs6V1)
    );
    let row = sqlx::query("SELECT scheduler_version,review_count FROM study_cards WHERE id=?")
        .bind(&card_id)
        .fetch_one(&pool)
        .await
        .map_err(|error| crate::shared::error::AppError::Database(error.to_string()))?;
    assert_eq!(row.get::<String, _>("scheduler_version"), "fsrs_6_v1");
    assert_eq!(row.get::<i64, _>("review_count"), 1);
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT scheduler_version FROM study_reviews WHERE id=?")
            .bind(&review.review_id)
            .fetch_one(&pool)
            .await
            .map_err(|error| crate::shared::error::AppError::Database(error.to_string()))?,
        "expanding_v1"
    );
    let ordinary_study = StudyRepository::new(pool.clone());
    assert!(ordinary_study
        .review(
            &ReviewStudyCardRequestDto {
                review_id: id(),
                card_id: card_id.clone(),
                expected_reviews: 1,
                rating: StudyRating::Good,
                selected_option: Some(1),
            },
            chrono::Utc::now().timestamp_millis(),
        )
        .await
        .is_err());

    let reverse = ChangeLearningRecallSchedulerRequestDto {
        operation_id: id(),
        scheduler_version: LearningRecallSchedulerVersion::ExpandingV1,
        ..fsrs
    };
    let reversed = recall.change_scheduler(&reverse).await?;
    assert_eq!(
        reversed
            .cards
            .first()
            .map(|card| card.scheduler.scheduler_version),
        Some(LearningRecallSchedulerVersion::ExpandingV1)
    );
    assert_eq!(
        reversed.cards.first().map(|card| card.versions.len()),
        Some(1)
    );
    assert_eq!(
        reversed.cards.first().map(|card| card.scheduler.due_at),
        reviewed.cards.first().map(|card| card.scheduler.due_at)
    );
    Ok(())
}

#[tokio::test]
async fn source_tombstone_reimport_selector_search_and_recall_history_persist() -> Result<()> {
    let pool = tests::pool().await?;
    let program = active_program(&pool).await?;
    let sources = LearningSourceLibraryRepository::new(pool.clone());
    let source_id = id();
    let first_version = id();
    sources
        .add(
            &id(),
            &source_id,
            &first_version,
            &program.summary.id,
            crate::features::learning::dto::LearningSourceKind::Pasted,
            "add_text",
            "My text",
            None,
            crate::features::learning::dto::LearningSourcePolicy::Fixed,
            CapturedLearningSource {
                title: "Source title".into(),
                publisher: None,
                requested_url: None,
                resolved_url: None,
                text: "A prefix 🦀 repeated rule. A second 🦀 repeated rule. Recall safely.".into(),
                truncated: false,
                extraction_version: "test_v1".into(),
            },
            "first-source".into(),
        )
        .await?;

    let recall = LearningRecallRepository::new(pool.clone());
    let selector_text = "🦀 repeated rule";
    let text = "A prefix 🦀 repeated rule. A second 🦀 repeated rule. Recall safely.";
    let start = text.rfind(selector_text).unwrap_or_default();
    let selector_id = id();
    let selector_req = CreateLearningSourceSelectorRequestDto {
        operation_id: id(),
        selector_id: selector_id.clone(),
        program_id: program.summary.id.clone(),
        source_id: source_id.clone(),
        source_version_id: first_version.clone(),
        start_byte: start,
        end_byte: start + selector_text.len(),
    };
    let created = recall.create_selector(&selector_req).await?;
    assert_eq!(created.candidate_count, 2);
    assert_eq!(
        created.match_status,
        LearningQuoteMatchStatus::ContextDisambiguated
    );
    let loaded = recall
        .get_selector(&program.summary.id, &selector_id)
        .await?;
    assert_eq!(loaded.match_status, created.match_status);
    assert_eq!(loaded.candidate_count, created.candidate_count);
    assert_eq!(loaded.start_byte, created.start_byte);
    assert_eq!(loaded.end_byte, created.end_byte);
    let matched = recall
        .match_selector(&MatchLearningSourceSelectorRequestDto {
            program_id: program.summary.id.clone(),
            selector_id: selector_id.clone(),
            target_version_id: first_version.clone(),
        })
        .await?;
    assert_eq!(matched.start_byte, Some(start));
    let other = active_program(&pool).await?;
    assert!(recall
        .get_selector(&other.summary.id, &selector_id)
        .await
        .is_err());
    let degraded_embedder = crate::application::ports::MockEmbeddingPort::new_degraded();
    let search = recall
        .search_sources(
            &SearchLearningSourcesSemanticallyRequestDto {
                program_id: program.summary.id.clone(),
                query: "repeated rule".into(),
                limit: 10,
            },
            Some(&degraded_embedder),
        )
        .await?;
    assert!(!search.is_empty());
    assert_eq!(
        search.first().map(|item| item.retrieval_kind.as_str()),
        Some("lexical_fallback")
    );

    let deletion = DeleteLearningSourceRequestDto {
        operation_id: id(),
        program_id: program.summary.id.clone(),
        source_id: source_id.clone(),
        expected_revision: 0,
        reason: "No longer using this source".into(),
    };
    sources.delete_source(&deletion).await?;
    assert!(sources
        .workspace(&program.summary.id)
        .await?
        .sources
        .iter()
        .all(|source| source.id != source_id));
    let old = sources
        .get_version(&GetLearningSourceVersionRequestDto {
            program_id: program.summary.id.clone(),
            source_id: source_id.clone(),
            version_id: first_version.clone(),
        })
        .await?;
    assert!(old.full_text.contains("Recall safely"));

    let new_version = id();
    sources
        .reimport_source(&ReimportLearningSourceRequestDto {
            operation_id: id(),
            program_id: program.summary.id.clone(),
            source_id: source_id.clone(),
            version_id: new_version.clone(),
            expected_revision: 1,
            replacement_text: Some("Fresh replacement body.".into()),
        })
        .await?;
    let workspace = sources.workspace(&program.summary.id).await?;
    let reimported = workspace
        .sources
        .iter()
        .find(|source| source.id == source_id)
        .expect("reimported source is visible again");
    assert_eq!(
        reimported.active_version_id.as_deref(),
        Some(new_version.as_str())
    );
    assert_eq!(reimported.versions.len(), 2);
    let fallback = recall
        .search_sources(
            &SearchLearningSourcesSemanticallyRequestDto {
                program_id: program.summary.id.clone(),
                query: "replacement body".into(),
                limit: 10,
            },
            None,
        )
        .await?;
    assert_eq!(
        fallback.first().map(|item| item.version.id.as_str()),
        Some(new_version.as_str())
    );

    let card_id = id();
    let card = SaveLearningRecallCardRequestDto {
        operation_id: id(),
        program_id: program.summary.id.clone(),
        card_id: card_id.clone(),
        expected_content_revision: None,
        format: LearningRecallCardFormat::QuestionAnswer,
        content: LearningRecallContentDto {
            prompt: "Explain the repeated rule.".into(),
            answer: "It is a stable rule.".into(),
            explanation: String::new(),
            options: vec![],
            correct_option_index: None,
            language: None,
            cloze_deletions: vec![],
        },
        source_version_ids: vec![new_version.clone()],
        change_reason: "Manual recall".into(),
    };
    recall.save_card(&card).await?;
    let mut edit = card.clone();
    edit.operation_id = id();
    edit.expected_content_revision = Some(1);
    edit.content.answer = "A changed answer.".into();
    edit.change_reason = "Clarified answer".into();
    recall.save_card(&edit).await?;
    let card_item = recall
        .workspace(&program.summary.id)
        .await?
        .cards
        .into_iter()
        .find(|item| item.id == card_id);
    assert_eq!(card_item.as_ref().map(|item| item.versions.len()), Some(2));
    assert_eq!(
        card_item.and_then(|item| item.source_version_ids.first().cloned()),
        Some(new_version)
    );

    let mut similar_card = card.clone();
    similar_card.operation_id = id();
    similar_card.card_id = id();
    similar_card.content.prompt = "Explain the repeated rule carefully.".into();
    similar_card.content.answer = "The rule remains stable.".into();
    similar_card.change_reason = "Add related recall prompt".into();
    recall.save_card(&similar_card).await?;
    let suggestions = recall.workspace(&program.summary.id).await?.duplicates;
    let suggestion = suggestions
        .iter()
        .find(|item| item.status == LearningRecallDuplicateStatus::Pending)
        .ok_or_else(|| {
            crate::shared::error::AppError::InvalidState("Expected a duplicate suggestion".into())
        })?;
    let decide = DecideLearningRecallDuplicateRequestDto {
        operation_id: id(),
        program_id: program.summary.id.clone(),
        suggestion_id: suggestion.id.clone(),
        accept: false,
    };
    let decided = recall.decide_duplicate(&decide).await?;
    assert!(decided.duplicates.iter().any(|item| {
        item.id == decide.suggestion_id && item.status == LearningRecallDuplicateStatus::Dismissed
    }));
    let replay = recall.decide_duplicate(&decide).await?;
    assert!(replay
        .duplicates
        .iter()
        .any(|item| item.id == decide.suggestion_id));
    let mut changed_decision = decide.clone();
    changed_decision.accept = true;
    assert!(recall.decide_duplicate(&changed_decision).await.is_err());
    Ok(())
}

#[tokio::test]
async fn legacy_study_cards_and_review_history_bootstrap_into_recall() -> Result<()> {
    let pool = tests::pool().await?;
    let program = active_program(&pool).await?;
    let memory = LearningRepository::new(pool.clone());
    let lesson = program
        .modules
        .first()
        .and_then(|module| module.lessons.first())
        .ok_or_else(|| {
            crate::shared::error::AppError::InvalidState("Fixture has no lesson".into())
        })?;
    memory
        .ensure_lesson_note(&program.summary.id, &lesson.id, "Notes", "")
        .await?;
    let deck_id: String =
        sqlx::query_scalar("SELECT deck_id FROM learning_memory WHERE program_id=?")
            .bind(&program.summary.id)
            .fetch_one(&pool)
            .await
            .map_err(|error| crate::shared::error::AppError::Database(error.to_string()))?;
    let card_id = id();
    sqlx::query("INSERT INTO study_cards(id,deck_id,question,answer,options_json,correct_index,explanation,source_json,topic,due_at,format,scheduler_version) VALUES(?,?,?,?,?,?,?,?,?,?,?,?)")
        .bind(&card_id).bind(&deck_id).bind("Legacy question").bind("Yes")
        .bind("[\"No\",\"Yes\"]").bind(1_i64).bind("Old note")
        .bind("{}").bind("legacy").bind(chrono::Utc::now().timestamp_millis())
        .bind("multiple_choice").bind("expanding_v1")
        .execute(&pool).await
        .map_err(|error| crate::shared::error::AppError::Database(error.to_string()))?;
    let review_id = id();
    sqlx::query("INSERT INTO study_reviews(id,card_id,reviewed_at,rating,mode,correct,selected_option,scheduler_version) VALUES(?,?,?,?,?,?,?,?)")
        .bind(&review_id).bind(&card_id).bind(100_i64).bind("good").bind("quiz")
        .bind(1_i64).bind(1_i64).bind("expanding_v1")
        .execute(&pool).await
        .map_err(|error| crate::shared::error::AppError::Database(error.to_string()))?;

    let workspace = LearningRecallRepository::new(pool.clone())
        .workspace(&program.summary.id)
        .await?;
    let card = workspace.cards.iter().find(|card| card.id == card_id);
    assert_eq!(card.map(|card| card.scheduler.review_count), Some(1));
    assert_eq!(card.map(|card| card.versions.len()), Some(1));
    let preserved: i64 = sqlx::query_scalar("SELECT count(*) FROM study_reviews WHERE id=?")
        .bind(review_id)
        .fetch_one(&pool)
        .await
        .map_err(|error| crate::shared::error::AppError::Database(error.to_string()))?;
    assert_eq!(preserved, 1);
    Ok(())
}

#[tokio::test]
async fn recall_card_and_immutable_version_survive_database_reopen() -> Result<()> {
    let directory = tempfile::tempdir()
        .map_err(|error| crate::shared::error::AppError::Database(error.to_string()))?;
    let path = directory.path().join("learning-recall.sqlite");
    let pool = file_pool(&path).await?;
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .map_err(|error| crate::shared::error::AppError::Database(error.to_string()))?;
    let program = active_program(&pool).await?;
    let lesson = program
        .modules
        .first()
        .and_then(|module| module.lessons.first())
        .ok_or_else(|| {
            crate::shared::error::AppError::InvalidState("Fixture has no lesson".into())
        })?;
    LearningRepository::new(pool.clone())
        .ensure_lesson_note(&program.summary.id, &lesson.id, "Notes", "")
        .await?;
    let card_id = id();
    LearningRecallRepository::new(pool.clone())
        .save_card(&SaveLearningRecallCardRequestDto {
            operation_id: id(),
            program_id: program.summary.id.clone(),
            card_id: card_id.clone(),
            expected_content_revision: None,
            format: LearningRecallCardFormat::QuestionAnswer,
            content: LearningRecallContentDto {
                prompt: "Persisted question".into(),
                answer: "Persisted answer".into(),
                explanation: String::new(),
                options: vec![],
                correct_option_index: None,
                language: None,
                cloze_deletions: vec![],
            },
            source_version_ids: vec![],
            change_reason: "Initial card".into(),
        })
        .await?;
    pool.close().await;

    let reopened = file_pool(&path).await?;
    sqlx::migrate!("./migrations")
        .run(&reopened)
        .await
        .map_err(|error| crate::shared::error::AppError::Database(error.to_string()))?;
    let workspace = LearningRecallRepository::new(reopened)
        .workspace(&program.summary.id)
        .await?;
    let saved = workspace.cards.iter().find(|card| card.id == card_id);
    assert_eq!(
        saved.map(|card| card.content.prompt.as_str()),
        Some("Persisted question")
    );
    assert_eq!(saved.map(|card| card.versions.len()), Some(1));
    Ok(())
}

#[tokio::test]
async fn typed_recall_formats_round_trip_through_canonical_study_rows() -> Result<()> {
    let pool = tests::pool().await?;
    let program = active_program(&pool).await?;
    let lesson = program
        .modules
        .first()
        .and_then(|module| module.lessons.first())
        .ok_or_else(|| {
            crate::shared::error::AppError::InvalidState("Fixture has no lesson".into())
        })?;
    LearningRepository::new(pool.clone())
        .ensure_lesson_note(&program.summary.id, &lesson.id, "Notes", "")
        .await?;
    let formats = [
        (
            LearningRecallCardFormat::Cloze,
            LearningRecallContentDto {
                prompt: "A glacier moves slowly.".into(),
                answer: "glacier".into(),
                explanation: String::new(),
                options: vec![],
                correct_option_index: None,
                language: None,
                cloze_deletions: vec!["glacier".into()],
            },
        ),
        (
            LearningRecallCardFormat::Reverse,
            LearningRecallContentDto {
                prompt: "Definition: a body of ice that moves slowly.".into(),
                answer: "glacier".into(),
                explanation: String::new(),
                options: vec![],
                correct_option_index: None,
                language: None,
                cloze_deletions: vec![],
            },
        ),
        (
            LearningRecallCardFormat::CodePrediction,
            LearningRecallContentDto {
                prompt: "What does `Vec::new()` create?".into(),
                answer: "An empty vector.".into(),
                explanation: String::new(),
                options: vec![],
                correct_option_index: None,
                language: Some("Rust".into()),
                cloze_deletions: vec![],
            },
        ),
        (
            LearningRecallCardFormat::Reconstruction,
            LearningRecallContentDto {
                prompt: "Reconstruct the rule from memory.".into(),
                answer: "State the rule in your own words.".into(),
                explanation: String::new(),
                options: vec![],
                correct_option_index: None,
                language: None,
                cloze_deletions: vec![],
            },
        ),
    ];
    let recall = LearningRecallRepository::new(pool.clone());
    let mut saved = Vec::new();
    for (format, content) in formats {
        let card_id = id();
        recall
            .save_card(&SaveLearningRecallCardRequestDto {
                operation_id: id(),
                program_id: program.summary.id.clone(),
                card_id: card_id.clone(),
                expected_content_revision: None,
                format,
                content,
                source_version_ids: vec![],
                change_reason: "Format round trip".into(),
            })
            .await?;
        saved.push(card_id);
    }
    let workspace = recall.workspace(&program.summary.id).await?;
    for card_id in saved {
        assert!(workspace.cards.iter().any(|card| card.id == card_id));
    }
    assert!(workspace.cards.iter().any(|card| {
        card.format == LearningRecallCardFormat::CodePrediction
            && card.content.language.as_deref() == Some("Rust")
    }));
    Ok(())
}

#[test]
fn recall_content_rejects_invalid_keys_and_format_metadata() {
    let mut content = LearningRecallContentDto {
        prompt: "A prompt with deletion".into(),
        answer: "expected".into(),
        explanation: String::new(),
        options: vec!["A".into(), "B".into()],
        correct_option_index: Some(0),
        language: None,
        cloze_deletions: vec![],
    };
    assert!(
        crate::features::learning::recall_repository::validate_content(
            LearningRecallCardFormat::MultipleChoice,
            &content
        )
        .is_err()
    );
    content.options = vec![];
    content.correct_option_index = None;
    content.cloze_deletions = vec!["deletion".into(), "deletion".into()];
    assert!(
        crate::features::learning::recall_repository::validate_content(
            LearningRecallCardFormat::Cloze,
            &content
        )
        .is_err()
    );
    content.cloze_deletions = vec![];
    content.language = Some("rust".into());
    assert!(
        crate::features::learning::recall_repository::validate_content(
            LearningRecallCardFormat::QuestionAnswer,
            &content
        )
        .is_err()
    );
}

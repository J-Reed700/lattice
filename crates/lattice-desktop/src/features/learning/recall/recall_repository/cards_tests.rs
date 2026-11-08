//! Guards on saving, editing and reviewing versioned recall cards.
use super::*;
use crate::features::learning::{
    dto::{
        AcceptLearningProgramRequestDto, LearningProgramDto, LearningSourceKind,
        LearningSourcePolicy,
    },
    recall::study_repository::StudyRepository,
    repository::LearningRepository,
    source_library::{CapturedLearningSource, NewLearningSource},
    tests,
};

fn id() -> String {
    uuid::Uuid::new_v4().to_string()
}

#[track_caller]
fn assert_invalid<T: std::fmt::Debug>(result: Result<T>) {
    assert!(
        matches!(result, Err(AppError::InvalidInput(_))),
        "expected InvalidInput, got {result:?}"
    );
}

#[track_caller]
fn assert_not_found<T: std::fmt::Debug>(result: Result<T>) {
    assert!(
        matches!(result, Err(AppError::NotFound(_))),
        "expected NotFound, got {result:?}"
    );
}

async fn active_program(pool: &SqlitePool) -> Result<LearningProgramDto> {
    let repo = LearningRepository::new(pool.clone());
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

fn free_answer() -> LearningRecallContentDto {
    LearningRecallContentDto {
        prompt: "In what year did the Battle of Hastings take place?".into(),
        answer: "1066".into(),
        explanation: "William of Normandy defeated Harold II in October 1066.".into(),
        options: vec![],
        correct_option_index: None,
        language: None,
        cloze_deletions: vec![],
    }
}

fn choices() -> LearningRecallContentDto {
    LearningRecallContentDto {
        options: vec!["1066".into(), "1215".into(), "1415".into()],
        correct_option_index: Some(0),
        ..free_answer()
    }
}

fn save(
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
        change_reason: "First draft".into(),
    }
}

fn review(
    program_id: &str,
    card_id: &str,
    selected_option: Option<usize>,
) -> ReviewLearningRecallCardRequestDto {
    ReviewLearningRecallCardRequestDto {
        review_id: id(),
        program_id: program_id.into(),
        card_id: card_id.into(),
        expected_review_count: 0,
        rating: StudyRating::Good,
        selected_option,
    }
}

async fn add_version(
    pool: &SqlitePool,
    program_id: &str,
    kind: LearningSourceKind,
) -> Result<(String, String)> {
    let (op, source_id, version_id) = (id(), id(), id());
    let web = kind == LearningSourceKind::Web;
    LearningSourceLibraryRepository::new(pool.clone())
        .add(
            NewLearningSource {
                operation_id: &op,
                source_id: &source_id,
                version_id: &version_id,
                program_id,
                kind,
                operation_kind: if web { "add_web" } else { "add_text" },
                origin: if web {
                    "https://example.org/hastings"
                } else {
                    "Chronicle notes"
                },
                requested_url: web.then_some("https://example.org/hastings"),
                freshness: if web {
                    LearningSourcePolicy::Manual
                } else {
                    LearningSourcePolicy::Fixed
                },
            },
            CapturedLearningSource {
                title: "The Norman Conquest".into(),
                publisher: None,
                requested_url: web.then(|| "https://example.org/hastings".to_owned()),
                resolved_url: web.then(|| "https://example.org/hastings/1066".to_owned()),
                text: "The Battle of Hastings was fought on 14 October 1066.".into(),
                truncated: false,
                extraction_version: "test_v1".into(),
                signals: crate::features::learning::references::trust::capture::not_extracted(
                    "test fixture",
                ),
            },
            id(),
        )
        .await?;
    Ok((source_id, version_id))
}

async fn deck_card(pool: &SqlitePool, program_id: &str, card_id: &str) -> Result<StudyCardDto> {
    let deck_id = LearningRepository::new(pool.clone())
        .memory_state(program_id)
        .await?
        .deck_id
        .ok_or_else(|| AppError::NotFound("memory deck".into()))?;
    StudyRepository::new(pool.clone())
        .get(&deck_id)
        .await?
        .cards
        .into_iter()
        .find(|card| card.id == card_id)
        .ok_or_else(|| AppError::NotFound("study card".into()))
}

#[tokio::test]
async fn malformed_or_foreign_card_saves_are_rejected_without_writing() -> Result<()> {
    let pool = tests::pool().await?;
    let program = active_program(&pool).await?;
    let pid = program.summary.id.clone();
    let recall = LearningRecallRepository::new(pool.clone());
    let qa = LearningRecallCardFormat::QuestionAnswer;

    let mut too_many = save(&pid, qa, free_answer());
    too_many.source_version_ids = (0..21).map(|_| id()).collect();
    assert_invalid(recall.save_card(&too_many).await);
    let mut repeated = save(&pid, qa, free_answer());
    let version = id();
    repeated.source_version_ids = vec![version.clone(), version];
    assert_invalid(recall.save_card(&repeated).await);
    let mut blank_reason = save(&pid, qa, free_answer());
    blank_reason.change_reason = "   ".into();
    assert_invalid(recall.save_card(&blank_reason).await);
    let mut long_reason = save(&pid, qa, free_answer());
    long_reason.change_reason = "r".repeat(501);
    assert_invalid(recall.save_card(&long_reason).await);
    let mut bad_card = save(&pid, qa, free_answer());
    bad_card.card_id = "not-a-uuid".into();
    assert_invalid(recall.save_card(&bad_card).await);
    assert_invalid(recall.save_card(&save(&pid, qa, choices())).await);
    let mut wrong_key = choices();
    wrong_key.answer = "1215".into();
    assert_invalid(
        recall
            .save_card(&save(
                &pid,
                LearningRecallCardFormat::MultipleChoice,
                wrong_key,
            ))
            .await,
    );

    let mut unowned = save(&pid, qa, free_answer());
    unowned.source_version_ids = vec![id()];
    assert_invalid(recall.save_card(&unowned).await);
    let mut phantom_edit = save(&pid, qa, free_answer());
    phantom_edit.expected_content_revision = Some(1);
    assert_not_found(recall.save_card(&phantom_edit).await);

    let draft = tests::fixture();
    LearningRepository::new(pool.clone()).create(&draft).await?;
    assert_not_found(
        recall
            .save_card(&save(&draft.summary.id, qa, free_answer()))
            .await,
    );

    let study_card_id = {
        let deck_id = id();
        let card = StudyCardDto {
            id: id(),
            format: StudyCardFormat::QuestionAnswer,
            deck_id: deck_id.clone(),
            question: "Who won at Hastings?".into(),
            answer: "William".into(),
            options: vec![],
            correct_index: 0,
            explanation: String::new(),
            source: StudySourceDto {
                chunk_id: "c".into(),
                document_id: String::new(),
                file_name: "notes".into(),
                file_path: String::new(),
                excerpt: "William won.".into(),
                url: None,
            },
            citations: vec![],
            topic: "History".into(),
            due_at: 0,
            interval_days: 0,
            review_count: 0,
            lapses: 0,
        };
        let card_id = card.id.clone();
        StudyRepository::new(pool.clone())
            .save(&StudyDeckDto {
                id: deck_id,
                title: "Medieval England".into(),
                focus: "battles".into(),
                study_goal: String::new(),
                model_name: "fixture-model".into(),
                created_at: 1,
                target_at: None,
                cards: vec![card],
            })
            .await?;
        card_id
    };
    let mut taken = save(&pid, qa, free_answer());
    taken.card_id = study_card_id;
    assert_invalid(recall.save_card(&taken).await);

    assert!(recall.workspace(&pid).await?.cards.is_empty());
    Ok(())
}

#[tokio::test]
async fn edits_are_revision_guarded_versioned_and_mirrored_into_the_deck() -> Result<()> {
    let pool = tests::pool().await?;
    let program = active_program(&pool).await?;
    let pid = program.summary.id.clone();
    let recall = LearningRecallRepository::new(pool.clone());

    let first = save(
        &pid,
        LearningRecallCardFormat::QuestionAnswer,
        free_answer(),
    );
    recall.save_card(&first).await?;
    recall.save_card(&first).await?;
    let mut conflicting = first.clone();
    conflicting.change_reason = "Different reason".into();
    assert_invalid(recall.save_card(&conflicting).await);
    let saved = recall.workspace(&pid).await?;
    assert_eq!(saved.cards.len(), 1);
    assert_eq!(saved.cards[0].content_revision, 1);
    assert_eq!(saved.cards[0].versions.len(), 1);

    let edit = |expected: Option<i64>| SaveLearningRecallCardRequestDto {
        operation_id: id(),
        expected_content_revision: expected,
        format: LearningRecallCardFormat::MultipleChoice,
        content: choices(),
        change_reason: "Turn into a quiz".into(),
        ..first.clone()
    };
    assert_invalid(recall.save_card(&edit(None)).await);
    assert_invalid(recall.save_card(&edit(Some(0))).await);
    recall.save_card(&edit(Some(1))).await?;
    assert_invalid(recall.save_card(&edit(Some(1))).await);

    let edited = recall.workspace(&pid).await?;
    let card = &edited.cards[0];
    assert_eq!(card.content_revision, 2);
    assert_eq!(card.format, LearningRecallCardFormat::MultipleChoice);
    assert_eq!(card.versions.len(), 2);
    assert!(card
        .versions
        .iter()
        .any(|v| v.change_reason == "Turn into a quiz"));
    let mirrored = deck_card(&pool, &pid, &first.card_id).await?;
    assert_eq!(mirrored.format, StudyCardFormat::MultipleChoice);
    assert_eq!(mirrored.options, vec!["1066", "1215", "1415"]);
    assert_eq!(mirrored.answer, "1066");
    assert_eq!(mirrored.source.file_name, "Personal recall card");
    Ok(())
}

#[tokio::test]
async fn cited_versions_become_the_deck_citations() -> Result<()> {
    let pool = tests::pool().await?;
    let program = active_program(&pool).await?;
    let pid = program.summary.id.clone();
    let recall = LearningRecallRepository::new(pool.clone());
    let (web_source, web_version) = add_version(&pool, &pid, LearningSourceKind::Web).await?;
    let (text_source, text_version) = add_version(&pool, &pid, LearningSourceKind::Pasted).await?;

    let mut cited = save(
        &pid,
        LearningRecallCardFormat::QuestionAnswer,
        free_answer(),
    );
    cited.source_version_ids = vec![web_version.clone(), text_version.clone()];
    recall.save_card(&cited).await?;
    let card = deck_card(&pool, &pid, &cited.card_id).await?;
    assert_eq!(card.citations.len(), 2);
    let web = &card.citations[0];
    assert_eq!(web.chunk_id, web_version);
    assert_eq!(web.document_id, web_source);
    assert_eq!(web.file_name, "The Norman Conquest");
    assert_eq!(
        web.url.as_deref(),
        Some("https://example.org/hastings/1066")
    );
    let text = &card.citations[1];
    assert_eq!(text.chunk_id, text_version);
    assert_eq!(text.document_id, text_source);
    assert_eq!(text.url, None);
    assert_eq!(
        recall.workspace(&pid).await?.cards[0].source_version_ids,
        vec![web_version, text_version]
    );
    Ok(())
}

#[tokio::test]
async fn reviews_must_fit_the_card_format_and_the_program() -> Result<()> {
    let pool = tests::pool().await?;
    let program = active_program(&pool).await?;
    let pid = program.summary.id.clone();
    let recall = LearningRecallRepository::new(pool.clone());
    let free = save(
        &pid,
        LearningRecallCardFormat::QuestionAnswer,
        free_answer(),
    );
    let quiz = save(&pid, LearningRecallCardFormat::MultipleChoice, choices());
    recall.save_card(&free).await?;
    recall.save_card(&quiz).await?;

    assert_invalid(
        recall
            .review_card(&review(&pid, &free.card_id, Some(0)))
            .await,
    );
    assert_invalid(recall.review_card(&review(&pid, &quiz.card_id, None)).await);
    assert_invalid(
        recall
            .review_card(&review(&pid, &quiz.card_id, Some(3)))
            .await,
    );
    let mut negative = review(&pid, &free.card_id, None);
    negative.expected_review_count = -1;
    assert_invalid(recall.review_card(&negative).await);
    assert_not_found(recall.review_card(&review(&pid, &id(), None)).await);
    let other = active_program(&pool).await?;
    assert_not_found(
        recall
            .review_card(&review(&other.summary.id, &free.card_id, None))
            .await,
    );
    let mut bad_review = review(&pid, &free.card_id, None);
    bad_review.review_id = "not-a-uuid".into();
    assert_invalid(recall.review_card(&bad_review).await);

    recall
        .review_card(&review(&pid, &free.card_id, None))
        .await?;
    let reviewed = recall
        .review_card(&review(&pid, &quiz.card_id, Some(2)))
        .await?;
    let count = |card_id: &str| {
        reviewed
            .cards
            .iter()
            .find(|c| c.id == card_id)
            .map(|c| c.scheduler.review_count)
    };
    assert_eq!(count(&free.card_id), Some(1));
    assert_eq!(count(&quiz.card_id), Some(1));
    Ok(())
}

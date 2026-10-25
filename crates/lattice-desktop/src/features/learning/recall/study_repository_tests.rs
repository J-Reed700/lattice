//! Deck saves, card edits, listing and deletion in the Study repository.
use super::{study_dto::*, study_repository::StudyRepository};
use crate::features::learning::persistence::db;
use crate::features::learning::tests::pool;
use lattice_core::shared::error::{AppError, Result};
use sqlx::SqlitePool;

fn id() -> String {
    uuid::Uuid::new_v4().to_string()
}

async fn fk_pool() -> Result<SqlitePool> {
    let pool = pool().await?;
    sqlx::query("PRAGMA foreign_keys=ON")
        .execute(&pool)
        .await
        .map_err(db)?;
    Ok(pool)
}

fn source() -> StudySourceDto {
    StudySourceDto {
        chunk_id: "chunk-4".into(),
        document_id: id(),
        file_name: "italian-cooking.pdf".into(),
        file_path: "/library/italian-cooking.pdf".into(),
        excerpt: "Pesto Genovese is made from basil, pine nuts, garlic and cheese.".into(),
        url: None,
    }
}

fn card(
    deck_id: &str,
    format: StudyCardFormat,
    options: &[&str],
    correct_index: usize,
    due_at: i64,
) -> StudyCardDto {
    StudyCardDto {
        id: id(),
        format,
        deck_id: deck_id.into(),
        question: "Which herb is the base of pesto Genovese?".into(),
        answer: "Basil".into(),
        options: options.iter().map(|o| (*o).to_owned()).collect(),
        correct_index,
        explanation: "The passage lists basil first.".into(),
        source: source(),
        citations: vec![],
        topic: "Sauces".into(),
        due_at,
        interval_days: 0,
        review_count: 0,
        lapses: 0,
    }
}

fn deck(created_at: i64, cards: impl FnOnce(&str) -> Vec<StudyCardDto>) -> StudyDeckDto {
    let id = id();
    StudyDeckDto {
        cards: cards(&id),
        id,
        title: "Italian sauces".into(),
        focus: "pesto".into(),
        study_goal: "Cook regional sauces".into(),
        model_name: "fixture-model".into(),
        created_at,
    }
}

#[track_caller]
fn assert_invalid<T: std::fmt::Debug>(result: Result<T>) {
    assert!(
        matches!(result, Err(AppError::InvalidInput(_))),
        "expected InvalidInput, got {result:?}"
    );
}

#[tokio::test]
async fn a_deck_with_one_malformed_card_is_not_saved_at_all() -> Result<()> {
    let pool = fk_pool().await?;
    let repo = StudyRepository::new(pool.clone());
    let mc = StudyCardFormat::MultipleChoice;
    let qa = StudyCardFormat::QuestionAnswer;

    let out_of_range = deck(1_000, |d| {
        vec![
            card(d, mc, &["Basil", "Mint"], 0, 0),
            card(d, mc, &["Basil", "Mint"], 2, 0),
        ]
    });
    assert_invalid(repo.save(&out_of_range).await);
    assert!(matches!(
        repo.get(&out_of_range.id).await,
        Err(AppError::NotFound(_))
    ));
    let orphan_cards: i64 = sqlx::query_scalar("SELECT count(*) FROM study_cards WHERE deck_id=?")
        .bind(&out_of_range.id)
        .fetch_one(&pool)
        .await
        .map_err(db)?;
    assert_eq!(orphan_cards, 0);

    assert_invalid(
        repo.save(&deck(1_000, |d| vec![card(d, mc, &[], 0, 0)]))
            .await,
    );
    assert_invalid(
        repo.save(&deck(1_000, |d| vec![card(d, qa, &["Basil"], 0, 0)]))
            .await,
    );
    assert!(repo.list(0).await?.is_empty());
    Ok(())
}

#[tokio::test]
async fn decks_list_newest_first_with_due_counts_and_round_trip_citations() -> Result<()> {
    let pool = fk_pool().await?;
    let repo = StudyRepository::new(pool.clone());
    let mc = StudyCardFormat::MultipleChoice;
    let older = deck(1_000, |d| {
        vec![card(d, mc, &["Basil", "Mint", "Sage"], 0, 500)]
    });
    let newer = deck(2_000, |d| {
        vec![
            card(d, StudyCardFormat::QuestionAnswer, &[], 0, 100),
            card(d, mc, &["Basil", "Parsley"], 0, 9_000),
        ]
    });
    repo.save(&older).await?;
    repo.save(&newer).await?;

    let listed = repo.list(1_000).await?;
    assert_eq!(
        listed.iter().map(|d| d.id.as_str()).collect::<Vec<_>>(),
        vec![newer.id.as_str(), older.id.as_str()]
    );
    assert_eq!((listed[0].card_count, listed[0].due_count), (2, 1));
    assert_eq!((listed[1].card_count, listed[1].due_count), (1, 1));
    assert_eq!(listed[0].quiz_attempts, 0);
    assert_eq!(repo.list(10_000).await?[0].due_count, 2);

    let stored = repo.get(&newer.id).await?;
    assert_eq!(stored.cards.len(), 2);
    assert_eq!(
        stored.cards[0].id, newer.cards[0].id,
        "cards keep insertion order"
    );
    assert_eq!(stored.cards[0].format, StudyCardFormat::QuestionAnswer);
    assert_eq!(
        stored.cards[0].citations.len(),
        1,
        "the primary source becomes the citation"
    );
    assert_eq!(stored.cards[0].source.chunk_id, "chunk-4");
    assert_eq!(stored.study_goal, "Cook regional sauces");

    repo.delete_deck(&newer.id).await?;
    repo.delete_deck(&newer.id).await?;
    let cards: i64 = sqlx::query_scalar("SELECT count(*) FROM study_cards WHERE deck_id=?")
        .bind(&newer.id)
        .fetch_one(&pool)
        .await
        .map_err(db)?;
    assert_eq!(cards, 0, "deleting a deck removes its cards");
    assert_eq!(repo.list(0).await?.len(), 1);
    Ok(())
}

#[tokio::test]
async fn editing_keeps_choices_distinct_and_edits_free_answers_in_place() -> Result<()> {
    let pool = fk_pool().await?;
    let repo = StudyRepository::new(pool.clone());
    let saved = deck(1_000, |d| {
        vec![
            card(
                d,
                StudyCardFormat::MultipleChoice,
                &["Basil", "Mint", "Sage"],
                0,
                0,
            ),
            card(d, StudyCardFormat::QuestionAnswer, &[], 0, 0),
        ]
    });
    repo.save(&saved).await?;
    let (choice_id, free_id) = (saved.cards[0].id.clone(), saved.cards[1].id.clone());
    let edit = |card_id: &str, answer: &str| UpdateStudyCardRequestDto {
        card_id: card_id.into(),
        question: "  Which herb gives pesto its colour?  ".into(),
        answer: answer.into(),
        explanation: "  Basil leaves are bright green.  ".into(),
    };

    assert_invalid(repo.update_card(&edit(&choice_id, "  mint ")).await);
    // Re-entering the current correct answer in a new case is not a clash.
    repo.update_card(&edit(&choice_id, "BASIL")).await?;
    repo.update_card(&edit(&choice_id, "  Sweet basil  "))
        .await?;
    repo.update_card(&edit(&free_id, "  Fresh basil  ")).await?;
    let missing = repo.update_card(&edit(&id(), "Basil")).await;
    assert!(matches!(missing, Err(AppError::NotFound(_))), "{missing:?}");

    let stored = repo.get(&saved.id).await?;
    let choice = &stored.cards[0];
    assert_eq!(choice.options, vec!["Sweet basil", "Mint", "Sage"]);
    assert_eq!(choice.answer, "Sweet basil");
    assert_eq!(choice.question, "Which herb gives pesto its colour?");
    assert_eq!(choice.explanation, "Basil leaves are bright green.");
    let free = &stored.cards[1];
    assert_eq!(free.answer, "Fresh basil");
    assert!(free.options.is_empty());
    Ok(())
}

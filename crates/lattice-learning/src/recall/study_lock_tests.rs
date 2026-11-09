//! Study writes that read first must wait for another writer, not fail.
use super::{study_dto::*, study_generation::parse_cards, study_repository::StudyRepository};
use sqlx::{sqlite::SqlitePoolOptions, SqlitePool};
use std::time::Duration;

const HELD: Duration = Duration::from_millis(300);

fn source() -> StudySourceDto {
    StudySourceDto {
        chunk_id: "chunk".into(),
        document_id: uuid::Uuid::new_v4().to_string(),
        file_name: "chapter.pdf".into(),
        file_path: "/library/chapter.pdf".into(),
        excerpt: "A petition and an appeal address different procedural issues. Study the stated conditions before selecting a procedure.".into(),
        url: None,
    }
}

fn deck() -> StudyDeckDto {
    let id = uuid::Uuid::new_v4().to_string();
    let generated = serde_json::json!({"cards":[{"question":"What should be studied before selecting a procedure?","options":["The stated conditions","Only the title","Nothing","Only the filing date","Only the fee"],"correctIndex":0,"explanation":"The passage explicitly asks the reader to study the stated conditions.","sourceIndex":0,"quote":"Study the stated conditions before selecting a procedure.","topic":"Procedures"}]});
    StudyDeckDto {
        cards: parse_cards(&generated.to_string(), &[source()], &id, 6, 1000).unwrap(),
        id,
        title: "Procedures".into(),
        focus: "procedures".into(),
        study_goal: "Understand the process".into(),
        model_name: "test".into(),
        created_at: 1000,
        target_at: None,
    }
}

async fn open() -> (tempfile::TempDir, SqlitePool, StudyDeckDto) {
    let dir = tempfile::tempdir().unwrap();
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(crate::test_support::file_options(dir.path().join("t.db")))
        .await
        .unwrap();
    sqlx::migrate!("../lattice-desktop/migrations")
        .run(&pool)
        .await
        .unwrap();
    let deck = deck();
    StudyRepository::new(pool.clone())
        .save(&deck)
        .await
        .unwrap();
    (dir, pool, deck)
}

/// Another connection takes the write lock now and commits after `HELD`.
async fn another_writer(pool: &SqlitePool) -> tokio::task::JoinHandle<()> {
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await.unwrap();
    sqlx::query("INSERT INTO study_decks (id,title,focus,study_goal,model_name,created_at) VALUES ('other','t','f','g','m',1)")
        .execute(&mut *tx)
        .await
        .unwrap();
    tokio::spawn(async move {
        tokio::time::sleep(HELD).await;
        tx.commit().await.unwrap();
    })
}

#[tokio::test]
async fn setting_a_target_waits_for_a_writer_between_its_read_and_its_write() {
    let (_dir, pool, deck) = open().await;
    let other = another_writer(&pool).await;
    StudyRepository::new(pool.clone())
        .set_target(&deck.id, Some(5_000_000), 2000)
        .await
        .unwrap();
    other.await.unwrap();
}

#[tokio::test]
async fn editing_a_card_waits_for_a_writer_between_its_read_and_its_write() {
    let (_dir, pool, deck) = open().await;
    let other = another_writer(&pool).await;
    let card = deck.cards.first().unwrap();
    StudyRepository::new(pool.clone())
        .update_card(&UpdateStudyCardRequestDto {
            card_id: card.id.clone(),
            question: "What comes before choosing a procedure?".into(),
            answer: "The stated conditions".into(),
            explanation: "The passage says so.".into(),
        })
        .await
        .unwrap();
    other.await.unwrap();
}

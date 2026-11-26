//! Study writes that read first must wait for another writer, not fail.
use super::{study_dto::*, study_generation::ChoiceCard, study_repository::StudyRepository};
use lattice_core::shared::persistence::Database;
use sqlx::{sqlite::SqlitePoolOptions, SqlitePool};
use std::time::Duration;

const HELD: Duration = Duration::from_millis(300);

fn source() -> StudySourceDto {
    let excerpt = "A petition and an appeal address different procedural issues. Study the stated conditions before selecting a procedure.";
    StudySourceDto {
        chunk_id: "chunk".into(),
        document_id: uuid::Uuid::new_v4().to_string(),
        file_name: "chapter.pdf".into(),
        file_path: "/library/chapter.pdf".into(),
        excerpt: excerpt.into(),
        url: None,
        citation_id: 1,
        support: lattice_core::application::contracts::replies::SupportDto::cites([excerpt]),
    }
}

fn deck() -> StudyDeckDto {
    let id = uuid::Uuid::new_v4().to_string();
    let card = ChoiceCard {
        question: "What should be studied before selecting a procedure?",
        answer: "The stated conditions",
        distractors: [
            "Only the title",
            "Nothing",
            "Only the filing date",
            "Only the fee",
        ]
        .map(String::from)
        .to_vec(),
        explanation: "The passage explicitly asks the reader to study the stated conditions.",
        topic: "Procedures",
    }
    .into_card(&id, 1000, source(), vec![source()]);
    StudyDeckDto {
        cards: vec![card],
        id,
        title: "Procedures".into(),
        focus: "procedures".into(),
        study_goal: "Understand the process".into(),
        model_name: "test".into(),
        created_at: 1000,
        target_at: None,
    }
}

async fn open() -> (tempfile::TempDir, SqlitePool, Database, StudyDeckDto) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("t.db");
    lattice_core::shared::testing::sqlite::write_migrated(&path)
        .await
        .unwrap();
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(crate::test_support::file_options(path))
        .await
        .unwrap();
    let db = Database::new(pool.clone());
    let deck = deck();
    StudyRepository::new(db.clone()).save(&deck).await.unwrap();
    (dir, pool, db, deck)
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
    let (_dir, pool, db, deck) = open().await;
    let other = another_writer(&pool).await;
    StudyRepository::new(db.clone())
        .set_target(&deck.id, Some(5_000_000), 2000)
        .await
        .unwrap();
    other.await.unwrap();
}

#[tokio::test]
async fn editing_a_card_waits_for_a_writer_between_its_read_and_its_write() {
    use lattice_core::shared::edits::{ContentEditRequest, EditOp, EditSource};
    let (_dir, pool, db, deck) = open().await;
    let edits = crate::edits::content_edits(&db).unwrap();
    let card = deck.cards.first().unwrap();
    let opened = edits
        .open(&crate::edits::keys::card_ref(&card.id))
        .await
        .unwrap();
    let other = another_writer(&pool).await;
    edits
        .apply(&ContentEditRequest {
            edit_id: uuid::Uuid::new_v4().to_string(),
            content: crate::edits::keys::card_ref(&card.id),
            expected_revision: opened.revision,
            ops: vec![
                EditOp::Replace {
                    path: "/question".into(),
                    value: serde_json::json!("What comes before choosing a procedure?"),
                },
                EditOp::Replace {
                    path: "/choices/0/text".into(),
                    value: serde_json::json!("The procedural sequence"),
                },
                EditOp::Replace {
                    path: "/explanation".into(),
                    value: serde_json::json!("The passage says so."),
                },
            ],
            source: EditSource::Learner,
            target: None,
        })
        .await
        .unwrap();
    other.await.unwrap();
}

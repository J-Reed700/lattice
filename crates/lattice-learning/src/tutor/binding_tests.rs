//! Binding a chat reads the course and writes the chat: no other writer gets in between.
use super::binding::{bind_conversation, SetConversationCourseRequestDto};
use crate::dto::LearningProgramStatus;
use crate::repository::LearningRepository;
use crate::test_support::file_options;
use crate::tests::fixture;
use lattice_core::shared::error::AppError;
use lattice_core::shared::persistence::Database;
use sqlx::sqlite::SqlitePoolOptions;
use std::time::Duration;

const HELD: Duration = Duration::from_millis(300);

#[tokio::test]
async fn binding_decides_on_the_course_as_the_other_writer_left_it() {
    let dir = tempfile::tempdir().unwrap();
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(file_options(dir.path().join("t.db")))
        .await
        .unwrap();
    let db = Database::new(pool.clone());
    sqlx::migrate!("../lattice-desktop/migrations")
        .run(&pool)
        .await
        .unwrap();
    let mut program = fixture();
    program.summary.status = LearningProgramStatus::Active;
    LearningRepository::new(db.clone())
        .create(&program)
        .await
        .unwrap();
    let conversation = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO conversations (id,title) VALUES (?,'Tutor')")
        .bind(&conversation)
        .execute(&pool)
        .await
        .unwrap();

    let mut writer = pool.begin_with("BEGIN IMMEDIATE").await.unwrap();
    sqlx::query("UPDATE learning_programs SET status='draft' WHERE id=?")
        .bind(&program.summary.id)
        .execute(&mut *writer)
        .await
        .unwrap();
    let other = tokio::spawn(async move {
        tokio::time::sleep(HELD).await;
        writer.commit().await.unwrap();
    });
    let bound = bind_conversation(
        &db,
        &SetConversationCourseRequestDto {
            conversation_id: conversation.clone(),
            program_id: program.summary.id.clone(),
            lesson_id: None,
        },
    )
    .await;
    other.await.unwrap();

    assert!(
        matches!(bound, Err(AppError::InvalidInput(_))),
        "the course was taken back to a draft before the chat was bound: {bound:?}"
    );
    let program_id: Option<String> =
        sqlx::query_scalar("SELECT program_id FROM conversations WHERE id=?")
            .bind(&conversation)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(program_id, None);
}

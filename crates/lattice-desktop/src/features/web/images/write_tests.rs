//! A claim decides which pictures are free once the write lock is the
//! claimer's own.
use super::repository::{ImageRepository, NewImageRow};
use lattice_core::application::contracts::images::ImageOrigin;
use lattice_core::shared::persistence::Database;
use sqlx::sqlite::SqlitePoolOptions;
use std::time::Duration;

fn new_row(id: &str) -> NewImageRow {
    NewImageRow {
        id: id.to_string(),
        conversation_id: "conv-1".into(),
        request_id: "req-1".into(),
        ext: "png".into(),
        mime: "image/png".into(),
        sha256: "0".repeat(64),
        byte_size: 4,
        width: 2,
        height: 2,
        caption: "c".into(),
        source_url: Some("https://example.org".into()),
        source_title: None,
        image_url: Some(format!("https://example.org/{id}.png")),
        origin: ImageOrigin::Page,
        license: None,
        attribution: None,
        original_name: None,
    }
}

#[tokio::test]
async fn a_picture_claimed_by_another_message_meanwhile_leaves_the_numbering_unbroken() {
    let directory = tempfile::tempdir().unwrap();
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(crate::tests::sqlite::file_options(
            directory.path().join("images.db"),
        ))
        .await
        .unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    sqlx::query(
        "INSERT INTO conversations (id, title, space_id, created_at, updated_at) \
         VALUES ('conv-1', 'Pictures', 'space_general', '2026-10-17T00:00:00Z', '2026-10-17T00:00:00Z')",
    )
    .execute(&pool)
    .await
    .unwrap();
    for message in ["msg-claim", "msg-other"] {
        sqlx::query(
            "INSERT INTO conversation_messages (id, conversation_id, role, content) \
             VALUES (?, 'conv-1', 'assistant', 'hi')",
        )
        .bind(message)
        .execute(&pool)
        .await
        .unwrap();
    }
    let db = Database::new(pool.clone());
    let repo = ImageRepository::new(db);
    repo.insert(&new_row("img-a")).await.unwrap();
    repo.insert(&new_row("img-b")).await.unwrap();

    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await.unwrap();
    sqlx::query(
        "UPDATE chat_images SET message_id = 'msg-other', request_id = 'req-other', ref = 'att-1' \
         WHERE id = 'img-a'",
    )
    .execute(&mut *tx)
    .await
    .unwrap();
    let other = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(300)).await;
        tx.commit().await.unwrap();
    });

    let claimed = repo
        .claim(
            "conv-1",
            &["img-a".to_string(), "img-b".to_string()],
            "msg-claim",
            "req-claim",
        )
        .await
        .unwrap();

    other.await.unwrap();
    let refs: Vec<(String, String)> = claimed
        .into_iter()
        .map(|row| (row.id, row.image_ref))
        .collect();
    assert_eq!(refs, vec![("img-b".to_string(), "att-1".to_string())]);
}

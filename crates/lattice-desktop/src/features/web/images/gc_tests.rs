//! The startup sweep deletes only rows still unclaimed when it takes the
//! write lock.
use lattice_core::shared::persistence::Database;
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use lattice_core::application::ports::chat_images::{ChatImagesPort, ImageTurnScope};
use lattice_core::shared::persistence::Transactions;
use sqlx::SqlitePool;

use super::crash_tests::{png, NoNetwork};
use super::service::ChatImageService;

async fn fixture() -> (SqlitePool, Arc<ChatImageService>, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(crate::tests::sqlite::file_options(
            dir.path().join("images.db"),
        ))
        .await
        .unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    sqlx::query(
        "INSERT INTO conversations (id, title, space_id, created_at, updated_at) \
         VALUES ('conv-sweep', 'Pictures', 'space_general', '2026-10-17T00:00:00Z', '2026-10-17T00:00:00Z')",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO conversation_messages (id, conversation_id, role, content) \
         VALUES ('msg-sweep', 'conv-sweep', 'user', 'hi')",
    )
    .execute(&pool)
    .await
    .unwrap();
    let gate = Arc::new(lattice_core::shared::fs::SweepGate::default());
    gate.open();
    let service = Arc::new(ChatImageService::new(
        Database::from(pool.clone()),
        dir.path().join("chat-images"),
        Arc::new(NoNetwork),
        Arc::new(lattice_core::shared::runtime::background::BackgroundTasks::default()),
        gate,
    ));
    (pool, service, dir)
}

#[tokio::test]
async fn a_picture_attached_while_the_sweep_waits_for_the_write_lock_is_kept() {
    let (pool, service, dir) = fixture().await;
    let scope = ImageTurnScope {
        conversation_id: "conv-sweep".into(),
        request_id: "turn-sweep".into(),
    };
    let image = service
        .store_attachment(&scope, "heron.png", png())
        .await
        .unwrap();

    let mut tx = pool.begin_write().await.unwrap();
    let sweeping = {
        let service = service.clone();
        tokio::spawn(async move { service.sweep(&HashSet::new()).await })
    };
    tokio::time::sleep(Duration::from_millis(300)).await;
    sqlx::query("UPDATE chat_images SET message_id = 'msg-sweep' WHERE id = ?")
        .bind(&image.id)
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    sweeping.await.unwrap().unwrap();

    let held: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM chat_images WHERE id = ?")
        .bind(&image.id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(held, 1, "the attached picture's row survives the sweep");
    assert!(dir
        .path()
        .join("chat-images")
        .join(&image.file_name)
        .exists());
}

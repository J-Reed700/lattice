//! A picture's row and its file are written in an order a failure between
//! them leaves clean: the row first, taken back when the file cannot follow.
use lattice_core::shared::persistence::Database;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use reqwest::header::HeaderMap;
use sqlx::SqlitePool;

use super::fetch::ImageNetwork;
use super::service::ChatImageService;
use crate::features::web::services::web::{BoundedBody, BoundedFetchError};
use lattice_core::application::ports::chat_images::{
    ChatImagesPort, CopiedMessage, ImageTurnScope,
};

pub(super) struct NoNetwork;

#[async_trait]
impl ImageNetwork for NoNetwork {
    async fn get(
        &self,
        _url: &str,
        _headers: HeaderMap,
        _max_bytes: usize,
        _deadline: Option<Instant>,
    ) -> Result<BoundedBody, BoundedFetchError> {
        Err(BoundedFetchError::Status(404))
    }
}

pub(super) fn png() -> Vec<u8> {
    let mut bytes = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    bytes.extend_from_slice(&13u32.to_be_bytes());
    bytes.extend_from_slice(b"IHDR");
    bytes.extend_from_slice(&8u32.to_be_bytes());
    bytes.extend_from_slice(&8u32.to_be_bytes());
    bytes.extend_from_slice(&[8, 2, 0, 0, 0, 0, 0, 0, 0]);
    bytes
}

async fn fixture() -> (SqlitePool, Arc<ChatImageService>, tempfile::TempDir) {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .idle_timeout(None)
        .max_lifetime(None)
        .connect(":memory:")
        .await
        .unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    for conversation in ["conv-crash", "conv-copy"] {
        sqlx::query(
            "INSERT INTO conversations (id, title, space_id, created_at, updated_at) \
             VALUES (?, 'Pictures', 'space_general', '2026-10-17T00:00:00Z', '2026-10-17T00:00:00Z')",
        )
        .bind(conversation)
        .execute(&pool)
        .await
        .unwrap();
    }
    sqlx::query(
        "INSERT INTO conversation_messages (id, conversation_id, role, content) \
         VALUES ('msg-copy', 'conv-copy', 'assistant', 'hi')",
    )
    .execute(&pool)
    .await
    .unwrap();
    let dir = tempfile::tempdir().unwrap();
    let sweep_gate = Arc::new(lattice_core::shared::fs::SweepGate::default());
    sweep_gate.open();
    let service = Arc::new(ChatImageService::new(
        Database::from(pool.clone()),
        dir.path().join("chat-images"),
        Arc::new(NoNetwork),
        Arc::new(lattice_core::shared::runtime::background::BackgroundTasks::default()),
        sweep_gate,
    ));
    (pool, service, dir)
}

async fn rows(pool: &SqlitePool) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM chat_images")
        .fetch_one(pool)
        .await
        .unwrap()
}

fn scope() -> ImageTurnScope {
    ImageTurnScope {
        conversation_id: "conv-crash".into(),
        request_id: "draft:crash".into(),
    }
}

#[tokio::test]
async fn a_picture_whose_file_cannot_be_written_leaves_no_row() {
    let (pool, service, dir) = fixture().await;
    std::fs::write(dir.path().join("chat-images"), b"in the way").unwrap();

    let result = service.store_attachment(&scope(), "heron.png", png()).await;

    assert!(result.is_err());
    assert_eq!(rows(&pool).await, 0);
}

#[tokio::test]
async fn a_copy_whose_file_cannot_follow_its_row_leaves_only_the_original() {
    let (pool, service, dir) = fixture().await;
    let original = service
        .store_attachment(&scope(), "heron.png", png())
        .await
        .unwrap();
    std::fs::remove_file(dir.path().join("chat-images").join(&original.file_name)).unwrap();

    let copied = service
        .copy_for_messages(
            "conv-copy",
            &[CopiedMessage {
                message_id: "msg-copy".into(),
                image_ids: vec![original.id.clone()],
            }],
        )
        .await
        .unwrap();

    assert_eq!(copied.len(), 1);
    assert!(copied.first().is_some_and(|(_, images)| images.is_empty()));
    assert_eq!(rows(&pool).await, 1);
}

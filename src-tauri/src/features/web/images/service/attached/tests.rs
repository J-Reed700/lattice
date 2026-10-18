use std::collections::HashSet;
use std::sync::Arc;

use async_trait::async_trait;
use image::codecs::png::PngEncoder;
use image::{ExtendedColorType, ImageEncoder, Rgb, RgbImage};
use lattice_core::application::contracts::images::ImageOrigin;
use lattice_core::application::ports::chat_images::{ChatImagesPort, ImageTurnScope};
use lattice_core::domain::models::vision::VisionGeometry;
use reqwest::header::HeaderMap;
use sqlx::SqlitePool;
use tempfile::TempDir;

use super::*;
use crate::features::web::images::fetch::ImageNetwork;
use crate::features::web::services::web::{BoundedBody, BoundedFetchError};

const CONVERSATION: &str = "conv-attached";
const OTHER: &str = "conv-other";
const MESSAGE: &str = "msg-question";
const GRID_28: VisionGeometry = VisionGeometry::CONSERVATIVE_DEFAULT;

/// Attaching never touches the network.
struct NoNetwork;

#[async_trait]
impl ImageNetwork for NoNetwork {
    async fn get(
        &self,
        url: &str,
        _headers: HeaderMap,
        _max_bytes: usize,
        _deadline: Option<std::time::Instant>,
    ) -> std::result::Result<BoundedBody, BoundedFetchError> {
        panic!("attaching fetched {url}")
    }
}

struct Fixture {
    pool: SqlitePool,
    dir: TempDir,
    service: Arc<ChatImageService>,
}

impl Fixture {
    fn root(&self) -> std::path::PathBuf {
        self.dir.path().join("chat-images")
    }

    fn draft(&self, conversation: &str, draft: &str) -> ImageTurnScope {
        ImageTurnScope {
            conversation_id: conversation.into(),
            request_id: format!("draft:{draft}"),
        }
    }

    fn prepared_files(&self) -> Vec<String> {
        std::fs::read_dir(self.root().join("prepared"))
            .map(|entries| {
                let mut names: Vec<String> = entries
                    .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
                    .collect();
                names.sort();
                names
            })
            .unwrap_or_default()
    }
}

async fn fixture() -> Fixture {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .idle_timeout(None)
        .max_lifetime(None)
        .connect(":memory:")
        .await
        .unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    for conversation in [CONVERSATION, OTHER] {
        sqlx::query(
            "INSERT INTO conversations (id, title, space_id, created_at, updated_at) \
             VALUES (?, 'Pictures', 'space_general', '2026-10-18T00:00:00Z', '2026-10-18T00:00:00Z')",
        )
        .bind(conversation)
        .execute(&pool)
        .await
        .unwrap();
    }
    sqlx::query(
        "INSERT INTO conversation_messages (id, conversation_id, role, content) \
         VALUES (?, ?, 'user', 'What is in this picture?')",
    )
    .bind(MESSAGE)
    .bind(CONVERSATION)
    .execute(&pool)
    .await
    .unwrap();
    let dir = TempDir::new().unwrap();
    let service = Arc::new(ChatImageService::new(
        pool.clone(),
        dir.path().join("chat-images"),
        Arc::new(NoNetwork),
        Arc::new(lattice_core::shared::runtime::background::BackgroundTasks::default()),
    ));
    Fixture { pool, dir, service }
}

fn png(width: u32, height: u32) -> Vec<u8> {
    let pixels = RgbImage::from_fn(width, height, |x, y| {
        Rgb([(x % 256) as u8, (y % 256) as u8, 90])
    });
    let mut bytes = Vec::new();
    PngEncoder::new(&mut bytes)
        .write_image(pixels.as_raw(), width, height, ExtendedColorType::Rgb8)
        .unwrap();
    bytes
}

#[tokio::test]
async fn an_attached_picture_is_a_draft_row_with_no_source() {
    let fx = fixture().await;
    let scope = fx.draft(CONVERSATION, "one");
    let first = fx
        .service
        .store_attachment(&scope, "Screen Shot.png", png(40, 30))
        .await
        .unwrap();
    let second = fx
        .service
        .store_attachment(&scope, "diagram.png", png(20, 20))
        .await
        .unwrap();

    assert_eq!(
        (first.r#ref.as_str(), second.r#ref.as_str()),
        ("att-1", "att-2")
    );
    assert_eq!(first.origin, ImageOrigin::Attachment);
    assert_eq!(first.source_url, None);
    assert_eq!(first.original_name.as_deref(), Some("Screen Shot.png"));
    assert_eq!((first.width, first.height), (40, 30));
    assert!(fx.root().join(&first.file_name).is_file());
    let row = fx
        .service
        .repository()
        .get(&first.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.image_url, None);
    assert_eq!(row.message_id, None);
    assert_eq!(row.request_id, "draft:one");
}

#[tokio::test]
async fn a_file_that_is_not_a_picture_is_refused_and_nothing_is_kept() {
    let fx = fixture().await;
    let error = fx
        .service
        .store_attachment(
            &fx.draft(CONVERSATION, "one"),
            "notes.txt",
            b"plain text".to_vec(),
        )
        .await
        .unwrap_err();
    assert!(error.to_string().contains("notes.txt"), "{error}");
    let rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM chat_images")
        .fetch_one(&fx.pool)
        .await
        .unwrap();
    assert_eq!(rows, 0);
}

#[tokio::test]
async fn rows_for_ids_keeps_the_order_given_and_drops_other_conversations() {
    let fx = fixture().await;
    let a = fx
        .service
        .store_attachment(&fx.draft(CONVERSATION, "one"), "a.png", png(8, 8))
        .await
        .unwrap();
    let b = fx
        .service
        .store_attachment(&fx.draft(CONVERSATION, "one"), "b.png", png(8, 8))
        .await
        .unwrap();
    let elsewhere = fx
        .service
        .store_attachment(&fx.draft(OTHER, "two"), "c.png", png(8, 8))
        .await
        .unwrap();

    let rows = fx
        .service
        .rows_for_ids(
            CONVERSATION,
            &[b.id.clone(), elsewhere.id.clone(), a.id.clone()],
        )
        .await
        .unwrap();
    let ids: Vec<&str> = rows.iter().map(|row| row.id.as_str()).collect();
    assert_eq!(ids, [b.id.as_str(), a.id.as_str()]);
}

#[tokio::test]
async fn a_message_claims_drafts_from_two_drops_in_order_and_again_idempotently() {
    let fx = fixture().await;
    // Two drops, two drafts, each numbering from att-1.
    let first = fx
        .service
        .store_attachment(&fx.draft(CONVERSATION, "one"), "a.png", png(8, 8))
        .await
        .unwrap();
    let second = fx
        .service
        .store_attachment(&fx.draft(CONVERSATION, "two"), "b.png", png(8, 8))
        .await
        .unwrap();
    let ids = vec![second.id.clone(), first.id.clone()];

    let claimed = fx
        .service
        .attach_ids_to_message(CONVERSATION, &ids, MESSAGE, "turn-1")
        .await
        .unwrap();
    let refs: Vec<(&str, &str)> = claimed
        .iter()
        .map(|row| (row.id.as_str(), row.r#ref.as_str()))
        .collect();
    assert_eq!(
        refs,
        [(second.id.as_str(), "att-1"), (first.id.as_str(), "att-2")]
    );
    let again = fx
        .service
        .attach_ids_to_message(CONVERSATION, &ids, MESSAGE, "turn-1")
        .await
        .unwrap();
    assert_eq!(again, claimed);
    let row = fx
        .service
        .repository()
        .get(&first.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.message_id.as_deref(), Some(MESSAGE));
    assert_eq!(row.request_id, "turn-1");

    // A claimed picture is the message's: releasing it as a draft does nothing.
    fx.service.release_images(CONVERSATION, &ids).await.unwrap();
    assert!(fx
        .service
        .repository()
        .get(&first.id)
        .await
        .unwrap()
        .is_some());
}

#[tokio::test]
async fn a_discarded_draft_is_released_with_its_file() {
    let fx = fixture().await;
    let picture = fx
        .service
        .store_attachment(&fx.draft(CONVERSATION, "one"), "a.png", png(8, 8))
        .await
        .unwrap();
    fx.service
        .release_images(CONVERSATION, std::slice::from_ref(&picture.id))
        .await
        .unwrap();
    assert!(fx
        .service
        .repository()
        .get(&picture.id)
        .await
        .unwrap()
        .is_none());
    assert!(!fx.root().join(&picture.file_name).exists());
}

#[tokio::test]
async fn restage_copies_the_picture_into_a_new_draft() {
    let fx = fixture().await;
    let picture = fx
        .service
        .store_attachment(&fx.draft(CONVERSATION, "one"), "a.png", png(8, 8))
        .await
        .unwrap();
    fx.service
        .attach_ids_to_message(
            CONVERSATION,
            std::slice::from_ref(&picture.id),
            MESSAGE,
            "turn-1",
        )
        .await
        .unwrap();

    let copy = fx.service.restage(CONVERSATION, &picture.id).await.unwrap();

    assert_ne!(copy.id, picture.id);
    assert_eq!(copy.r#ref, "att-1");
    assert_eq!(copy.original_name, picture.original_name);
    assert!(fx.root().join(&copy.file_name).is_file());
    let row = fx
        .service
        .repository()
        .get(&copy.id)
        .await
        .unwrap()
        .unwrap();
    assert!(row.request_id.starts_with("draft:"));
    assert_eq!(row.message_id, None);
    assert!(fx.service.restage(OTHER, &picture.id).await.is_err());
}

#[tokio::test]
async fn a_picture_is_prepared_once_per_size_and_then_read_from_the_cache() {
    let fx = fixture().await;
    let picture = fx
        .service
        .store_attachment(&fx.draft(CONVERSATION, "one"), "big.png", png(1600, 1200))
        .await
        .unwrap();
    let fit = GRID_28.plan(1600, 1200);

    let prepared = fx
        .service
        .prepared(&picture.id, GRID_28, None)
        .await
        .unwrap()
        .unwrap();
    assert_eq!((prepared.width, prepared.height), (fit.width, fit.height));
    assert_eq!(prepared.tokens, fit.tokens);
    assert_eq!(prepared.mime, "image/png");
    assert_eq!(
        fx.prepared_files(),
        [format!("{}-{}x{}.png", picture.id, fit.width, fit.height)]
    );

    // With the original gone, the same size still comes from the cache.
    std::fs::remove_file(fx.root().join(&picture.file_name)).unwrap();
    let cached = fx
        .service
        .prepared(&picture.id, GRID_28, None)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(cached, prepared);

    assert_eq!(
        fx.service
            .prepared(&"f".repeat(32), GRID_28, None)
            .await
            .unwrap(),
        None
    );
}

#[tokio::test]
async fn releasing_a_picture_removes_its_prepared_copies_and_the_sweep_takes_orphans() {
    let fx = fixture().await;
    let picture = fx
        .service
        .store_attachment(&fx.draft(CONVERSATION, "one"), "a.png", png(300, 200))
        .await
        .unwrap();
    fx.service
        .prepared(&picture.id, GRID_28, None)
        .await
        .unwrap()
        .unwrap();
    // A prepared file whose picture has no row, left by a crash.
    let orphan = format!("{}-28x28.jpg", "e".repeat(32));
    std::fs::write(fx.root().join("prepared").join(&orphan), b"x").unwrap();

    let removed = fx.service.remove_unclaimed_files().await.unwrap();
    assert_eq!(removed, 1);
    assert_eq!(fx.prepared_files().len(), 1);

    fx.service
        .release_images(CONVERSATION, std::slice::from_ref(&picture.id))
        .await
        .unwrap();
    assert!(fx.prepared_files().is_empty());
}

#[tokio::test]
async fn the_startup_sweep_keeps_drafts_staged_since_this_start_only() {
    let fx = fixture().await;
    let fresh = fx
        .service
        .store_attachment(&fx.draft(CONVERSATION, "open"), "a.png", png(8, 8))
        .await
        .unwrap();
    let stale = fx
        .service
        .store_attachment(&fx.draft(CONVERSATION, "closed"), "b.png", png(8, 8))
        .await
        .unwrap();
    sqlx::query("UPDATE chat_images SET created_at = '2000-01-01T00:00:00+00:00' WHERE id = ?")
        .bind(&stale.id)
        .execute(&fx.pool)
        .await
        .unwrap();

    let report = fx.service.sweep(&HashSet::new()).await.unwrap();

    assert_eq!(report.removed_leases, 1);
    assert!(fx
        .service
        .repository()
        .get(&fresh.id)
        .await
        .unwrap()
        .is_some());
    assert!(fx
        .service
        .repository()
        .get(&stale.id)
        .await
        .unwrap()
        .is_none());
}

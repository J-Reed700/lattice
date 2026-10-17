//! Fetch validation, Commons parsing, store, repository, service lease
//! lifecycle, sweep and protocol tests (§10). The network is a fake that
//! serves canned bodies; the guarded client is exercised only on requests
//! it refuses before sending anything.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use async_trait::async_trait;
use reqwest::header::{HeaderMap, USER_AGENT};
use sqlx::SqlitePool;
use tempfile::TempDir;

use super::commons::{parse_search, search_url, strip_html};
use super::fetch::{fetch_image, validate_image, ImageNetwork, ImageRejected, COMMONS_USER_AGENT};
use super::protocol::{respond, route, Route};
use super::repository::NewImageRow;
use super::store::{new_id, ImageStore, Verified};
use super::*;
use crate::features::web::services::web::{BoundedBody, BoundedFetchError, WebService};
use lattice_core::application::contracts::images::{FindImageInput, ImageOrigin, NO_PICTURE_NOTE};
use lattice_core::application::ports::chat_images::{
    ChatImagesPort, CopiedMessage, ImageTurnScope,
};

const CONVERSATION: &str = "conv-images";
const MESSAGE: &str = "msg-images";
const REQUEST: &str = "req-images";

// ---- image fixtures: headers only, which is all validation reads ----

fn png(width: u32, height: u32) -> Vec<u8> {
    let mut bytes = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    bytes.extend_from_slice(&13u32.to_be_bytes());
    bytes.extend_from_slice(b"IHDR");
    bytes.extend_from_slice(&width.to_be_bytes());
    bytes.extend_from_slice(&height.to_be_bytes());
    bytes.extend_from_slice(&[8, 2, 0, 0, 0, 0, 0, 0, 0]);
    bytes
}

fn gif(width: u16, height: u16) -> Vec<u8> {
    let mut bytes = b"GIF89a".to_vec();
    bytes.extend_from_slice(&width.to_le_bytes());
    bytes.extend_from_slice(&height.to_le_bytes());
    bytes.extend_from_slice(&[0, 0, 0, 0x3B]);
    bytes
}

fn jpeg(width: u16, height: u16) -> Vec<u8> {
    let mut bytes = vec![0xFF, 0xD8];
    bytes.extend_from_slice(&[
        0xFF, 0xE0, 0x00, 0x10, b'J', b'F', b'I', b'F', 0x00, 0x01, 0x01, 0x00, 0x00, 0x01, 0x00,
        0x01, 0x00, 0x00,
    ]);
    bytes.extend_from_slice(&[0xFF, 0xC0, 0x00, 0x11, 0x08]);
    bytes.extend_from_slice(&height.to_be_bytes());
    bytes.extend_from_slice(&width.to_be_bytes());
    bytes.extend_from_slice(&[0x03, 0x01, 0x22, 0x00, 0x02, 0x11, 0x01, 0x03, 0x11, 0x01]);
    bytes.extend_from_slice(&[0xFF, 0xD9]);
    bytes
}

fn webp(width: u32, height: u32) -> Vec<u8> {
    let mut chunk = b"VP8X".to_vec();
    chunk.extend_from_slice(&10u32.to_le_bytes());
    chunk.extend_from_slice(&[0, 0, 0, 0]);
    chunk.extend_from_slice(&(width - 1).to_le_bytes()[..3]);
    chunk.extend_from_slice(&(height - 1).to_le_bytes()[..3]);
    let mut bytes = b"RIFF".to_vec();
    bytes.extend_from_slice(&((chunk.len() + 4) as u32).to_le_bytes());
    bytes.extend_from_slice(b"WEBP");
    bytes.extend_from_slice(&chunk);
    bytes
}

// ---- fetch validation ----

#[test]
fn validate_accepts_each_stored_format_by_magic_bytes() {
    for (bytes, mime, ext, width, height) in [
        (png(640, 480), "image/png", "png", 640, 480),
        (gif(320, 200), "image/gif", "gif", 320, 200),
        (jpeg(1280, 720), "image/jpeg", "jpg", 1280, 720),
        (webp(800, 600), "image/webp", "webp", 800, 600),
    ] {
        let image = validate_image(bytes).unwrap();
        assert_eq!((image.mime, image.ext), (mime, ext));
        assert_eq!((image.width, image.height), (width, height), "{mime}");
        assert_eq!(image.sha256.len(), 64);
    }
}

#[test]
fn validate_rejects_svg_and_html_named_as_a_picture() {
    let svg = br#"<svg xmlns="http://www.w3.org/2000/svg"><script>alert(1)</script></svg>"#;
    assert!(matches!(
        validate_image(svg.to_vec()),
        Err(ImageRejected::Format(_))
    ));
    let html = b"<!doctype html><html><body>not a jpg</body></html>";
    assert!(matches!(
        validate_image(html.to_vec()),
        Err(ImageRejected::Format(_))
    ));
}

#[test]
fn validate_rejects_over_the_byte_and_pixel_bounds() {
    let mut huge = png(10, 10);
    huge.resize(MAX_IMAGE_BYTES + 1, 0);
    assert_eq!(validate_image(huge).unwrap_err(), ImageRejected::TooLarge);
    assert_eq!(
        validate_image(png(10_000, 10_000)).unwrap_err(),
        ImageRejected::TooManyPixels {
            width: 10_000,
            height: 10_000
        }
    );
    assert!(
        validate_image(png(5_000, 10_000)).is_ok(),
        "exactly 50 MP is allowed"
    );
}

/// Serves canned bodies by URL and records what was asked.
#[derive(Default)]
struct FakeNetwork {
    bodies: Mutex<HashMap<String, Result<BoundedBody, BoundedFetchError>>>,
    requests: Mutex<Vec<(String, Option<String>)>>,
}

impl FakeNetwork {
    fn serve(&self, url: &str, bytes: Vec<u8>) {
        self.serve_from(url, url, bytes);
    }

    fn serve_from(&self, url: &str, final_url: &str, bytes: Vec<u8>) {
        self.bodies.lock().unwrap().insert(
            url.to_string(),
            Ok(BoundedBody {
                bytes,
                final_url: final_url.to_string(),
            }),
        );
    }

    fn requested(&self) -> Vec<(String, Option<String>)> {
        self.requests.lock().unwrap().clone()
    }
}

#[async_trait]
impl ImageNetwork for FakeNetwork {
    async fn get(
        &self,
        url: &str,
        headers: HeaderMap,
        max_bytes: usize,
        _deadline: Option<Instant>,
    ) -> Result<BoundedBody, BoundedFetchError> {
        let agent = headers
            .get(USER_AGENT)
            .and_then(|v| v.to_str().ok())
            .map(str::to_string);
        self.requests.lock().unwrap().push((url.to_string(), agent));
        match self.bodies.lock().unwrap().get(url) {
            Some(Ok(body)) if body.bytes.len() > max_bytes => Err(BoundedFetchError::TooLarge),
            Some(result) => result.clone(),
            None => Err(BoundedFetchError::Status(404)),
        }
    }
}

#[tokio::test]
async fn fetch_sniffs_the_bytes_and_stays_on_https() {
    // The body carries no Content-Type at all: only the bytes decide.
    let network = FakeNetwork::default();
    network.serve("https://img.example.org/a", png(64, 48));
    let fetched = fetch_image(
        &network,
        "https://img.example.org/a",
        HeaderMap::new(),
        None,
    )
    .await
    .unwrap();
    assert_eq!(fetched.image.mime, "image/png");

    network.serve_from(
        "https://img.example.org/b",
        "http://img.example.org/b",
        png(64, 48),
    );
    assert_eq!(
        fetch_image(
            &network,
            "https://img.example.org/b",
            HeaderMap::new(),
            None
        )
        .await
        .unwrap_err(),
        ImageRejected::Scheme,
        "a redirect off https is refused"
    );
    assert_eq!(
        fetch_image(&network, "http://img.example.org/a", HeaderMap::new(), None)
            .await
            .unwrap_err(),
        ImageRejected::Scheme
    );
    assert_eq!(
        network.requested().len(),
        2,
        "the http URL was never requested"
    );
}

#[tokio::test]
async fn the_guarded_client_refuses_local_and_metadata_addresses_before_sending() {
    let dir = TempDir::new().unwrap();
    let web = WebService::new(dir.path()).unwrap();
    for url in [
        "https://127.0.0.1/a.png",
        "https://169.254.169.254/latest/meta-data",
        "https://localhost/a.png",
        "https://[::1]/a.png",
    ] {
        assert!(
            matches!(
                fetch_image(&web, url, HeaderMap::new(), None).await,
                Err(ImageRejected::Blocked(_))
            ),
            "{url}"
        );
    }
    let past = Instant::now();
    assert_eq!(
        web.fetch_bytes_bounded(
            "https://example.org/a.png",
            HeaderMap::new(),
            10,
            Some(past)
        )
        .await
        .unwrap_err(),
        BoundedFetchError::Deadline
    );
}

// ---- Commons ----

fn commons_response() -> serde_json::Value {
    serde_json::json!({
        "batchcomplete": true,
        "query": {"pages": [
            {"pageid": 3, "ns": 6, "title": "File:Heron two.png", "index": 2,
             "imageinfo": [{"thumburl": "https://upload.wikimedia.org/two.png",
                            "url": "https://upload.wikimedia.org/two-full.png",
                            "descriptionurl": "https://commons.wikimedia.org/wiki/File:Heron_two.png",
                            "mime": "image/png"}]},
            {"pageid": 1, "ns": 6, "title": "File:Heron one.png", "index": 1,
             "imageinfo": [{"thumburl": "https://upload.wikimedia.org/one.png",
                            "descriptionurl": "https://commons.wikimedia.org/wiki/File:Heron_one.png",
                            "mime": "image/png",
                            "extmetadata": {
                                "ImageDescription": {"value": "<b>Grey</b>  heron &amp;\n fish"},
                                "LicenseShortName": {"value": "CC BY-SA 4.0"},
                                "Artist": {"value": "<a href=\"//x\">Jane</a>"}}}]},
            {"pageid": 2, "ns": 6, "title": "File:Heron.svg", "index": 0,
             "imageinfo": [{"url": "https://upload.wikimedia.org/h.svg", "mime": "image/svg+xml"}]}
        ]}
    })
}

#[test]
fn commons_search_url_carries_the_filter_width_and_double_limit() {
    let url = search_url("grey heron", 3);
    assert!(url.starts_with("https://commons.wikimedia.org/w/api.php?"));
    assert!(url.contains("gsrsearch=filetype%3Abitmap+grey+heron"));
    assert!(url.contains("gsrlimit=6"));
    assert!(url.contains("iiurlwidth=1280"));
    assert!(url.contains("gsrnamespace=6"));
    assert!(
        url.contains("iiextmetadatafilter=ImageDescription%7CLicenseShortName%7CArtist%7CCredit")
    );
}

#[test]
fn commons_parse_ranks_by_index_skips_svg_and_tolerates_missing_metadata() {
    let candidates = parse_search(&commons_response());
    assert_eq!(candidates.len(), 2);
    let one = &candidates[0];
    assert_eq!(one.image_url, "https://upload.wikimedia.org/one.png");
    assert_eq!(one.caption, "Grey heron & fish");
    assert_eq!(one.license.as_deref(), Some("CC BY-SA 4.0"));
    assert_eq!(one.attribution.as_deref(), Some("Jane"));
    assert_eq!(one.origin, ImageOrigin::WikimediaCommons);
    let two = &candidates[1];
    assert_eq!(
        two.caption, "Heron two",
        "the file name stands in for a missing description"
    );
    assert_eq!(two.license, None);
    assert!(parse_search(&serde_json::json!({"batchcomplete": true})).is_empty());
    assert_eq!(strip_html("<p>a<br>b</p>"), "ab");
}

// ---- store ----

#[tokio::test]
async fn store_builds_paths_only_for_valid_ids_and_extensions() {
    let dir = TempDir::new().unwrap();
    let store = ImageStore::new(dir.path().join("chat-images"));
    let id = new_id();
    assert!(store.path_for(&id, "png").is_some());
    assert!(store.path_for(&id, "svg").is_none());
    assert!(store.path_for(&id.to_uppercase(), "png").is_none());
    assert!(store.path_for("../etc/passwd", "png").is_none());

    let bytes = png(4, 4);
    let (sha, size) = store.write(&id, "png", bytes.clone()).await.unwrap();
    assert_eq!(size, bytes.len() as u64);
    assert!(
        matches!(store.read_verified(&id, "png", &sha).await, Verified::Intact(b) if b == bytes)
    );
    assert!(!dir
        .path()
        .join("chat-images")
        .join(format!("{id}.png.part"))
        .exists());
    std::fs::write(store.path_for(&id, "png").unwrap(), b"swapped").unwrap();
    assert!(matches!(
        store.read_verified(&id, "png", &sha).await,
        Verified::Mismatch
    ));
    store.remove(&id, "png").await;
    assert!(matches!(
        store.read_verified(&id, "png", &sha).await,
        Verified::Missing
    ));
    assert!(store.write(&id, "svg", bytes).await.is_err());
}

// ---- service, repository, sweep, protocol ----

struct Fixture {
    pool: SqlitePool,
    dir: TempDir,
    network: Arc<FakeNetwork>,
    service: Arc<ChatImageService>,
}

impl Fixture {
    fn root(&self) -> std::path::PathBuf {
        self.dir.path().join("chat-images")
    }

    async fn row_count(&self) -> i64 {
        sqlx::query_scalar("SELECT COUNT(*) FROM chat_images")
            .fetch_one(&self.pool)
            .await
            .unwrap()
    }

    fn scope(&self) -> ImageTurnScope {
        ImageTurnScope {
            conversation_id: CONVERSATION.into(),
            request_id: REQUEST.into(),
        }
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
    sqlx::query(
        "INSERT INTO conversations (id, title, model_name, space_id, created_at, updated_at) \
         VALUES (?, 'Pictures', 'model', 'space_general', '2026-10-17T00:00:00Z', '2026-10-17T00:00:00Z')",
    )
    .bind(CONVERSATION)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO conversation_messages (id, conversation_id, role, content) \
         VALUES (?, ?, 'assistant', 'Here: ![Grey heron](img-1)')",
    )
    .bind(MESSAGE)
    .bind(CONVERSATION)
    .execute(&pool)
    .await
    .unwrap();
    let dir = TempDir::new().unwrap();
    let network = Arc::new(FakeNetwork::default());
    network.serve(
        &search_url("heron", 3),
        serde_json::to_vec(&commons_response()).unwrap(),
    );
    network.serve("https://upload.wikimedia.org/one.png", png(64, 48));
    network.serve("https://upload.wikimedia.org/two.png", gif(32, 24));
    let service = Arc::new(ChatImageService::new(
        pool.clone(),
        dir.path().join("chat-images"),
        network.clone(),
        Arc::new(lattice_core::shared::runtime::background::BackgroundTasks::default()),
    ));
    Fixture {
        pool,
        dir,
        network,
        service,
    }
}

fn heron() -> FindImageInput {
    FindImageInput {
        query: "heron".into(),
        from_page_url: None,
        limit: 3,
    }
}

#[tokio::test]
async fn find_and_store_writes_rows_files_and_numbered_refs() {
    let fx = fixture().await;
    let (output, images) = fx
        .service
        .find_and_store(&fx.scope(), heron(), None)
        .await
        .unwrap();
    assert_eq!(images.len(), 2);
    assert_eq!(images[0].r#ref, "img-1");
    assert_eq!(images[0].mime, "image/png");
    assert_eq!(images[0].file_name, format!("{}.png", images[0].id));
    assert_eq!(images[1].r#ref, "img-2");
    assert_eq!(images[1].mime, "image/gif");
    assert_eq!(output.images[0].markdown, "![Grey heron & fish](img-1)");
    assert_eq!(output.images[0].license.as_deref(), Some("CC BY-SA 4.0"));
    assert!(output
        .note
        .as_deref()
        .unwrap()
        .contains("Stored 2 of the 3"));
    for image in &images {
        assert!(fx.root().join(&image.file_name).exists());
    }
    assert!(fx
        .network
        .requested()
        .iter()
        .all(|(_, agent)| agent.as_deref() == Some(COMMONS_USER_AGENT)));

    // A second call in the same turn hands back the same pictures.
    let (again, repeat) = fx
        .service
        .find_and_store(&fx.scope(), heron(), None)
        .await
        .unwrap();
    assert_eq!(repeat, images);
    assert_eq!(again.images.len(), 2);
    assert_eq!(fx.row_count().await, 2);
}

#[tokio::test]
async fn find_and_store_reports_rejections_and_search_failures_in_the_note() {
    let fx = fixture().await;
    fx.network
        .serve("https://upload.wikimedia.org/one.png", b"<svg/>".to_vec());
    let (output, images) = fx
        .service
        .find_and_store(&fx.scope(), heron(), None)
        .await
        .unwrap();
    assert_eq!(images.len(), 1);
    assert_eq!(images[0].r#ref, "img-1", "refs count stored pictures only");
    assert!(output.note.as_deref().unwrap().contains("Skipped 1"));

    let missing = FindImageInput {
        query: "nothing served".into(),
        ..heron()
    };
    let (output, images) = fx
        .service
        .find_and_store(&fx.scope(), missing, None)
        .await
        .unwrap();
    assert!(images.is_empty());
    assert_eq!(output.note.as_deref(), Some(NO_PICTURE_NOTE));

    let empty = FindImageInput {
        query: "  ".into(),
        ..heron()
    };
    assert!(fx
        .service
        .find_and_store(&fx.scope(), empty, None)
        .await
        .is_err());
}

#[tokio::test]
async fn from_page_url_takes_the_pages_own_picture() {
    let fx = fixture().await;
    let page = "https://news.example.org/story";
    fx.network.serve(
        page,
        br#"<html><head><title>Herons return</title>
            <meta property="og:image" content="/media/heron.webp"></head></html>"#
            .to_vec(),
    );
    fx.network
        .serve("https://news.example.org/media/heron.webp", webp(400, 300));
    let input = FindImageInput {
        query: "heron".into(),
        from_page_url: Some(page.into()),
        limit: 1,
    };
    let (output, images) = fx
        .service
        .find_and_store(&fx.scope(), input, None)
        .await
        .unwrap();
    assert_eq!(output.note, None);
    assert_eq!(images.len(), 1);
    assert_eq!(images[0].origin, ImageOrigin::Page);
    assert_eq!(images[0].caption, "Herons return");
    assert_eq!(images[0].source_url, page);
    assert_eq!(images[0].license, None);
}

#[tokio::test]
async fn attach_keeps_referenced_pictures_and_releases_the_rest() {
    let fx = fixture().await;
    let (_, images) = fx
        .service
        .find_and_store(&fx.scope(), heron(), None)
        .await
        .unwrap();
    let attached = fx
        .service
        .attach_to_message(REQUEST, MESSAGE, &["img-1".to_string()])
        .await
        .unwrap();
    assert_eq!(attached.len(), 1);
    assert_eq!(attached[0].id, images[0].id);
    assert!(fx.root().join(&images[0].file_name).exists());
    assert!(!fx.root().join(&images[1].file_name).exists());
    assert_eq!(fx.row_count().await, 1);
    assert_eq!(
        fx.service.list_for_message(MESSAGE).await.unwrap(),
        attached
    );

    // Releasing the request now touches nothing: the message holds img-1.
    fx.service.release_request(REQUEST).await.unwrap();
    assert_eq!(fx.row_count().await, 1);

    fx.service.release_message(MESSAGE).await.unwrap();
    assert_eq!(fx.row_count().await, 0);
    assert!(!fx.root().join(&images[0].file_name).exists());
}

#[tokio::test]
async fn release_request_and_conversation_empty_rows_and_files() {
    let fx = fixture().await;
    let (_, images) = fx
        .service
        .find_and_store(&fx.scope(), heron(), None)
        .await
        .unwrap();
    fx.service.release_request(REQUEST).await.unwrap();
    assert_eq!(fx.row_count().await, 0);
    assert!(images
        .iter()
        .all(|image| !fx.root().join(&image.file_name).exists()));

    let (_, images) = fx
        .service
        .find_and_store(&fx.scope(), heron(), None)
        .await
        .unwrap();
    fx.service
        .attach_to_message(REQUEST, MESSAGE, &["img-1".into(), "img-2".into()])
        .await
        .unwrap();
    fx.service.release_conversation(CONVERSATION).await.unwrap();
    assert_eq!(fx.row_count().await, 0);
    assert!(images
        .iter()
        .all(|image| !fx.root().join(&image.file_name).exists()));
}

fn new_row(id: &str) -> NewImageRow {
    NewImageRow {
        id: id.to_string(),
        conversation_id: CONVERSATION.into(),
        request_id: REQUEST.into(),
        ext: "png".into(),
        mime: "image/png".into(),
        sha256: "0".repeat(64),
        byte_size: 4,
        width: 2,
        height: 2,
        caption: "c".into(),
        source_url: "https://example.org".into(),
        source_title: None,
        image_url: format!("https://example.org/{id}.png"),
        origin: ImageOrigin::Page,
        license: None,
        attribution: None,
    }
}

#[tokio::test]
async fn rows_cascade_from_message_and_conversation_deletes() {
    let fx = fixture().await;
    let repository = fx.service.repository();
    assert_eq!(
        repository.insert(&new_row(&new_id())).await.unwrap(),
        "img-1"
    );
    assert_eq!(
        repository.insert(&new_row(&new_id())).await.unwrap(),
        "img-2"
    );
    repository
        .attach(REQUEST, MESSAGE, &["img-1".into()])
        .await
        .unwrap();
    sqlx::query("DELETE FROM conversation_messages WHERE id = ?")
        .bind(MESSAGE)
        .execute(&fx.pool)
        .await
        .unwrap();
    assert_eq!(
        fx.row_count().await,
        1,
        "the attached row went with its message"
    );
    sqlx::query("DELETE FROM conversations WHERE id = ?")
        .bind(CONVERSATION)
        .execute(&fx.pool)
        .await
        .unwrap();
    assert_eq!(
        fx.row_count().await,
        0,
        "the leased row went with its conversation"
    );
}

#[tokio::test]
async fn sweep_drops_leases_and_unclaimed_files_and_keeps_the_rest() {
    let fx = fixture().await;
    let (_, images) = fx
        .service
        .find_and_store(&fx.scope(), heron(), None)
        .await
        .unwrap();
    fx.service
        .repository()
        .attach(REQUEST, MESSAGE, &["img-1".into()])
        .await
        .unwrap();
    let stray = format!("{}.png", new_id());
    let part = format!("{}.jpg.part", new_id());
    for name in [stray.as_str(), part.as_str(), "notes.txt"] {
        std::fs::write(fx.root().join(name), b"x").unwrap();
    }

    let report = fx.service.sweep(&HashSet::new()).await.unwrap();
    assert_eq!(report.removed_leases, 1);
    assert_eq!(
        report.removed_orphans, 3,
        "img-2's file, the stray and the part"
    );
    assert!(fx.root().join(&images[0].file_name).exists());
    assert!(!fx.root().join(&images[1].file_name).exists());
    assert!(!fx.root().join(&stray).exists());
    assert!(!fx.root().join(&part).exists());
    assert!(
        fx.root().join("notes.txt").exists(),
        "unrecognised names are left alone"
    );
    assert_eq!(fx.row_count().await, 1);
}

/// A research turn that resumes after the restart still owns what it
/// leased: the sweep keeps its rows and files.
#[tokio::test]
async fn sweep_keeps_the_leases_of_a_turn_that_resumes() {
    let fx = fixture().await;
    let (_, images) = fx
        .service
        .find_and_store(&fx.scope(), heron(), None)
        .await
        .unwrap();
    let kept: HashSet<String> = [REQUEST.to_string()].into_iter().collect();

    let report = fx.service.sweep(&kept).await.unwrap();
    assert_eq!(report.removed_leases, 0);
    assert_eq!(report.removed_orphans, 0);
    assert!(images
        .iter()
        .all(|image| fx.root().join(&image.file_name).exists()));
    assert_eq!(fx.row_count().await, images.len() as i64);
}

/// After a truncate or regenerate the cascade has taken the rows; their
/// files go at once, and a running turn's leased pictures stay.
#[tokio::test]
async fn unclaimed_files_go_and_leased_ones_stay() {
    let fx = fixture().await;
    let (_, images) = fx
        .service
        .find_and_store(&fx.scope(), heron(), None)
        .await
        .unwrap();
    let stray = format!("{}.png", new_id());
    std::fs::write(fx.root().join(&stray), b"x").unwrap();

    assert_eq!(fx.service.remove_unclaimed_files().await.unwrap(), 1);
    assert!(!fx.root().join(&stray).exists());
    assert!(images
        .iter()
        .all(|image| fx.root().join(&image.file_name).exists()));
}

/// A fork's copied message gets its own row and file for each picture, so
/// deleting the original conversation leaves the fork's pictures intact.
#[tokio::test]
async fn a_copied_message_keeps_its_pictures_when_the_original_is_deleted() {
    const FORK: &str = "conv-fork";
    const FORK_MESSAGE: &str = "msg-fork";
    let fx = fixture().await;
    let (_, images) = fx
        .service
        .find_and_store(&fx.scope(), heron(), None)
        .await
        .unwrap();
    fx.service
        .attach_to_message(REQUEST, MESSAGE, &["img-1".into()])
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO conversations (id, title, model_name, space_id, created_at, updated_at) \
         VALUES (?, 'Fork', 'model', 'space_general', '2026-10-17T00:00:00Z', '2026-10-17T00:00:00Z')",
    )
    .bind(FORK)
    .execute(&fx.pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO conversation_messages (id, conversation_id, role, content) \
         VALUES (?, ?, 'assistant', 'Here: ![Grey heron](img-1)')",
    )
    .bind(FORK_MESSAGE)
    .bind(FORK)
    .execute(&fx.pool)
    .await
    .unwrap();

    let copied = fx
        .service
        .copy_for_messages(
            FORK,
            &[CopiedMessage {
                message_id: FORK_MESSAGE.into(),
                // The second picture was released; a gone original is skipped.
                image_ids: vec![images[0].id.clone(), images[1].id.clone()],
            }],
        )
        .await
        .unwrap();
    assert_eq!(copied.len(), 1);
    let (message_id, copies) = &copied[0];
    assert_eq!(message_id, FORK_MESSAGE);
    assert_eq!(copies.len(), 1);
    let copy = &copies[0];
    assert_ne!(copy.id, images[0].id);
    assert_eq!(copy.r#ref, "img-1");
    assert_eq!(copy.caption, images[0].caption);
    assert_eq!(copy.file_name, format!("{}.png", copy.id));

    fx.service.release_conversation(CONVERSATION).await.unwrap();
    sqlx::query("DELETE FROM conversations WHERE id = ?")
        .bind(CONVERSATION)
        .execute(&fx.pool)
        .await
        .unwrap();

    assert!(!fx.root().join(&images[0].file_name).exists());
    let row = fx
        .service
        .repository()
        .get(&copy.id)
        .await
        .unwrap()
        .expect("the copy's row outlives the original");
    assert_eq!(row.message_id.as_deref(), Some(FORK_MESSAGE));
    assert!(matches!(
        fx.service
            .store()
            .read_verified(&copy.id, &row.ext, &row.sha256)
            .await,
        Verified::Intact(_)
    ));
    assert_eq!(
        fx.service.list_for_message(FORK_MESSAGE).await.unwrap(),
        *copies
    );
}

#[test]
fn route_accepts_only_a_valid_id_and_stored_extension() {
    let id = "0123456789abcdef0123456789abcdef";
    assert_eq!(
        route("GET", &format!("/{id}.webp")),
        Route::Image {
            id: id.into(),
            ext: "webp".into()
        }
    );
    assert_eq!(
        route("GET", "/%30123456789abcdef0123456789abcdef.png"),
        Route::Image {
            id: id.into(),
            ext: "png".into()
        },
        "the path is percent-decoded once"
    );
    for path in [
        format!("/{id}.svg"),
        format!("/{}.png", id.to_uppercase()),
        format!("/{id}.png/extra"),
        format!("/{id}"),
        "/../secret.png".to_string(),
        String::from("/"),
    ] {
        assert_eq!(route("GET", &path), Route::NotFound, "{path}");
    }
    assert_eq!(route("POST", &format!("/{id}.png")), Route::NotFound);
}

#[tokio::test]
async fn respond_serves_intact_pictures_to_the_main_webview_only() {
    let fx = fixture().await;
    let (_, images) = fx
        .service
        .find_and_store(&fx.scope(), heron(), None)
        .await
        .unwrap();
    let path = format!("/{}", images[0].file_name);

    let ok = respond(&fx.service, "main", "GET", &path).await;
    assert_eq!(ok.status(), 200);
    let headers = ok.headers();
    assert_eq!(headers["content-type"], "image/png");
    assert_eq!(headers["x-content-type-options"], "nosniff");
    assert_eq!(headers["content-security-policy"], "default-src 'none'");
    assert_eq!(
        headers["cache-control"],
        "private, max-age=31536000, immutable"
    );
    assert_eq!(headers["content-length"], ok.body().len().to_string());
    assert_eq!(ok.body(), &png(64, 48));

    assert_eq!(
        respond(&fx.service, "reader", "GET", &path).await.status(),
        404
    );
    let unknown = format!("/{}.png", new_id());
    assert_eq!(
        respond(&fx.service, "main", "GET", &unknown).await.status(),
        404
    );
    let wrong_ext = format!("/{}.jpg", images[0].id);
    assert_eq!(
        respond(&fx.service, "main", "GET", &wrong_ext)
            .await
            .status(),
        404
    );

    std::fs::write(fx.root().join(&images[0].file_name), b"swapped").unwrap();
    assert_eq!(
        respond(&fx.service, "main", "GET", &path).await.status(),
        409
    );
    std::fs::remove_file(fx.root().join(&images[0].file_name)).unwrap();
    assert_eq!(
        respond(&fx.service, "main", "GET", &path).await.status(),
        409
    );
}

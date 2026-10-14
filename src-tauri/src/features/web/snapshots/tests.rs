//! Store, repository, job and protocol tests (§12).

use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use sqlx::SqlitePool;
use tempfile::TempDir;

use super::protocol::{respond, route, Route, CSP};
use super::store::{new_id, sha256_hex};
use super::*;
use crate::features::web::mocks::MockPageCapture;
use crate::features::web::traits::page_snapshot::CaptureError;

const CONVERSATION: &str = "conv-snap";
const URL: &str = "https://Example.com/article";

#[derive(Default)]
struct RecordingEvents(Mutex<Vec<(String, serde_json::Value)>>);

impl RecordingEvents {
    fn named(&self, name: &str) -> Vec<serde_json::Value> {
        self.0
            .lock()
            .unwrap()
            .iter()
            .filter(|(event, _)| event == name)
            .map(|(_, payload)| payload.clone())
            .collect()
    }
}

impl SnapshotEvents for RecordingEvents {
    fn emit(&self, event: &str, payload: serde_json::Value) {
        self.0.lock().unwrap().push((event.to_string(), payload));
    }
}

struct Fixture {
    pool: SqlitePool,
    dir: TempDir,
    capture: Arc<MockPageCapture>,
    events: Arc<RecordingEvents>,
    service: Arc<WebPageSnapshotService>,
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
         VALUES (?, 'Snap', 'model', 'space_general', '2026-10-13T00:00:00Z', '2026-10-13T00:00:00Z')",
    )
    .bind(CONVERSATION)
    .execute(&pool)
    .await
    .unwrap();
    let dir = tempfile::tempdir().unwrap();
    let capture = Arc::new(MockPageCapture::new());
    let events = Arc::new(RecordingEvents::default());
    let jobs = crate::shared::runtime::jobs::JobRuntime::new(pool.clone());
    let service = Arc::new(WebPageSnapshotService::new(
        pool.clone(),
        dir.path().join("web-snapshots"),
        capture.clone(),
        jobs,
        events.clone(),
    ));
    service.register_job().await.unwrap();
    Fixture {
        pool,
        dir,
        capture,
        events,
        service,
    }
}

impl Fixture {
    fn root(&self) -> std::path::PathBuf {
        self.dir.path().join("web-snapshots")
    }

    async fn settled(&self, url: &str) -> WebPageSnapshotDto {
        for _ in 0..500 {
            let dto = self.service.get(CONVERSATION, url).await.unwrap().unwrap();
            if dto.status != WebPageSnapshotStatus::Pending {
                return dto;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("the snapshot never settled");
    }

    /// The statuses of every changed event so far, once the last is `ready`.
    async fn statuses_until_ready(&self) -> Vec<String> {
        for _ in 0..500 {
            let statuses: Vec<String> = self
                .events
                .named(CHANGED_EVENT)
                .iter()
                .map(|event| event["snapshot"]["status"].as_str().unwrap().to_string())
                .collect();
            if statuses.last().map(String::as_str) == Some("ready") {
                return statuses;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("the ready event never arrived");
    }

    async fn ready(&self) -> WebPageSnapshotDto {
        self.service
            .ensure_capture(CONVERSATION, URL, false)
            .await
            .unwrap();
        let dto = self.settled(URL).await;
        assert_eq!(dto.status, WebPageSnapshotStatus::Ready);
        dto
    }

    async fn serve(
        &self,
        label: &str,
        path: &str,
        query: Option<&str>,
    ) -> tauri::http::Response<Vec<u8>> {
        respond(&self.service, label, "GET", path, query).await
    }
}

// --- store / repository / job -------------------------------------------

#[tokio::test]
async fn capture_goes_pending_then_ready_with_hash() {
    let f = fixture().await;
    let first = f
        .service
        .ensure_capture(CONVERSATION, URL, false)
        .await
        .unwrap();
    assert_eq!(first.status, WebPageSnapshotStatus::Pending);
    assert_eq!(first.normalized_url, "https://example.com/article");
    assert_eq!(first.id.len(), 32);

    let ready = f.settled(URL).await;
    assert_eq!(ready.status, WebPageSnapshotStatus::Ready);
    let bytes = std::fs::read(f.root().join(format!("{}.html", ready.id))).unwrap();
    assert_eq!(ready.sha256.as_deref(), Some(sha256_hex(&bytes).as_str()));
    assert_eq!(ready.byte_size, Some(bytes.len() as u64));
    assert_eq!(ready.verified, Some(true));
    assert_eq!(ready.method.as_deref(), Some("http"));
    assert_eq!(ready.resource_count, 2);
    assert!(!f.root().join(format!("{}.html.part", ready.id)).exists());

    // The row turns ready before the job announces it, so wait for the event.
    let statuses = f.statuses_until_ready().await;
    assert_eq!(statuses.first().map(String::as_str), Some("pending"));
    assert_eq!(statuses.last().map(String::as_str), Some("ready"));
}

#[cfg(unix)]
#[tokio::test]
async fn snapshot_files_are_private() {
    use std::os::unix::fs::PermissionsExt;
    let f = fixture().await;
    let ready = f.ready().await;
    let mode = std::fs::metadata(f.root().join(format!("{}.html", ready.id)))
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o600);
}

#[tokio::test]
async fn first_wins() {
    let f = fixture().await;
    let ready = f.ready().await;
    let again = f
        .service
        .ensure_capture(CONVERSATION, "https://example.com/article ", false)
        .await
        .unwrap();
    assert_eq!(again.id, ready.id);
    assert_eq!(again.status, WebPageSnapshotStatus::Ready);
    assert_eq!(f.capture.calls(), 1);
    assert_eq!(f.service.list(CONVERSATION).await.unwrap().len(), 1);
}

#[tokio::test]
async fn failed_row_is_retried_only_on_request_with_a_new_job() {
    let f = fixture().await;
    f.capture.push(Err(CaptureError::Blocked { status: 403 }));
    f.service
        .ensure_capture(CONVERSATION, URL, false)
        .await
        .unwrap();
    let failed = f.settled(URL).await;
    assert_eq!(failed.status, WebPageSnapshotStatus::Failed);
    assert_eq!(failed.error_code.as_deref(), Some("blocked"));
    let repository = repository::SnapshotRepository::new(f.pool.clone());
    let first_job = repository.get(&failed.id).await.unwrap().unwrap().job_id;

    // An automatic ensure leaves a failed row alone.
    let untouched = f
        .service
        .ensure_capture(CONVERSATION, URL, false)
        .await
        .unwrap();
    assert_eq!(untouched.status, WebPageSnapshotStatus::Failed);
    assert_eq!(f.capture.calls(), 1);

    let retried = f
        .service
        .ensure_capture(CONVERSATION, URL, true)
        .await
        .unwrap();
    assert_eq!(retried.id, failed.id);
    let ready = f.settled(URL).await;
    assert_eq!(ready.status, WebPageSnapshotStatus::Ready);
    assert_eq!(ready.error_code, None);
    let second_job = repository.get(&failed.id).await.unwrap().unwrap().job_id;
    assert!(second_job.is_some());
    assert_ne!(first_job, second_job);
    assert_eq!(f.capture.calls(), 2);
}

#[tokio::test]
async fn non_web_urls_are_refused() {
    let f = fixture().await;
    assert!(f
        .service
        .ensure_capture(CONVERSATION, "file:///etc/passwd", false)
        .await
        .is_err());
}

#[tokio::test]
async fn rows_cascade_with_their_conversation_and_the_sweep_takes_the_file() {
    let f = fixture().await;
    let ready = f.ready().await;
    let file = f.root().join(format!("{}.html", ready.id));
    sqlx::query("DELETE FROM conversations WHERE id = ?")
        .bind(CONVERSATION)
        .execute(&f.pool)
        .await
        .unwrap();
    assert!(f.service.list(CONVERSATION).await.unwrap().is_empty());
    assert!(file.exists());
    let report = f.service.sweep().await.unwrap();
    assert_eq!(report.removed_orphans, 1);
    assert!(!file.exists());
}

#[tokio::test]
async fn sweep_removes_orphans_and_stale_parts_and_keeps_live_files() {
    let f = fixture().await;
    let ready = f.ready().await;
    let root = f.root();
    let live = root.join(format!("{}.html", ready.id));
    let orphan = root.join(format!("{}.html", new_id()));
    let stale = root.join(format!("{}.html.part", new_id()));
    let fresh = root.join(format!("{}.html.part", new_id()));
    let unrelated = root.join("notes.txt");
    for path in [&orphan, &stale, &fresh, &unrelated] {
        std::fs::write(path, b"x").unwrap();
    }
    std::fs::File::options()
        .write(true)
        .open(&stale)
        .unwrap()
        .set_modified(SystemTime::now() - Duration::from_secs(2 * 60 * 60))
        .unwrap();

    let report = f.service.sweep().await.unwrap();
    assert_eq!(report.removed_orphans, 1);
    assert_eq!(report.removed_parts, 1);
    assert!(live.exists() && fresh.exists() && unrelated.exists());
    assert!(!orphan.exists() && !stale.exists());
}

// --- protocol ------------------------------------------------------------

#[test]
fn routes_parse_ids_links_and_refuse_everything_else() {
    let id = "0123456789abcdef0123456789abcdef";
    assert_eq!(
        route("GET", &format!("/{id}/index.html"), None),
        Route::Snapshot {
            id: id.into(),
            clean: false
        }
    );
    // convertFileSrc percent-encodes the whole path.
    assert_eq!(
        route("GET", &format!("/{id}%2Findex.html"), Some("clean=1")),
        Route::Snapshot {
            id: id.into(),
            clean: true
        }
    );
    for bad in [
        "/../etc/passwd".to_string(),
        "/%2E%2E%2Fetc%2Fpasswd".to_string(),
        format!("/{id}/../../x/index.html"),
        format!("/{}/index.html", id.to_uppercase()),
        format!("/{id}%252Findex.html"),
        format!("/{id}"),
        "/".to_string(),
    ] {
        assert_eq!(route("GET", &bad, None), Route::NotFound, "{bad}");
    }
    assert_eq!(
        route("POST", &format!("/{id}/index.html"), None),
        Route::NotFound
    );

    let link = format!(
        "u={}&s={id}",
        urlencoding::encode("https://example.com/a?b=c")
    );
    assert_eq!(
        route("GET", "/l", Some(&link)),
        Route::Link {
            url: "https://example.com/a?b=c".into(),
            id: id.into()
        }
    );
    let script = format!("u={}&s={id}", urlencoding::encode("javascript:alert(1)"));
    assert_eq!(route("GET", "/l", Some(&script)), Route::NotFound);
    let long = format!("u=https%3A%2F%2Fexample.com%2F{}&s={id}", "a".repeat(2100));
    assert_eq!(route("GET", "/l", Some(&long)), Route::NotFound);
    assert_eq!(
        route("GET", "/l", Some("u=https%3A%2F%2Fexample.com&s=nope")),
        Route::NotFound
    );
}

#[tokio::test]
async fn serves_a_ready_snapshot_with_locked_headers_only_to_main() {
    let f = fixture().await;
    let ready = f.ready().await;
    let path = format!("/{}%2Findex.html", ready.id);

    let ok = f.serve("main", &path, None).await;
    assert_eq!(ok.status(), 200);
    let headers = ok.headers();
    assert_eq!(headers["content-type"], "text/html; charset=utf-8");
    assert_eq!(headers["content-security-policy"], CSP);
    assert_eq!(headers["referrer-policy"], "no-referrer");
    assert_eq!(headers["x-content-type-options"], "nosniff");
    assert_eq!(headers["cache-control"], "no-store");
    assert_eq!(headers["content-length"], ok.body().len().to_string());
    let on_disk = std::fs::read(f.root().join(format!("{}.html", ready.id))).unwrap();
    assert_eq!(ok.body(), &on_disk);

    let hidden = f.serve("page-reader-1", &path, None).await;
    assert_eq!(hidden.status(), 404);
    assert!(hidden.body().is_empty());
    assert_eq!(hidden.headers()["cache-control"], "no-store");
}

#[tokio::test]
async fn clean_layer_is_added_only_with_clean_1_and_never_written_to_disk() {
    let f = fixture().await;
    let ready = f.ready().await;
    let path = format!("/{}/index.html", ready.id);
    let plain = f.serve("main", &path, None).await;
    assert!(!String::from_utf8_lossy(plain.body()).contains("lattice-clean"));
    let clean = f.serve("main", &path, Some("clean=1")).await;
    let body = String::from_utf8_lossy(clean.body()).to_string();
    let style = body.find("<style id=\"lattice-clean\">").unwrap();
    assert!(style < body.find("</head>").unwrap());
    let on_disk = std::fs::read_to_string(f.root().join(format!("{}.html", ready.id))).unwrap();
    assert!(!on_disk.contains("lattice-clean"));
}

#[tokio::test]
async fn a_tampered_file_is_refused_with_409() {
    let f = fixture().await;
    let ready = f.ready().await;
    std::fs::write(
        f.root().join(format!("{}.html", ready.id)),
        b"<html><head></head><body>forged</body></html>",
    )
    .unwrap();
    let refused = f
        .serve("main", &format!("/{}/index.html", ready.id), None)
        .await;
    assert_eq!(refused.status(), 409);
    assert_eq!(refused.headers()["content-security-policy"], CSP);
    assert!(String::from_utf8_lossy(refused.body()).contains("no longer matches"));
    let dto = f.service.get(CONVERSATION, URL).await.unwrap().unwrap();
    assert_eq!(dto.verified, Some(false));
}

#[tokio::test]
async fn unknown_ids_are_404_and_pending_rows_are_425() {
    let f = fixture().await;
    let unknown = f
        .serve("main", &format!("/{}/index.html", new_id()), None)
        .await;
    assert_eq!(unknown.status(), 404);
    assert_eq!(unknown.headers()["content-security-policy"], CSP);

    let repository = repository::SnapshotRepository::new(f.pool.clone());
    let (row, created) = repository
        .insert_or_get(
            &new_id(),
            CONVERSATION,
            "https://example.com/pending",
            "https://example.com/pending",
        )
        .await
        .unwrap();
    assert!(created);
    let pending = f
        .serve("main", &format!("/{}/index.html", row.id), None)
        .await;
    assert_eq!(pending.status(), 425);
}

#[tokio::test]
async fn link_clicks_answer_204_and_emit_the_link_event() {
    let f = fixture().await;
    let id = new_id();
    let query = format!(
        "u={}&s={id}",
        urlencoding::encode("https://example.org/next")
    );
    let answered = f.serve("main", "/l", Some(&query)).await;
    assert_eq!(answered.status(), 204);
    assert!(answered.body().is_empty());
    assert_eq!(
        f.events.named(LINK_EVENT),
        vec![serde_json::json!({ "snapshotId": id, "url": "https://example.org/next" })]
    );
    // Hidden windows never reach the event.
    assert_eq!(
        f.serve("page-reader-2", "/l", Some(&query)).await.status(),
        404
    );
    assert_eq!(f.events.named(LINK_EVENT).len(), 1);
}

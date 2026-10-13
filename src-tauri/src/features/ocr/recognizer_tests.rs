#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic
)]

use super::*;
use crate::features::ocr::model::{FixedOcrModel, OcrModel};
use crate::features::ocr::server::{RunningServer, ServerProcess};
use serde_json::json;
use std::sync::atomic::{AtomicUsize, Ordering};
use wiremock::matchers::{body_string_contains, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

struct NoProcess;

impl ServerProcess for NoProcess {
    fn has_exited(&mut self) -> bool {
        false
    }

    fn kill(&mut self) {}
}

/// "Starts" a server that is really a wiremock endpoint.
struct PointAt {
    endpoint: String,
    launches: AtomicUsize,
}

#[async_trait]
impl ServerLauncher for PointAt {
    async fn launch(&self, _model: &OcrModel) -> Result<RunningServer, OcrError> {
        self.launches.fetch_add(1, Ordering::SeqCst);
        Ok(RunningServer::new(
            self.endpoint.clone(),
            "key".into(),
            Box::new(NoProcess),
        ))
    }
}

fn ocr(model: Option<OcrModel>, endpoint: &str) -> (VisionOcr, Arc<PointAt>) {
    let launcher = Arc::new(PointAt {
        endpoint: endpoint.to_string(),
        launches: AtomicUsize::new(0),
    });
    let ocr = VisionOcr::new(
        Arc::new(FixedOcrModel(model)),
        Arc::clone(&launcher) as Arc<dyn ServerLauncher>,
    )
    .unwrap();
    (ocr, launcher)
}

fn model() -> OcrModel {
    OcrModel::in_dir("paddleocr-vl-1.6", Path::new("/models/ocr"), "OCR:")
}

async fn answering(text: &str) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .and(body_string_contains("\"OCR:\""))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "choices": [{ "message": { "content": text }, "finish_reason": "stop" }],
        })))
        .mount(&server)
        .await;
    server
}

fn png_file(dir: &Path) -> std::path::PathBuf {
    let path = dir.join("whiteboard.png");
    std::fs::write(&path, b"\x89PNG\r\n\x1a\nrest").unwrap();
    path
}

#[tokio::test]
async fn without_a_model_nothing_is_rendered_or_started() {
    let (ocr, launcher) = ocr(None, "http://127.0.0.1:1");

    let error = ocr
        .recognize_page(Path::new("/missing.pdf"), 1)
        .await
        .unwrap_err();

    assert_eq!(error, OcrError::NotConfigured);
    assert_eq!(launcher.launches.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn an_image_file_is_read_through_the_server() {
    let server = answering("Quarterly plan\nShip OCR").await;
    let dir = tempfile::tempdir().unwrap();
    let (ocr, launcher) = ocr(Some(model()), &server.uri());

    let text = ocr.recognize_page(&png_file(dir.path()), 1).await.unwrap();

    assert_eq!(text, "Quarterly plan\nShip OCR");
    assert_eq!(launcher.launches.load(Ordering::SeqCst), 1);
}

/// A page that cannot be rendered never costs a server start.
#[tokio::test]
async fn an_unrenderable_page_does_not_start_the_server() {
    let dir = tempfile::tempdir().unwrap();
    let (ocr, launcher) = ocr(Some(model()), "http://127.0.0.1:1");

    let error = ocr
        .recognize_page(&png_file(dir.path()), 2)
        .await
        .unwrap_err();

    assert!(matches!(error, OcrError::PageRender { page: 2, .. }));
    assert_eq!(launcher.launches.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn pages_of_one_import_share_one_server() {
    let server = answering("text").await;
    let dir = tempfile::tempdir().unwrap();
    let image = png_file(dir.path());
    let (ocr, launcher) = ocr(Some(model()), &server.uri());

    for _ in 0..3 {
        ocr.recognize_page(&image, 1).await.unwrap();
    }

    assert_eq!(launcher.launches.load(Ordering::SeqCst), 1);
}

#[cfg(target_os = "macos")]
#[tokio::test]
async fn a_pdf_page_is_sent_as_a_rendered_png() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(body_string_contains("data:image/png;base64,"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "choices": [{ "message": { "content": "Scanned words" }, "finish_reason": "stop" }],
        })))
        .expect(1)
        .mount(&server)
        .await;
    let dir = tempfile::tempdir().unwrap();
    let pdf = dir.path().join("scan.pdf");
    write_one_page_pdf(&pdf);
    let (ocr, _) = ocr(Some(model()), &server.uri());

    assert_eq!(ocr.recognize_page(&pdf, 1).await.unwrap(), "Scanned words");
}

#[cfg(target_os = "macos")]
fn write_one_page_pdf(path: &Path) {
    use lopdf::{dictionary, Document, Object, Stream};
    let mut doc = Document::with_version("1.5");
    let pages_id = doc.new_object_id();
    let content_id = doc.add_object(Stream::new(
        dictionary! {},
        b"0 0 0 rg 72 72 200 100 re f".to_vec(),
    ));
    let page_id = doc.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "Contents" => content_id,
        "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
    });
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => vec![page_id.into()],
            "Count" => 1,
        }),
    );
    let catalog_id = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    doc.trailer.set("Root", catalog_id);
    doc.save(path).unwrap();
}

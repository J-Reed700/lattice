//! The fetch chain's archive steps against a fixture archive: a live page that
//! answered 404 is rescued and labelled, a dated read takes the CDX-nearest
//! copy, a CDX 429 is reported and pauses CDX, and the refusal cache keeps
//! only what the archive could not rescue.
//!
//! The live read itself cannot reach a fixture server (the address guard
//! refuses loopback, as it must), so these tests enter the chain where the
//! live read hands over its failure.

use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::{TimeZone, Utc};
use lattice_core::application::contracts::settings::SourcingSettingsDto;
use tempfile::TempDir;
use tokio::sync::Semaphore;
use wiremock::matchers::{any, path, path_regex, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::archive::NOTHING_ARCHIVED;
use super::*;
use crate::features::web::services::finders::FinderHttp;
use crate::features::web::services::page_cache::CachedLookup;
use crate::features::web::services::wayback::{Endpoints, Wayback};
use crate::features::web::services::web::SitePacer;

const GONE: &str = "https://gone.example/report";
const AVAILABILITY_HIT: &str = r#"{"url": "gone.example/report", "archived_snapshots": {"closest": {"status": "200", "available": true, "url": "http://web.archive.org/web/20150102000131/https://gone.example/report", "timestamp": "20150102000131"}}}"#;
const AVAILABILITY_NONE: &str = r#"{"url": "gone.example/report", "archived_snapshots": {}}"#;
const AVAILABILITY_REDIRECT: &str = r#"{"archived_snapshots": {"closest": {"status": "302", "available": true, "url": "http://web.archive.org/web/20150101000000/https://gone.example/report", "timestamp": "20150101000000"}}}"#;
const ARCHIVED_HTML: &str = "<html><head><title>The 2014 report</title></head><body><article><p>The archived report says the bridge opened in 2014.</p></article></body></html>";

fn service(dir: &TempDir, server: &MockServer, settings: SourcingSettingsDto) -> WebService {
    let http = FinderHttp {
        client: reqwest::Client::new(),
        pacer: Arc::new(SitePacer::with_interval(Duration::ZERO)),
        parsers: Arc::new(Semaphore::new(2)),
        contact: None,
        public_only: false,
    };
    let endpoints = Endpoints {
        availability: format!("{}/wayback/available", server.uri()),
        cdx: format!("{}/cdx/search/cdx", server.uri()),
        replay: format!("{}/web/", server.uri()),
    };
    WebService {
        wayback: Wayback::with_endpoints(http, settings, endpoints),
        ..WebService::new(dir.path()).unwrap()
    }
}

async fn replay_serves(server: &MockServer, timestamp: &str) {
    Mock::given(path_regex(format!(r"^/web/{timestamp}id_/")))
        // `set_body_string` would label the copy `text/plain`, whatever header
        // came before it, and an archived copy is read by its type.
        .respond_with(
            ResponseTemplate::new(200).set_body_raw(ARCHIVED_HTML, "text/html; charset=utf-8"),
        )
        .mount(server)
        .await;
}

async fn after_live_404(service: &WebService, url: &str) -> Result<ReadPage> {
    service
        .rescue_or_refuse(
            url,
            &Url::parse(url).unwrap(),
            LiveFailure::Status(StatusCode::NOT_FOUND),
            Instant::now(),
            PageRead::Chat,
        )
        .await
}

/// A 404 with an availability hit is read from the archive, labelled with
/// the capture and the original, and cached as that copy: the refusal cache
/// never remembers a rescued URL, and the next read costs no request.
#[tokio::test]
async fn a_404_with_an_availability_hit_is_read_from_the_archive() {
    let dir = TempDir::new().unwrap();
    let server = MockServer::start().await;
    Mock::given(path("/wayback/available"))
        .and(query_param("url", GONE))
        .respond_with(ResponseTemplate::new(200).set_body_string(AVAILABILITY_HIT))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(path("/cdx/search/cdx"))
        .respond_with(ResponseTemplate::new(200).set_body_string("[]"))
        .expect(0)
        .mount(&server)
        .await;
    replay_serves(&server, "20150102000131").await;
    let service = service(&dir, &server, SourcingSettingsDto::default());

    let page = after_live_404(&service, GONE).await.unwrap();
    let captured_at = Utc.with_ymd_and_hms(2015, 1, 2, 0, 1, 31).unwrap();
    assert_eq!(
        page.output.read_via,
        ReadVia::Archive {
            captured_at,
            original: GONE.to_string(),
        }
    );
    assert!(page.output.content.contains("bridge opened in 2014"));
    assert_eq!(
        page.output.url,
        format!("{}/web/20150102000131/{GONE}", server.uri())
    );
    // The tool output says how the page was read.
    let payload = serde_json::to_value(&page.output).unwrap();
    assert_eq!(
        payload["read_via"]["archive"]["original"],
        serde_json::json!(GONE)
    );

    match service.page_cache.get(GONE).await {
        Some(CachedFetch::Page(cached)) => assert_eq!(cached.read_via, page.output.read_via),
        other => panic!("a rescued URL is cached as its copy, not refused: {other:?}"),
    }
    let again = service.read_page(GONE).await.unwrap();
    assert!(again.output.from_cache);
    assert_eq!(again.output.read_via, page.output.read_via);
}

/// `as_of` reads the copy nearest the day: availability's closest capture
/// was a redirect, so CDX names the nearest 200 capture, and that is read.
#[tokio::test]
async fn a_dated_read_takes_the_cdx_nearest_copy() {
    let dir = TempDir::new().unwrap();
    let server = MockServer::start().await;
    Mock::given(path("/wayback/available"))
        .and(query_param("timestamp", "20150101"))
        .respond_with(ResponseTemplate::new(200).set_body_string(AVAILABILITY_REDIRECT))
        .mount(&server)
        .await;
    Mock::given(path("/cdx/search/cdx"))
        .and(query_param("closest", "20150101"))
        .and(query_param("sort", "closest"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(r#"[["timestamp","statuscode"],["20141231120000","200"]]"#),
        )
        .expect(1)
        .mount(&server)
        .await;
    replay_serves(&server, "20141231120000").await;
    let service = service(&dir, &server, SourcingSettingsDto::default());

    let day = NaiveDate::from_ymd_opt(2015, 1, 1).unwrap();
    let page = service.read_page_as_of(GONE, day).await.unwrap();
    assert_eq!(
        page.output.read_via,
        ReadVia::Archive {
            captured_at: Utc.with_ymd_and_hms(2014, 12, 31, 12, 0, 0).unwrap(),
            original: GONE.to_string(),
        }
    );
    // A dated copy never stands in for the live page.
    assert_eq!(service.page_cache.get(GONE).await, None);
    // The same dated read again is served from the lookup and copy caches.
    let again = service.read_page_as_of(GONE, day).await.unwrap();
    assert!(again.output.from_cache);
}

/// A CDX 429 opens the archive's CDX breaker; the read's error names it, the
/// refusal is not remembered (the archive could not answer), and the next
/// read skips CDX without a request and says until when.
#[tokio::test]
async fn a_cdx_429_opens_the_archive_breaker_and_the_chain_reports_it() {
    let dir = TempDir::new().unwrap();
    let server = MockServer::start().await;
    Mock::given(path("/wayback/available"))
        .respond_with(ResponseTemplate::new(200).set_body_string(AVAILABILITY_NONE))
        .mount(&server)
        .await;
    Mock::given(path("/cdx/search/cdx"))
        .respond_with(ResponseTemplate::new(429))
        .expect(1)
        .mount(&server)
        .await;
    let service = service(&dir, &server, SourcingSettingsDto::default());

    let error = after_live_404(&service, GONE)
        .await
        .unwrap_err()
        .to_string();
    assert!(error.contains("HTTP 404"), "{error}");
    assert!(error.contains("wayback_cdx: failed (HTTP 429)"), "{error}");
    assert_eq!(service.page_cache.get(GONE).await, None);

    let other = "https://gone.example/other";
    let error = after_live_404(&service, other)
        .await
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("wayback_cdx: skipped (paused until"),
        "{error}"
    );
}

/// Both indexes answered with nothing: the refusal is settled and kept.
#[tokio::test]
async fn a_refusal_the_archive_cannot_rescue_is_remembered() {
    let dir = TempDir::new().unwrap();
    let server = MockServer::start().await;
    Mock::given(path("/wayback/available"))
        .respond_with(ResponseTemplate::new(200).set_body_string(AVAILABILITY_NONE))
        .mount(&server)
        .await;
    Mock::given(path("/cdx/search/cdx"))
        .respond_with(ResponseTemplate::new(200).set_body_string("[]"))
        .expect(1)
        .mount(&server)
        .await;
    let service = service(&dir, &server, SourcingSettingsDto::default());

    let error = after_live_404(&service, GONE)
        .await
        .unwrap_err()
        .to_string();
    assert!(error.contains(NOTHING_ARCHIVED), "{error}");
    assert!(matches!(
        service.page_cache.get(GONE).await,
        Some(CachedFetch::Refusal { .. })
    ));
    // The miss is a lookup the cache answers next time: no second request.
    let parsed = Url::parse(GONE).unwrap();
    assert_eq!(
        service.page_cache.archive_lookup(&parsed, None).await,
        Some(CachedLookup::NothingArchived)
    );
}

/// The archive switch off: nothing is sent, a dated read says why it cannot
/// be done, and a refusal is settled as it was before the archive existed.
#[tokio::test]
async fn the_archive_switch_off_sends_nothing_and_settles_the_refusal() {
    let dir = TempDir::new().unwrap();
    let server = MockServer::start().await;
    Mock::given(any())
        .respond_with(ResponseTemplate::new(200))
        .expect(0)
        .mount(&server)
        .await;
    let settings = SourcingSettingsDto {
        archive_fallback: false,
        ..SourcingSettingsDto::default()
    };
    let service = service(&dir, &server, settings);

    let error = after_live_404(&service, GONE)
        .await
        .unwrap_err()
        .to_string();
    assert!(error.contains("archive fallback is off"), "{error}");
    assert!(matches!(
        service.page_cache.get(GONE).await,
        Some(CachedFetch::Refusal { .. })
    ));
    let day = NaiveDate::from_ymd_opt(2015, 1, 1).unwrap();
    assert!(matches!(
        service.read_page_as_of(GONE, day).await,
        Err(AppError::InvalidInput(_))
    ));
}

/// An archive URL read directly is an archived read and says so.
#[test]
fn a_direct_read_of_an_archive_url_is_labelled() {
    assert_eq!(
        direct_read_via("https://web.archive.org/web/20150102000131/https://gone.example/report"),
        ReadVia::Archive {
            captured_at: Utc.with_ymd_and_hms(2015, 1, 2, 0, 1, 31).unwrap(),
            original: GONE.to_string(),
        }
    );
    assert_eq!(direct_read_via(GONE), ReadVia::Direct);
}

/// A search with `as_of` is a dated read of the page its identifier names:
/// the one result is the archived copy's snapshot address, dated by its
/// capture. Without an address there is nothing to look up.
#[tokio::test]
async fn a_dated_search_answers_with_the_archived_copy_of_the_named_page() {
    use lattice_core::application::contracts::web::WebSearchInput;
    use lattice_core::application::ports::WebServiceTrait;
    let dir = TempDir::new().unwrap();
    let server = MockServer::start().await;
    Mock::given(path("/wayback/available"))
        .respond_with(ResponseTemplate::new(200).set_body_string(AVAILABILITY_HIT))
        .mount(&server)
        .await;
    replay_serves(&server, "20150102000131").await;
    let service = service(&dir, &server, SourcingSettingsDto::default());
    let dated = |identifier: Option<&str>| -> WebSearchInput {
        serde_json::from_value(serde_json::json!({
            "query": "bridge opening report",
            "kind": "web",
            "identifier": identifier,
            "as_of": "2015-01-01",
            "max_results": 12
        }))
        .unwrap()
    };

    let output = service.search_web(&dated(Some(GONE))).await.unwrap();
    assert_eq!(output.results.len(), 1);
    let copy = &output.results[0];
    assert_eq!(
        copy.url,
        format!("{}/web/20150102000131/{GONE}", server.uri())
    );
    assert_eq!(copy.title, "The 2014 report");
    assert_eq!(
        copy.published_date,
        Some(Utc.with_ymd_and_hms(2015, 1, 2, 0, 1, 31).unwrap())
    );
    assert!(!output.has_more);

    assert!(matches!(
        service.search_web(&dated(None)).await,
        Err(AppError::InvalidInput(_))
    ));
}

/// A one-page PDF holding `lines`, built as the indexing PDF tests build theirs.
fn one_page_pdf(lines: &[&str]) -> Vec<u8> {
    use lopdf::content::{Content, Operation};
    use lopdf::{dictionary, Document, Object, Stream};
    let mut doc = Document::with_version("1.5");
    let pages_id = doc.new_object_id();
    let font = doc.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica",
    });
    let resources_id = doc.add_object(dictionary! { "Font" => dictionary! { "F1" => font } });
    let operations: Vec<Operation> = lines
        .iter()
        .enumerate()
        .flat_map(|(index, text)| {
            vec![
                Operation::new("BT", vec![]),
                Operation::new("Tf", vec!["F1".into(), 11.into()]),
                Operation::new("Td", vec![72.into(), (720 - 16 * index as i64).into()]),
                Operation::new("Tj", vec![Object::string_literal(*text)]),
                Operation::new("ET", vec![]),
            ]
        })
        .collect();
    let content_id = doc.add_object(Stream::new(
        dictionary! {},
        Content { operations }.encode().expect("content encodes"),
    ));
    let page_id = doc.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "Contents" => content_id,
        "Resources" => resources_id,
        "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
    });
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => vec![Object::Reference(page_id)],
            "Count" => 1,
        }),
    );
    let catalog_id = doc.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    doc.trailer.set("Root", catalog_id);
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).expect("fixture saves");
    bytes
}

/// An archived PDF is read as a PDF. Parsed as a page, its raw bytes were
/// saved as a course source's text (`%PDF-1.5 … FlateDecode …`), and a
/// refresh read them back the same way: the live read had failed, and the
/// rescue read the archive's copy as markup.
#[tokio::test]
async fn an_archived_pdf_is_read_as_a_pdf_not_as_its_bytes() {
    let dir = TempDir::new().unwrap();
    let server = MockServer::start().await;
    Mock::given(path("/wayback/available"))
        .and(query_param("url", GONE))
        .respond_with(ResponseTemplate::new(200).set_body_string(AVAILABILITY_HIT))
        .mount(&server)
        .await;
    Mock::given(path_regex(r"^/web/20150102000131id_/"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "application/pdf")
                .set_body_bytes(one_page_pdf(&[
                    "The archived report says the bridge opened in 2014.",
                    "It carried two lanes of traffic in each direction.",
                ])),
        )
        .mount(&server)
        .await;
    let service = service(&dir, &server, SourcingSettingsDto::default());

    let page = after_live_404(&service, GONE).await.unwrap();

    let content = &page.output.content;
    assert!(content.contains("bridge opened in 2014"), "{content}");
    assert!(
        !content.contains("%PDF"),
        "the raw bytes were kept: {content}"
    );
}

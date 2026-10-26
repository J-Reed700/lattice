//! The Wayback client against fixture servers: what each endpoint is sent,
//! how its answers read, and how a refusal opens its breaker. The fixtures
//! follow the shapes archive.org documents and answered with on 2026-10-18.

use std::sync::Arc;
use std::time::Duration;

use chrono::TimeZone;
use tokio::sync::Semaphore;
use wiremock::matchers::{header_regex, method, path, path_regex, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::*;
use crate::features::web::services::web::SitePacer;

/// The availability API's answer for `example.com` near 2015-01-01, as it
/// came back on 2026-10-18.
const AVAILABILITY_HIT: &str = r#"{"url": "example.com", "archived_snapshots": {"closest": {"status": "200", "available": true, "url": "http://web.archive.org/web/20150102000131/http://example.com/", "timestamp": "20150102000131"}}, "timestamp": "20150101"}"#;
const AVAILABILITY_NONE: &str = r#"{"url": "example.com", "archived_snapshots": {}}"#;
const AVAILABILITY_REDIRECT: &str = r#"{"archived_snapshots": {"closest": {"status": "302", "available": true, "url": "http://web.archive.org/web/20150101000000/http://example.com/", "timestamp": "20150101000000"}}}"#;
const CDX_NEAREST: &str = r#"[["timestamp","statuscode"],["20141231120000","200"]]"#;

fn http() -> FinderHttp {
    FinderHttp {
        client: reqwest::Client::new(),
        pacer: Arc::new(SitePacer::with_interval(Duration::ZERO)),
        parsers: Arc::new(Semaphore::new(2)),
        contact: None,
        public_only: false,
    }
}

fn endpoints(server: &MockServer) -> Endpoints {
    Endpoints {
        availability: format!("{}/wayback/available", server.uri()),
        cdx: format!("{}/cdx/search/cdx", server.uri()),
        replay: format!("{}/web/", server.uri()),
    }
}

fn wayback(server: &MockServer) -> Wayback {
    Wayback::with_endpoints(http(), SourcingSettingsDto::default(), endpoints(server))
}

fn example() -> Url {
    Url::parse("https://example.com/").unwrap()
}

fn at(timestamp: &str) -> DateTime<Utc> {
    NaiveDateTime::parse_from_str(timestamp, CAPTURE_TIMESTAMP)
        .unwrap()
        .and_utc()
}

#[test]
fn availability_answers_read_as_documented() {
    assert_eq!(
        capture_from_availability(AVAILABILITY_HIT).unwrap(),
        Some(Capture {
            timestamp: "20150102000131".to_string(),
            captured_at: Utc.with_ymd_and_hms(2015, 1, 2, 0, 1, 31).unwrap(),
        })
    );
    assert_eq!(capture_from_availability(AVAILABILITY_NONE).unwrap(), None);
    // A capture of a redirect is not a readable copy.
    assert_eq!(
        capture_from_availability(AVAILABILITY_REDIRECT).unwrap(),
        None
    );
    assert!(matches!(
        capture_from_availability("<html>busy</html>"),
        Err(FailureCause::Parse(_))
    ));
    assert!(matches!(
        capture_from_availability(r#"{"url": "example.com"}"#),
        Err(FailureCause::Parse(_))
    ));
}

#[test]
fn cdx_answers_read_by_their_header_row() {
    assert_eq!(
        capture_from_cdx(CDX_NEAREST)
            .unwrap()
            .map(|c| c.captured_at),
        Some(at("20141231120000"))
    );
    // Columns are found by name, not position.
    assert_eq!(
        capture_from_cdx(r#"[["statuscode","timestamp"],["200","20141231120000"]]"#)
            .unwrap()
            .map(|c| c.timestamp),
        Some("20141231120000".to_string())
    );
    assert_eq!(capture_from_cdx("[]").unwrap(), None);
    assert_eq!(capture_from_cdx("").unwrap(), None);
    assert_eq!(
        capture_from_cdx(r#"[["timestamp","statuscode"]]"#).unwrap(),
        None
    );
    assert!(matches!(
        capture_from_cdx(r#"[["original"],["https://example.com/"]]"#),
        Err(FailureCause::Parse(_))
    ));
}

#[test]
fn nearest_means_the_day_asked_for_or_now() {
    let now = Utc.with_ymd_and_hms(2026, 10, 18, 7, 52, 35).unwrap();
    assert_eq!(
        target_stamp(NaiveDate::from_ymd_opt(2015, 1, 1), now),
        "20150101"
    );
    assert_eq!(target_stamp(None, now), "20261018075235");
}

/// Availability answers first, under the declared agent; CDX is not asked
/// when availability holds a readable capture.
#[tokio::test]
async fn an_availability_hit_is_found_without_asking_cdx() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/wayback/available"))
        .and(query_param("url", "https://example.com/"))
        .and(header_regex("user-agent", r"^Lattice/"))
        .respond_with(ResponseTemplate::new(200).set_body_string(AVAILABILITY_HIT))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/cdx/search/cdx"))
        .respond_with(ResponseTemplate::new(200).set_body_string(CDX_NEAREST))
        .expect(0)
        .mount(&server)
        .await;

    let lookup = wayback(&server).lookup(&example(), None).await;
    assert_eq!(
        lookup,
        Lookup::Found(Capture {
            timestamp: "20150102000131".to_string(),
            captured_at: at("20150102000131"),
        })
    );
}

/// A dated lookup sends the day to availability and, when its closest
/// capture is not a 200, asks CDX for the 200 capture nearest that day.
#[tokio::test]
async fn a_dated_lookup_takes_the_cdx_nearest_capture() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/wayback/available"))
        .and(query_param("timestamp", "20150101"))
        .respond_with(ResponseTemplate::new(200).set_body_string(AVAILABILITY_REDIRECT))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/cdx/search/cdx"))
        .and(query_param("url", "https://example.com/"))
        .and(query_param("closest", "20150101"))
        .and(query_param("sort", "closest"))
        .and(query_param("filter", "statuscode:200"))
        .and(query_param("limit", "1"))
        .and(query_param("output", "json"))
        .and(header_regex("user-agent", r"^Lattice/"))
        .respond_with(ResponseTemplate::new(200).set_body_string(CDX_NEAREST))
        .expect(1)
        .mount(&server)
        .await;

    let day = NaiveDate::from_ymd_opt(2015, 1, 1);
    match wayback(&server).lookup(&example(), day).await {
        Lookup::Found(capture) => assert_eq!(capture.captured_at, at("20141231120000")),
        other => panic!("expected the CDX capture, got {other:?}"),
    }
}

/// Both indexes answered and neither holds a copy: settled.
#[tokio::test]
async fn both_indexes_empty_is_nothing_archived() {
    let server = MockServer::start().await;
    Mock::given(path("/wayback/available"))
        .respond_with(ResponseTemplate::new(200).set_body_string(AVAILABILITY_NONE))
        .mount(&server)
        .await;
    Mock::given(path("/cdx/search/cdx"))
        .respond_with(ResponseTemplate::new(200).set_body_string("[]"))
        .mount(&server)
        .await;

    assert_eq!(
        wayback(&server).lookup(&example(), None).await,
        Lookup::NothingArchived
    );
}

/// CDX's 429 (it carried no `Retry-After` when seen live) opens its breaker
/// at once; the lookup says so, and the next lookup skips CDX without a
/// request and names the pause.
#[tokio::test]
async fn a_cdx_429_opens_its_breaker_and_is_reported() {
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
    let wayback = wayback(&server);

    match wayback.lookup(&example(), None).await {
        Lookup::Unanswered(note) => {
            assert!(note.contains("wayback_cdx: failed (HTTP 429)"), "{note}");
            assert!(note.contains("wayback_available: empty"), "{note}");
        }
        other => panic!("a refused index is not an answer: {other:?}"),
    }
    assert!(wayback.paused_until(&CDX_ID).is_some());

    match wayback.lookup(&example(), None).await {
        Lookup::Unanswered(note) => {
            assert!(
                note.contains("wayback_cdx: skipped (paused until"),
                "{note}"
            );
            assert!(note.contains("HTTP 429"), "{note}");
        }
        other => panic!("a paused index is not an answer: {other:?}"),
    }
}

/// A `Retry-After` from the archive holds the endpoint for exactly that.
#[tokio::test]
async fn an_archive_retry_after_is_honoured_in_full() {
    let server = MockServer::start().await;
    Mock::given(path("/wayback/available"))
        .respond_with(ResponseTemplate::new(429).insert_header("retry-after", "3600"))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(path("/cdx/search/cdx"))
        .respond_with(ResponseTemplate::new(200).set_body_string("[]"))
        .mount(&server)
        .await;
    let wayback = wayback(&server);
    let before = Utc::now();

    assert!(matches!(
        wayback.lookup(&example(), None).await,
        Lookup::Unanswered(_)
    ));
    let until = wayback.paused_until(&AVAILABILITY_ID).unwrap();
    assert!(until - before >= chrono::Duration::seconds(3600));
}

/// With the archive fallback off nothing is sent anywhere.
#[tokio::test]
async fn the_archive_switch_off_sends_nothing() {
    let server = MockServer::start().await;
    Mock::given(wiremock::matchers::any())
        .respond_with(ResponseTemplate::new(200))
        .expect(0)
        .mount(&server)
        .await;
    let settings = SourcingSettingsDto {
        archive_fallback: false,
        ..SourcingSettingsDto::default()
    };
    let wayback = Wayback::with_endpoints(http(), settings, endpoints(&server));

    assert!(wayback.is_off());
    assert_eq!(
        wayback.lookup(&example(), None).await,
        Lookup::NothingArchived
    );
}

/// The replay asks for the capture's own bytes (`id_`) and labels the copy
/// with the capture it served and the page a person would open.
#[tokio::test]
async fn a_replay_reads_the_raw_capture() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path_regex(
            r"^/web/20150102000131id_/https:/+example\.com/$",
        ))
        .and(header_regex("user-agent", r"^Lattice/"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            "<html><body><p>Archived words.</p></body></html>",
            "text/html; charset=utf-8",
        ))
        .expect(1)
        .mount(&server)
        .await;
    let capture = Capture {
        timestamp: "20150102000131".to_string(),
        captured_at: at("20150102000131"),
    };

    let replayed = wayback(&server).replay(&example(), &capture).await.unwrap();
    assert!(replayed.html.contains("Archived words."));
    assert_eq!(replayed.captured_at, capture.captured_at);
    assert_eq!(
        replayed.snapshot_url,
        format!("{}/web/20150102000131/https://example.com/", server.uri())
    );
    assert_eq!(
        replayed.content_type.as_deref(),
        Some("text/html; charset=utf-8")
    );
}

/// A capture the replay will not serve is gone; a replay that failed is not.
#[tokio::test]
async fn a_replay_refusal_is_gone_and_a_failure_is_unanswered() {
    let server = MockServer::start().await;
    Mock::given(path_regex(r"^/web/20150102000131id_/"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;
    Mock::given(path_regex(r"^/web/20160101000000id_/"))
        .respond_with(ResponseTemplate::new(503))
        .mount(&server)
        .await;
    let wayback = wayback(&server);
    let capture = |timestamp: &str| Capture {
        timestamp: timestamp.to_string(),
        captured_at: at(timestamp),
    };

    assert!(matches!(
        wayback.replay(&example(), &capture("20150102000131")).await,
        Err(ReplayMiss::Gone(_))
    ));
    assert!(matches!(
        wayback.replay(&example(), &capture("20160101000000")).await,
        Err(ReplayMiss::Unanswered(_))
    ));
}

//! The three finders against fixture servers: what each sends, and how each
//! reads answers, refusals, challenges and a host's `Retry-After`.

use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::{TimeZone, Utc};
use lattice_core::application::ports::{Budget, SourceFinder};
use lattice_core::domain::sourcing::{
    FailureCause, FinderResult, Freshness, NeedKind, Origin, SourceNeed,
};
use reqwest::header::{HeaderMap, HeaderValue, RETRY_AFTER};
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;
use wiremock::matchers::{header_regex, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::*;
use crate::features::web::services::web::SitePacer;

fn http() -> FinderHttp {
    FinderHttp {
        client: reqwest::Client::new(),
        pacer: Arc::new(SitePacer::with_interval(Duration::ZERO)),
        parsers: Arc::new(Semaphore::new(2)),
        contact: None,
    }
}

fn need(query: &str) -> SourceNeed {
    SourceNeed {
        kind: NeedKind::Web,
        query: query.to_string(),
        subject: None,
        scope: None,
        freshness: Freshness::Any,
        want: 10,
        alternatives: Vec::new(),
        also: Vec::new(),
    }
}

fn budget() -> Budget {
    Budget {
        deadline: Instant::now() + Duration::from_secs(60),
        stop: CancellationToken::new(),
        requests: None,
    }
}

const CHALLENGE: &str = r#"<html><body>
    <div class="anomaly-modal__title">Unfortunately, bots use DuckDuckGo too.</div>
</body></html>"#;

fn results_page(urls: &[&str]) -> String {
    let items: String = urls
        .iter()
        .map(|url| {
            format!(
                r#"<div class="result"><a class="result__a" href="{url}">Title for {url}</a>
                   <div class="result__snippet">About {url}</div></div>"#
            )
        })
        .collect();
    format!("<html><body>{items}</body></html>")
}

/// A challenge is reported as one, and no other endpoint of the same host
/// is tried: asking again by another door is what turns it into a block.
#[tokio::test]
async fn a_duckduckgo_challenge_is_reported_and_no_other_endpoint_is_asked() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/html/"))
        .respond_with(ResponseTemplate::new(200).set_body_string(CHALLENGE))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/lite/"))
        .respond_with(ResponseTemplate::new(200).set_body_string(results_page(&[])))
        .expect(0)
        .mount(&server)
        .await;
    let finder = duckduckgo::DuckDuckGo::with_endpoints(
        http(),
        vec![
            format!("{}/html/", server.uri()),
            format!("{}/lite/", server.uri()),
        ],
    );

    let outcome = finder.find(&need("rust"), 1, &budget()).await;

    assert_eq!(
        outcome.result,
        FinderResult::Failed(FailureCause::Challenge)
    );
    assert!(outcome.leads.is_empty());
}

/// An endpoint that is gone (404) gives way to the next one.
#[tokio::test]
async fn a_gone_duckduckgo_endpoint_falls_through_to_the_next() {
    let server = MockServer::start().await;
    Mock::given(path("/html/"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;
    Mock::given(path("/lite/"))
        .respond_with(
            ResponseTemplate::new(200).set_body_string(results_page(&["https://a.example/one"])),
        )
        .mount(&server)
        .await;
    let finder = duckduckgo::DuckDuckGo::with_endpoints(
        http(),
        vec![
            format!("{}/html/", server.uri()),
            format!("{}/lite/", server.uri()),
        ],
    );

    let outcome = finder.find(&need("rust"), 1, &budget()).await;

    assert_eq!(outcome.result, FinderResult::Answered);
    assert_eq!(outcome.leads[0].url.as_str(), "https://a.example/one");
}

/// Page 2 asks for the second ten (`s=10`) and ranks its leads 11 and up,
/// so fusion reads them as deeper than page 1's; a scope rides as `site:`.
#[tokio::test]
async fn a_second_duckduckgo_page_is_asked_by_offset_and_ranked_after_the_first() {
    let server = MockServer::start().await;
    Mock::given(path("/html/"))
        .and(query_param("s", "10"))
        .and(query_param("q", "site:docs.rs tokio"))
        .respond_with(ResponseTemplate::new(200).set_body_string(results_page(&[
            "https://docs.rs/tokio/a",
            "https://docs.rs/tokio/b",
        ])))
        .expect(1)
        .mount(&server)
        .await;
    let finder =
        duckduckgo::DuckDuckGo::with_endpoints(http(), vec![format!("{}/html/", server.uri())]);
    let mut scoped = need("tokio");
    scoped.scope = Origin::parse("docs.rs");

    let outcome = finder.find(&scoped, 2, &budget()).await;

    let ranks: Vec<usize> = outcome
        .leads
        .iter()
        .map(|lead| lead.found_by[0].1)
        .collect();
    assert_eq!(ranks, [11, 12]);
    assert_eq!(
        outcome.leads[0].snippet.as_deref(),
        Some("About https://docs.rs/tokio/a")
    );
}

/// A 429 with `Retry-After: 60` is reported with the wait, for the breaker
/// to hold the finder exactly that long.
#[tokio::test]
async fn a_bing_429_carries_its_retry_after() {
    let server = MockServer::start().await;
    Mock::given(path("/search"))
        .respond_with(ResponseTemplate::new(429).insert_header("Retry-After", "60"))
        .mount(&server)
        .await;
    let finder = bing_html::BingHtml::with_endpoint(http(), format!("{}/search", server.uri()));

    let outcome = finder.find(&need("rust"), 1, &budget()).await;

    assert_eq!(
        outcome.result,
        FinderResult::Failed(FailureCause::Status {
            code: 429,
            retry_after: Some(Duration::from_secs(60)),
        })
    );
}

/// Bing's automated-query refusal is a challenge, not an empty page.
#[tokio::test]
async fn a_bing_refusal_page_is_a_challenge() {
    let server = MockServer::start().await;
    Mock::given(path("/search"))
        .respond_with(
            ResponseTemplate::new(200).set_body_string(
                "<html><title>403 - Forbidden</title><p>automated queries</p></html>",
            ),
        )
        .mount(&server)
        .await;
    let finder = bing_html::BingHtml::with_endpoint(http(), format!("{}/search", server.uri()));

    let outcome = finder.find(&need("rust"), 1, &budget()).await;

    assert_eq!(
        outcome.result,
        FinderResult::Failed(FailureCause::Challenge)
    );
}

/// Wikipedia is an API: it is asked as a declared client (Wikimedia's
/// User-Agent policy), never with a browser's agent.
#[tokio::test]
async fn wikipedia_is_asked_as_a_declared_client() {
    let server = MockServer::start().await;
    Mock::given(path("/w/api.php"))
        .and(header_regex("user-agent", r"^Lattice/\S+ \(\+https://"))
        .and(query_param("srlimit", "10"))
        .and(query_param("sroffset", "0"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "query": { "search": [
                { "title": "Rust (programming language)", "snippet": "A <span class=\"searchmatch\">language</span>" },
                { "title": "Tokio", "snippet": "" }
            ]}
        })))
        .expect(1)
        .mount(&server)
        .await;
    let finder = wikipedia::Wikipedia::with_api(http(), format!("{}/w/api.php", server.uri()));

    let outcome = finder.find(&need("rust"), 1, &budget()).await;

    assert_eq!(outcome.result, FinderResult::Answered);
    let urls: Vec<&str> = outcome.leads.iter().map(|lead| lead.url.as_str()).collect();
    assert_eq!(
        urls,
        [
            "https://en.wikipedia.org/wiki/Rust_(programming_language)",
            "https://en.wikipedia.org/wiki/Tokio"
        ]
    );
    assert_eq!(outcome.leads[0].snippet.as_deref(), Some("A language"));
    assert_eq!(outcome.leads[1].snippet, None);
}

/// The API's own rate-limit error, delivered in a 200, is a 429.
#[tokio::test]
async fn wikipedia_ratelimited_error_is_a_refusal() {
    let server = MockServer::start().await;
    Mock::given(path("/w/api.php"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "error": { "code": "ratelimited", "info": "slow down" }
        })))
        .mount(&server)
        .await;
    let finder = wikipedia::Wikipedia::with_api(http(), format!("{}/w/api.php", server.uri()));

    let outcome = finder.find(&need("rust"), 1, &budget()).await;

    assert_eq!(
        outcome.result,
        FinderResult::Failed(FailureCause::Status {
            code: 429,
            retry_after: None,
        })
    );
}

#[test]
fn retry_after_reads_seconds_and_http_dates() {
    let now = Utc.with_ymd_and_hms(2026, 10, 18, 14, 0, 0).unwrap();
    let mut headers = HeaderMap::new();
    headers.insert(RETRY_AFTER, HeaderValue::from_static("60"));
    assert_eq!(retry_after(&headers, now), Some(Duration::from_secs(60)));

    headers.insert(
        RETRY_AFTER,
        HeaderValue::from_static("Sun, 18 Oct 2026 14:02:00 GMT"),
    );
    assert_eq!(retry_after(&headers, now), Some(Duration::from_secs(120)));

    // A date already past means "now", not an error.
    headers.insert(
        RETRY_AFTER,
        HeaderValue::from_static("Sun, 18 Oct 2026 13:00:00 GMT"),
    );
    assert_eq!(retry_after(&headers, now), Some(Duration::ZERO));

    headers.insert(RETRY_AFTER, HeaderValue::from_static("soon"));
    assert_eq!(retry_after(&headers, now), None);
    assert_eq!(retry_after(&HeaderMap::new(), now), None);
}

#[test]
fn the_keyless_registry_holds_the_three_providers_with_honest_identities() {
    let registry = keyless_registry(&http());
    let ids: Vec<&str> = registry
        .iter()
        .map(|entry| entry.descriptor().id.as_str())
        .collect();
    assert_eq!(ids, ["duckduckgo", "bing_html", "wikipedia"]);
    use lattice_core::domain::sourcing::Identity;
    assert_eq!(wikipedia::DESCRIPTOR.identity, Identity::DeclaredClient);
    assert_eq!(duckduckgo::DESCRIPTOR.identity, Identity::BrowserLike);
    assert!(duckduckgo::DESCRIPTOR.rate.trip_on_first_refusal);
    assert!(!wikipedia::DESCRIPTOR.rate.trip_on_first_refusal);
}

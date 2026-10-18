//! Wiremock tests for Owner site discovery (robots.txt, sitemaps, llms.txt).

use std::sync::Arc;
use std::time::Duration;

use lattice_core::application::ports::{Budget, SourceFinder};
use lattice_core::domain::sourcing::{
    FinderResult, Freshness, LeadKind, NeedKind, Origin, SourceNeed,
};
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;
use wiremock::matchers::{method, path};
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

fn budget() -> Budget {
    Budget {
        deadline: std::time::Instant::now() + Duration::from_secs(60),
        stop: CancellationToken::new(),
        requests: None,
    }
}

#[tokio::test]
async fn site_discovery_llms_and_sitemap_term_matching_and_owner() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/robots.txt"))
        .respond_with(ResponseTemplate::new(200).set_body_string("User-agent: *\nAllow: /\n"))
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path("/llms.txt"))
        .respond_with(ResponseTemplate::new(200).set_body_string(
            "# Documentation\n- [Async Guide](/docs/async): Async programming\n- [Sync Guide](/docs/sync): Sync programming\n",
        ))
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path("/sitemap.xml"))
        .respond_with(ResponseTemplate::new(200).set_body_string(format!(
            "<urlset><loc>{}/docs/async-advanced</loc><loc>{}/contact</loc></urlset>",
            server.uri(),
            server.uri()
        )))
        .mount(&server)
        .await;

    let finder = SiteDiscovery::with_scheme_for_test(http(), "http");
    let scope = Origin::parse(&server.uri()).expect("valid origin");
    let need = SourceNeed {
        kind: NeedKind::OfficialDocs,
        query: "async".to_string(),
        subject: None,
        scope: Some(scope.clone()),
        freshness: Freshness::Any,
        want: 10,
        alternatives: Vec::new(),
        also: Vec::new(),
    };

    let outcome = finder.find(&need, 1, &budget()).await;
    assert_eq!(outcome.result, FinderResult::Answered);

    // Matching URLs should be async guide and async-advanced, but NOT sync or contact
    assert_eq!(outcome.leads.len(), 2);

    // Both should have owner set to scope
    assert_eq!(outcome.leads[0].owner, Some(scope.clone()));
    assert_eq!(outcome.leads[0].kind, LeadKind::OfficialSite);
    assert_eq!(outcome.leads[1].owner, Some(scope.clone()));
    assert_eq!(outcome.leads[1].kind, LeadKind::OfficialSite);

    // First lead is the shorter path /docs/async with title "Async Guide"
    assert_eq!(outcome.leads[0].title.as_deref(), Some("Async Guide"));
    assert!(outcome.leads[0].url.as_str().contains("/docs/async"));
    assert!(outcome.leads[1]
        .url
        .as_str()
        .contains("/docs/async-advanced"));
}

#[tokio::test]
async fn site_discovery_robots_disallow_prevents_fetching_private_sitemap() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/robots.txt"))
        .respond_with(ResponseTemplate::new(200).set_body_string(format!(
            "User-agent: *\nDisallow: /private-sitemap.xml\nSitemap: {}/private-sitemap.xml\n",
            server.uri()
        )))
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path("/llms.txt"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;

    // Disallowed sitemap path MUST NOT be fetched:
    Mock::given(method("GET"))
        .and(path("/private-sitemap.xml"))
        .respond_with(ResponseTemplate::new(200).set_body_string("<urlset></urlset>"))
        .expect(0)
        .mount(&server)
        .await;

    let finder = SiteDiscovery::with_scheme_for_test(http(), "http");
    let scope = Origin::parse(&server.uri()).expect("valid origin");
    let need = SourceNeed {
        kind: NeedKind::OfficialDocs,
        query: "test".to_string(),
        subject: None,
        scope: Some(scope),
        freshness: Freshness::Any,
        want: 10,
        alternatives: Vec::new(),
        also: Vec::new(),
    };

    let outcome = finder.find(&need, 1, &budget()).await;
    assert_eq!(outcome.result, FinderResult::Empty);
    assert!(outcome.leads.is_empty());
}

#[tokio::test]
async fn site_discovery_without_scope_returns_empty() {
    let finder = SiteDiscovery::with_scheme_for_test(http(), "http");
    let need = SourceNeed {
        kind: NeedKind::OfficialDocs,
        query: "test".to_string(),
        subject: None,
        scope: None,
        freshness: Freshness::Any,
        want: 10,
        alternatives: Vec::new(),
        also: Vec::new(),
    };

    let outcome = finder.find(&need, 1, &budget()).await;
    assert_eq!(outcome.result, FinderResult::Empty);
    assert!(outcome.leads.is_empty());
}

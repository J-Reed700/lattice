//! Wiremock fixture tests for Track F4 finders: Community, Law, and Keyed engines.

use std::sync::Arc;
use std::time::{Duration, Instant};

use lattice_core::application::ports::{Budget, SourceFinder};
use lattice_core::domain::sourcing::{
    FailureCause, FinderResult, Freshness, Identifier, LeadKind, NeedKind, Origin, SourceNeed,
    SubjectHint, SubjectKind, Terms,
};
use serde_json::json;
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;
use wiremock::matchers::{header, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::brave::Brave;
use super::eurlex_celex::EurlexCelex;
use super::exa::Exa;
use super::federal_register::FederalRegister;
use super::marginalia::Marginalia;
use super::searxng::Searxng;
use super::stack_exchange::StackExchange;
use super::tavily::Tavily;
use super::FinderHttp;
use crate::features::web::services::web::SitePacer;

fn test_http() -> FinderHttp {
    FinderHttp {
        client: reqwest::Client::new(),
        pacer: Arc::new(SitePacer::with_interval(Duration::ZERO)),
        parsers: Arc::new(Semaphore::new(2)),
        contact: None,
    }
}

fn test_budget() -> Budget {
    Budget {
        deadline: Instant::now() + Duration::from_secs(30),
        stop: CancellationToken::new(),
        requests: None,
    }
}

fn make_need(kind: NeedKind, query: &str) -> SourceNeed {
    SourceNeed {
        kind,
        query: query.to_string(),
        subject: None,
        scope: None,
        freshness: Freshness::Any,
        want: 10,
        alternatives: Vec::new(),
        also: Vec::new(),
    }
}

#[tokio::test]
async fn stack_exchange_cc_by_sa_attribution_and_backoff() {
    let server = MockServer::start().await;
    let success_body = json!({
        "items": [{
            "link": "https://stackoverflow.com/questions/123",
            "title": "Rust question",
            "owner": { "display_name": "ferris" },
            "creation_date": 1_700_000_000
        }]
    });

    Mock::given(method("GET"))
        .and(path("/2.3/search/advanced"))
        .and(query_param("q", "rust"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&success_body))
        .mount(&server)
        .await;

    let finder = StackExchange::with_api(test_http(), server.uri(), None);
    let outcome = finder
        .find(&make_need(NeedKind::Community, "rust"), 1, &test_budget())
        .await;
    assert_eq!(outcome.result, FinderResult::Answered);
    assert_eq!(outcome.leads.len(), 1);
    assert_eq!(
        outcome.leads[0].terms,
        Some(Terms::CcBySa {
            post_url: url::Url::parse("https://stackoverflow.com/questions/123").unwrap(),
            author: Some("ferris".to_string()),
        })
    );

    let backoff_server = MockServer::start().await;
    let backoff_body = json!({
        "backoff": 15,
        "items": []
    });
    Mock::given(method("GET"))
        .and(path("/2.3/search/advanced"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&backoff_body))
        .mount(&backoff_server)
        .await;

    let backoff_finder = StackExchange::with_api(test_http(), backoff_server.uri(), None);
    let backoff_outcome = backoff_finder
        .find(&make_need(NeedKind::Community, "rust"), 1, &test_budget())
        .await;
    assert_eq!(
        backoff_outcome.result,
        FinderResult::Failed(FailureCause::Status {
            code: 429,
            retry_after: Some(Duration::from_secs(15)),
        })
    );
}

#[tokio::test]
async fn federal_register_results_and_owner() {
    let server = MockServer::start().await;
    let body = json!({
        "results": [{
            "title": "Safety Standards",
            "html_url": "https://www.federalregister.gov/documents/2024/01/01/safety",
            "abstract": "Proposed safety standard updates.",
            "publication_date": "2024-01-01"
        }]
    });

    Mock::given(method("GET"))
        .and(path("/api/v1/documents.json"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&body))
        .mount(&server)
        .await;

    let finder = FederalRegister::with_api(test_http(), server.uri());
    let outcome = finder
        .find(&make_need(NeedKind::Law, "safety"), 1, &test_budget())
        .await;
    assert_eq!(outcome.result, FinderResult::Answered);
    assert_eq!(outcome.leads.len(), 1);
    assert_eq!(
        outcome.leads[0].owner,
        Some(Origin::new("federalregister.gov", None))
    );
    assert_eq!(outcome.leads[0].title.as_deref(), Some("Safety Standards"));
}

#[tokio::test]
async fn eurlex_celex_resolution() {
    let finder = EurlexCelex::new(test_http());
    let query_need = make_need(NeedKind::Law, "consult 32016R0679 for GDPR");
    let query_outcome = finder.find(&query_need, 1, &test_budget()).await;

    assert_eq!(query_outcome.result, FinderResult::Answered);
    assert_eq!(
        query_outcome.leads[0].url.as_str(),
        "https://eur-lex.europa.eu/legal-content/EN/TXT/?uri=CELEX:32016R0679"
    );
    assert_eq!(
        query_outcome.leads[0].kind,
        LeadKind::Document(Identifier::Celex("32016R0679".to_string()))
    );

    let mut subject_need = make_need(NeedKind::Law, "AI Act");
    subject_need.subject = Some(SubjectHint {
        name: "EU AI Act".to_string(),
        kind: SubjectKind::Law,
        identifiers: vec![Identifier::Celex("32024R1689".to_string())],
    });
    let subject_outcome = finder.find(&subject_need, 1, &test_budget()).await;
    assert_eq!(
        subject_outcome.leads[0].url.as_str(),
        "https://eur-lex.europa.eu/legal-content/EN/TXT/?uri=CELEX:32024R1689"
    );
}

#[tokio::test]
async fn marginalia_non_commercial_terms() {
    let server = MockServer::start().await;
    let body = json!({
        "results": [{
            "url": "https://indie.example/post",
            "title": "Indie Post",
            "description": "Indie web post"
        }]
    });

    Mock::given(method("GET"))
        .and(path("/public/search/indie"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&body))
        .mount(&server)
        .await;

    let finder = Marginalia::with_api(test_http(), server.uri());
    let outcome = finder
        .find(&make_need(NeedKind::Web, "indie"), 1, &test_budget())
        .await;
    assert_eq!(outcome.result, FinderResult::Answered);
    assert_eq!(outcome.leads[0].terms, Some(Terms::NonCommercial));
}

#[tokio::test]
async fn keyed_engines_brave_tavily_exa() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/res/v1/web/search"))
        .and(header("x-subscription-token", "brave-key"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "web": { "results": [{ "url": "https://a.example/brave", "title": "Brave Hit", "description": "desc" }] }
        })))
        .mount(&server)
        .await;

    Mock::given(method("POST"))
        .and(path("/search"))
        .and(header("authorization", "Bearer tavily-key"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "results": [{ "url": "https://a.example/tavily", "title": "Tavily Hit", "content": "snippet" }]
        })))
        .mount(&server)
        .await;

    let brave = Brave::with_api(test_http(), server.uri(), Some("brave-key".to_string()));
    let b_out = brave
        .find(&make_need(NeedKind::Web, "test"), 1, &test_budget())
        .await;
    assert_eq!(b_out.result, FinderResult::Answered);
    assert_eq!(b_out.leads[0].url.as_str(), "https://a.example/brave");

    let tavily = Tavily::with_api(test_http(), server.uri(), Some("tavily-key".to_string()));
    let t_out = tavily
        .find(&make_need(NeedKind::Web, "test"), 1, &test_budget())
        .await;
    assert_eq!(t_out.result, FinderResult::Answered);
    assert_eq!(t_out.leads[0].url.as_str(), "https://a.example/tavily");

    let exa_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/search"))
        .and(header("x-api-key", "exa-key"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "results": [{ "url": "https://a.example/exa", "title": "Exa Hit", "text": "snippet" }]
        })))
        .mount(&exa_server)
        .await;

    let exa = Exa::with_api(test_http(), exa_server.uri(), Some("exa-key".to_string()));
    let e_out = exa
        .find(&make_need(NeedKind::Web, "test"), 1, &test_budget())
        .await;
    assert_eq!(e_out.result, FinderResult::Answered);
    assert_eq!(e_out.leads[0].url.as_str(), "https://a.example/exa");
}

#[tokio::test]
async fn searxng_json_and_403_forbidden() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/search"))
        .and(query_param("format", "json"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "results": [{ "url": "https://a.example/searx", "title": "Searx Hit", "content": "text" }]
        })))
        .mount(&server)
        .await;

    let searxng = Searxng::with_base_url(test_http(), server.uri());
    let ok_out = searxng
        .find(&make_need(NeedKind::Web, "query"), 1, &test_budget())
        .await;
    assert_eq!(ok_out.result, FinderResult::Answered);
    assert_eq!(ok_out.leads[0].url.as_str(), "https://a.example/searx");

    let forbidden_server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/search"))
        .respond_with(ResponseTemplate::new(403))
        .mount(&forbidden_server)
        .await;

    let forbidden_searx = Searxng::with_base_url(test_http(), forbidden_server.uri());
    let err_out = forbidden_searx
        .find(&make_need(NeedKind::Web, "query"), 1, &test_budget())
        .await;
    assert_eq!(
        err_out.result,
        FinderResult::Failed(FailureCause::Status {
            code: 403,
            retry_after: None,
        })
    );
}

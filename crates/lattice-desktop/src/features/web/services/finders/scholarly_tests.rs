//! Wiremock tests for scholarly finders: OpenAlex, Crossref, Semantic Scholar,
//! arXiv, Unpaywall, and Europe PMC.

use std::sync::Arc;
use std::time::{Duration, Instant};

use lattice_core::application::ports::{Budget, SourceFinder};
use lattice_core::domain::sourcing::{
    Auth, FailureCause, FinderResult, Freshness, Identifier, KeyService, LeadKind, NeedKind,
    Origin, SourceNeed, SubjectHint, SubjectKind,
};
use serde_json::json;
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;
use wiremock::matchers::{header, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::arxiv::{self, Arxiv};
use super::crossref::{self, Crossref};
use super::europe_pmc::EuropePmc;
use super::marginalia;
use super::openalex::{self, OpenAlex};
use super::semantic_scholar::SemanticScholar;
use super::unpaywall::Unpaywall;
use super::FinderHttp;
use crate::features::web::services::web::SitePacer;

fn http(contact: Option<&str>) -> FinderHttp {
    FinderHttp {
        client: reqwest::Client::new(),
        pacer: Arc::new(SitePacer::with_interval(Duration::ZERO)),
        parsers: Arc::new(Semaphore::new(2)),
        contact: contact.map(str::to_string),
        public_only: false,
    }
}

fn need(query: &str) -> SourceNeed {
    SourceNeed {
        kind: NeedKind::Paper,
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

#[test]
fn arxiv_rate_policy_sets_min_interval_to_3_seconds() {
    assert_eq!(arxiv::DESCRIPTOR.rate.min_interval, Duration::from_secs(3));
}

#[tokio::test]
async fn openalex_doi_lookup_returns_landing_page_and_oa_copy() {
    let server = MockServer::start().await;
    let doi = "10.1038/s41586-020-2649-2";
    Mock::given(method("GET"))
        .and(path(format!("/works/https://doi.org/{doi}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": "https://openalex.org/W1", "doi": format!("https://doi.org/{doi}"),
            "title": "Cryo-EM SARS-CoV-2", "publication_date": "2020-08-20",
            "primary_location": { "landing_page_url": "https://nature.com/articles/s41586" },
            "open_access": { "oa_url": "https://europepmc.org/articles/PMC7428700" }
        })))
        .expect(1)
        .mount(&server)
        .await;

    let finder = OpenAlex::with_api(http(None), server.uri());
    let outcome = finder.find(&need(doi), 1, &budget()).await;

    assert_eq!(outcome.result, FinderResult::Answered);
    assert_eq!(outcome.leads.len(), 2);
    assert_eq!(
        outcome.leads[0].url.as_str(),
        "https://nature.com/articles/s41586"
    );
    assert_eq!(
        outcome.leads[1].url.as_str(),
        "https://europepmc.org/articles/PMC7428700"
    );
    let exp_kind = LeadKind::Document(Identifier::Doi(doi.to_string()));
    assert_eq!(outcome.leads[0].kind, exp_kind);
    assert_eq!(outcome.leads[1].kind, exp_kind);
}

#[tokio::test]
async fn openalex_query_search_paginates() {
    let server = MockServer::start().await;
    Mock::given(method("GET")).and(path("/works"))
        .and(query_param("search", "quantum")).and(query_param("per-page", "10")).and(query_param("page", "2"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "results": [{
                "id": "https://openalex.org/W999", "doi": "https://doi.org/10.1103/PhysRevLett.1",
                "title": "Quantum Paper", "primary_location": { "landing_page_url": "https://link.aps.org/doi/1" }
            }]
        })))
        .expect(1).mount(&server).await;

    let finder = OpenAlex::with_api(http(None), server.uri());
    let outcome = finder.find(&need("quantum"), 2, &budget()).await;
    assert_eq!(outcome.result, FinderResult::Answered);
    assert_eq!(outcome.leads[0].found_by[0].1, 11);
}

#[tokio::test]
async fn crossref_doi_lookup_returns_publisher_landing_and_references() {
    let server = MockServer::start().await;
    let doi = "10.1038/s41586-020-2649-2";
    Mock::given(method("GET")).and(path(format!("/works/{doi}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "status": "ok",
            "message": {
                "DOI": doi, "URL": format!("http://dx.doi.org/{doi}"),
                "title": ["Cryo-EM structures of SARS-CoV-2"], "abstract": "<jats:p>Abstract</jats:p>",
                "reference": [{ "DOI": "10.1016/j.cell.2020.02.052" }, { "DOI": "10.1126/science.abb2507" }]
            }
        })))
        .expect(1).mount(&server).await;

    let finder = Crossref::with_api(http(Some("user@example.com")), server.uri());
    let mut leads_need = need(doi);
    leads_need.kind = NeedKind::Leads;
    let outcome = finder.find(&leads_need, 1, &budget()).await;

    assert_eq!(outcome.result, FinderResult::Answered);
    assert_eq!(outcome.leads.len(), 3);
    assert_eq!(
        outcome.leads[0].url.as_str(),
        format!("http://dx.doi.org/{doi}")
    );
    assert_eq!(
        outcome.leads[1].url.as_str(),
        "https://doi.org/10.1016/j.cell.2020.02.052"
    );
    assert_eq!(
        outcome.leads[1].kind,
        LeadKind::Document(Identifier::Doi("10.1016/j.cell.2020.02.052".to_string()))
    );
    assert_eq!(outcome.leads[1].found_by[0].1, 2);
    assert_eq!(
        outcome.leads[2].url.as_str(),
        "https://doi.org/10.1126/science.abb2507"
    );
}

#[tokio::test]
async fn semantic_scholar_search_returns_paper_and_oa_pdf() {
    let server = MockServer::start().await;
    Mock::given(method("GET")).and(path("/graph/v1/paper/search"))
        .and(query_param("query", "transformers")).and(query_param("limit", "10")).and(query_param("offset", "0"))
        .and(header("x-api-key", "test-key"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "total": 1, "offset": 0,
            "data": [{
                "paperId": "abc123", "title": "Attention Is All You Need",
                "url": "https://www.semanticscholar.org/paper/abc123", "abstract": "We propose the Transformer.",
                "year": 2017, "externalIds": { "DOI": "10.5555/3295222.3295349" },
                "openAccessPdf": { "url": "https://arxiv.org/pdf/1706.03762.pdf" }
            }]
        })))
        .expect(1).mount(&server).await;

    let finder = SemanticScholar::with_api(http(None), server.uri(), Some("test-key".to_string()));
    let outcome = finder.find(&need("transformers"), 1, &budget()).await;

    assert_eq!(outcome.result, FinderResult::Answered);
    assert_eq!(outcome.leads.len(), 2);
    assert_eq!(
        outcome.leads[0].url.as_str(),
        "https://www.semanticscholar.org/paper/abc123"
    );
    assert_eq!(
        outcome.leads[1].url.as_str(),
        "https://arxiv.org/pdf/1706.03762.pdf"
    );
    let exp = LeadKind::Document(Identifier::Doi("10.5555/3295222.3295349".to_string()));
    assert_eq!(outcome.leads[0].kind, exp);
    assert_eq!(outcome.leads[1].kind, exp);
}

#[tokio::test]
async fn arxiv_search_and_id_lookup_parse_atom_xml() {
    let server = MockServer::start().await;
    let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<feed xmlns="http://www.w3.org/2005/Atom">
  <entry>
    <id>http://arxiv.org/abs/2101.12345v1</id>
    <published>2021-01-27T15:00:00Z</published>
    <title>Deep Residual Learning</title>
    <summary>Neural networks are difficult to train.</summary>
  </entry>
</feed>"#;

    Mock::given(method("GET"))
        .and(path("/api/query"))
        .and(query_param("id_list", "2101.12345"))
        .respond_with(ResponseTemplate::new(200).set_body_string(xml))
        .expect(1)
        .mount(&server)
        .await;

    let finder = Arxiv::with_api(http(None), server.uri());
    let mut arxiv_need = need("unused");
    arxiv_need.subject = Some(SubjectHint {
        name: "Deep Residual Learning".to_string(),
        kind: SubjectKind::Paper,
        identifiers: vec![Identifier::Arxiv("2101.12345".to_string())],
    });

    let outcome = finder.find(&arxiv_need, 1, &budget()).await;
    assert_eq!(outcome.result, FinderResult::Answered);
    assert_eq!(outcome.leads.len(), 1);
    assert_eq!(
        outcome.leads[0].kind,
        LeadKind::Document(Identifier::Arxiv("2101.12345v1".to_string()))
    );
    assert_eq!(outcome.leads[0].owner, Some(Origin::new("arxiv.org", None)));
    assert_eq!(
        outcome.leads[0].title.as_deref(),
        Some("Deep Residual Learning")
    );
    assert_eq!(
        outcome.leads[0].snippet.as_deref(),
        Some("Neural networks are difficult to train.")
    );
}

#[tokio::test]
async fn unpaywall_doi_returns_publisher_landing_and_oa_copy() {
    let server = MockServer::start().await;
    let doi = "10.1038/nature12373";
    Mock::given(method("GET"))
        .and(path(format!("/v2/{doi}")))
        .and(query_param("email", "user@example.com"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "doi": doi, "title": "A map of the brain", "doi_url": format!("https://doi.org/{doi}"),
            "best_oa_location": {
                "url_for_landing_page": "https://europepmc.org/articles/PMC3653229",
                "url_for_pdf": "https://europepmc.org/articles/PMC3653229?pdf=render"
            }
        })))
        .expect(1)
        .mount(&server)
        .await;

    let finder = Unpaywall::with_api(http(Some("user@example.com")), server.uri());
    let outcome = finder.find(&need(doi), 1, &budget()).await;

    assert_eq!(outcome.result, FinderResult::Answered);
    assert_eq!(outcome.leads.len(), 3);
    assert_eq!(
        outcome.leads[0].url.as_str(),
        format!("https://doi.org/{doi}")
    );
    assert_eq!(
        outcome.leads[1].url.as_str(),
        "https://europepmc.org/articles/PMC3653229"
    );
    assert_eq!(
        outcome.leads[2].url.as_str(),
        "https://europepmc.org/articles/PMC3653229?pdf=render"
    );

    let no_contact = Unpaywall::with_api(http(None), server.uri());
    assert_eq!(
        no_contact.find(&need(doi), 1, &budget()).await.result,
        FinderResult::Empty
    );
    assert_eq!(
        finder.find(&need(doi), 2, &budget()).await.result,
        FinderResult::Empty
    );
}

#[tokio::test]
async fn europe_pmc_search_returns_med_article_and_doi() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/europepmc/webservices/rest/search"))
        .and(query_param("query", "CRISPR"))
        .and(query_param("format", "json"))
        .and(query_param("pageSize", "10"))
        .and(query_param("page", "1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "resultList": {
                "result": [{
                    "pmid": "23287718", "doi": "10.1126/science.1232033",
                    "title": "Multiplex genome engineering using CRISPR/Cas systems.",
                    "pubYear": "2013", "pmcid": "PMC3795411"
                }]
            }
        })))
        .expect(1)
        .mount(&server)
        .await;

    let finder = EuropePmc::with_api(http(None), server.uri());
    let outcome = finder.find(&need("CRISPR"), 1, &budget()).await;

    assert_eq!(outcome.result, FinderResult::Answered);
    assert_eq!(outcome.leads.len(), 1);
    assert_eq!(
        outcome.leads[0].url.as_str(),
        "https://europepmc.org/article/MED/23287718"
    );
    assert_eq!(
        outcome.leads[0].kind,
        LeadKind::Document(Identifier::Doi("10.1126/science.1232033".to_string()))
    );
}

#[test]
fn the_sources_with_higher_limits_take_an_optional_key() {
    for (auth, service) in [
        (openalex::DESCRIPTOR.auth, KeyService::OpenAlex),
        (marginalia::DESCRIPTOR.auth, KeyService::Marginalia),
        (crossref::DESCRIPTOR.auth, KeyService::CrossrefPlus),
    ] {
        assert_eq!(auth, Auth::OptionalKey { service });
    }
}

async fn openalex_server(response: ResponseTemplate) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/works"))
        .respond_with(response)
        .mount(&server)
        .await;
    server
}

fn openalex_works() -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(json!({
        "results": [{
            "id": "https://openalex.org/W1", "title": "A paper",
            "primary_location": { "landing_page_url": "https://example.org/a" }
        }]
    }))
}

#[tokio::test]
async fn openalex_with_a_key_sends_it_as_a_bearer_header_and_never_in_the_url() {
    let server = openalex_server(openalex_works()).await;

    let finder = OpenAlex::with_api_key(http(None), server.uri(), Some(" oa-secret ".to_string()));
    let outcome = finder.find(&need("graphs"), 1, &budget()).await;

    assert_eq!(outcome.result, FinderResult::Answered);
    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1);
    let authorization = requests[0].headers.get("authorization").unwrap();
    assert_eq!(authorization.to_str().unwrap(), "Bearer oa-secret");
    let url = requests[0].url.as_str();
    assert!(!url.contains("api_key"), "{url}");
    assert!(!url.contains("oa-secret"), "{url}");
}

#[tokio::test]
async fn openalex_without_a_key_sends_no_authorization_and_still_runs() {
    let server = openalex_server(openalex_works()).await;

    let finder = OpenAlex::with_api(http(None), server.uri());
    let outcome = finder.find(&need("graphs"), 1, &budget()).await;

    assert_eq!(outcome.result, FinderResult::Answered);
    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1);
    assert!(requests[0].headers.get("authorization").is_none());
    assert!(!requests[0].url.as_str().contains("api_key"));
}

#[tokio::test]
async fn openalex_with_a_blank_key_sends_no_authorization() {
    let server = openalex_server(openalex_works()).await;

    let finder = OpenAlex::with_api_key(http(None), server.uri(), Some("   ".to_string()));
    finder.find(&need("graphs"), 1, &budget()).await;

    let requests = server.received_requests().await.unwrap();
    assert!(requests[0].headers.get("authorization").is_none());
}

#[tokio::test]
async fn openalex_spent_daily_budget_holds_the_finder_until_the_reset() {
    let spent = ResponseTemplate::new(429)
        .insert_header("X-RateLimit-Remaining", "0")
        .insert_header("X-RateLimit-Reset", "54000");
    let server = openalex_server(spent).await;

    let finder = OpenAlex::with_api(http(None), server.uri());
    let outcome = finder.find(&need("graphs"), 1, &budget()).await;

    assert_eq!(
        outcome.result,
        FinderResult::Failed(FailureCause::Status {
            code: 429,
            retry_after: Some(Duration::from_secs(54_000)),
        })
    );
}

#[tokio::test]
async fn openalex_429_with_budget_left_names_no_wait() {
    let busy = ResponseTemplate::new(429)
        .insert_header("X-RateLimit-Remaining", "12")
        .insert_header("X-RateLimit-Reset", "54000");
    let server = openalex_server(busy).await;

    let finder = OpenAlex::with_api(http(None), server.uri());
    let outcome = finder.find(&need("graphs"), 1, &budget()).await;

    assert_eq!(
        outcome.result,
        FinderResult::Failed(FailureCause::Status {
            code: 429,
            retry_after: None,
        })
    );
}

async fn crossref_server() -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "status": "ok",
            "message": { "items": [{ "DOI": "10.1000/xyz", "title": ["A paper"] }] }
        })))
        .mount(&server)
        .await;
    server
}

fn mailto_of(request: &wiremock::Request) -> Option<String> {
    request
        .url
        .query_pairs()
        .find(|(name, _)| name == "mailto")
        .map(|(_, value)| value.into_owned())
}

#[tokio::test]
async fn crossref_sends_an_email_contact_as_mailto_on_every_request() {
    let server = crossref_server().await;

    let finder = Crossref::with_api(http(Some(" user@example.org ")), server.uri());
    finder.find(&need("graphs"), 1, &budget()).await;
    finder.find(&need("graphs"), 2, &budget()).await;
    finder.find(&need("10.1000/xyz"), 1, &budget()).await;

    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 3);
    for request in &requests {
        assert_eq!(
            mailto_of(request).as_deref(),
            Some("user@example.org"),
            "{}",
            request.url
        );
    }
}

#[tokio::test]
async fn crossref_keeps_a_url_contact_in_the_headers_only() {
    let server = crossref_server().await;

    let finder = Crossref::with_api(http(Some("https://example.org/about")), server.uri());
    finder.find(&need("graphs"), 1, &budget()).await;
    finder.find(&need("10.1000/xyz"), 1, &budget()).await;

    for request in server.received_requests().await.unwrap() {
        assert_eq!(mailto_of(&request), None, "{}", request.url);
        let agent = request.headers.get("user-agent").unwrap().to_str().unwrap();
        assert!(agent.contains("https://example.org/about"), "{agent}");
    }
}

#[tokio::test]
async fn crossref_sends_mailto_only_for_what_the_shared_check_calls_an_email() {
    for contact in [
        "https://user@host.example/x",
        "mailto:user@example.org",
        "user@example.org/about",
    ] {
        let server = crossref_server().await;
        let finder = Crossref::with_api(http(Some(contact)), server.uri());
        finder.find(&need("graphs"), 1, &budget()).await;

        let requests = server.received_requests().await.unwrap();
        assert_eq!(mailto_of(&requests[0]), None, "{contact}");
        assert_eq!(
            finder.descriptor().rate.min_interval,
            Duration::from_secs(1),
            "{contact}"
        );
    }
}

#[tokio::test]
async fn crossref_sends_no_mailto_without_a_contact() {
    let server = crossref_server().await;

    let finder = Crossref::with_api(http(None), server.uri());
    finder.find(&need("graphs"), 1, &budget()).await;

    let requests = server.received_requests().await.unwrap();
    assert_eq!(mailto_of(&requests[0]), None);
}

#[test]
fn crossref_declares_the_list_limit_of_the_pool_it_uses() {
    let public = Duration::from_secs(1);
    let polite = Duration::from_secs(1) / 3;
    let interval = |contact: Option<&str>, token: Option<&str>| {
        Crossref::with_api_token(
            http(contact),
            "http://unused.invalid".to_string(),
            token.map(str::to_string),
        )
        .descriptor()
        .rate
        .min_interval
    };
    assert_eq!(interval(None, None), public);
    assert_eq!(interval(Some("https://example.org/about"), None), public);
    assert_eq!(interval(Some("user@example.org"), None), polite);
    assert_eq!(interval(Some("user@example.org"), Some("tok")), polite);
}

async fn three_crossref_searches_take(contact: Option<&str>) -> Duration {
    let server = crossref_server().await;
    let finder = Crossref::with_api(http(contact), server.uri());
    let started = Instant::now();
    for _ in 0..3 {
        finder.find(&need("graphs"), 1, &budget()).await;
    }
    started.elapsed()
}

#[tokio::test]
async fn crossref_without_a_contact_is_held_to_one_list_request_a_second() {
    let taken = three_crossref_searches_take(None).await;
    assert!(taken >= Duration::from_secs(2), "{taken:?}");
}

#[tokio::test]
async fn crossref_with_an_email_contact_is_held_to_three_list_requests_a_second() {
    let taken = three_crossref_searches_take(Some("user@example.org")).await;
    assert!(taken >= Duration::from_secs(1) * 2 / 3, "{taken:?}");
    assert!(taken < Duration::from_millis(1900), "{taken:?}");
}

#[tokio::test]
async fn crossref_with_a_plus_token_sends_it_and_keeps_the_polite_pace() {
    let server = crossref_server().await;

    let finder = Crossref::with_api_token(
        http(Some("user@example.org")),
        server.uri(),
        Some(" plus-token ".to_string()),
    );
    finder.find(&need("graphs"), 1, &budget()).await;

    let requests = server.received_requests().await.unwrap();
    let token = requests[0].headers.get("crossref-plus-api-token").unwrap();
    assert_eq!(token.to_str().unwrap(), "Bearer plus-token");
    assert_eq!(
        finder.descriptor().rate.min_interval,
        Duration::from_secs(1) / 3
    );
}

#[tokio::test]
async fn crossref_without_a_token_sends_no_plus_header() {
    let server = crossref_server().await;

    let finder = Crossref::with_api(http(None), server.uri());
    finder.find(&need("graphs"), 1, &budget()).await;

    let requests = server.received_requests().await.unwrap();
    assert!(requests[0].headers.get("crossref-plus-api-token").is_none());
}

#[tokio::test]
async fn crossref_invalid_token_is_a_401_the_finder_reports() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&server)
        .await;

    let finder = Crossref::with_api_token(http(None), server.uri(), Some("bad".to_string()));
    let outcome = finder.find(&need("graphs"), 1, &budget()).await;

    assert_eq!(
        outcome.result,
        FinderResult::Failed(FailureCause::Status {
            code: 401,
            retry_after: None,
        })
    );
}

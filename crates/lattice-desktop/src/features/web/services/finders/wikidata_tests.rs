//! Wiremock tests for Wikidata subject owner resolution.

use std::sync::Arc;
use std::time::Duration;

use lattice_core::application::ports::{Budget, SourceFinder};
use lattice_core::domain::sourcing::{
    FailureCause, FinderResult, Freshness, Identifier, LeadKind, NeedKind, Origin, SourceNeed,
    SubjectHint, SubjectKind,
};
use serde_json::json;
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;
use wiremock::matchers::{method, path, query_param};
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
async fn resolve_subject_dot_net_yields_learn_microsoft_and_github_dotnet() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/w/api.php"))
        .and(query_param("action", "wbsearchentities"))
        .and(query_param("search", ".NET"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "search": [
                {
                    "id": "Q47547466",
                    "title": "Q47547466",
                    "label": ".NET",
                    "description": "software framework"
                }
            ]
        })))
        .expect(1)
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path("/w/api.php"))
        .and(query_param("action", "wbgetentities"))
        .and(query_param("ids", "Q47547466"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "entities": {
                "Q47547466": {
                    "id": "Q47547466",
                    "labels": { "en": { "value": ".NET" } },
                    "descriptions": { "en": { "value": "software framework" } },
                    "claims": {
                        "P856": [
                            {
                                "mainsnak": {
                                    "datavalue": {
                                        "value": "https://learn.microsoft.com/en-us/dotnet"
                                    }
                                }
                            }
                        ],
                        "P1324": [
                            {
                                "mainsnak": {
                                    "datavalue": {
                                        "value": "https://github.com/dotnet"
                                    }
                                }
                            }
                        ]
                    }
                }
            }
        })))
        .expect(1)
        .mount(&server)
        .await;

    let finder = Wikidata::with_api(http(), format!("{}/w/api.php", server.uri()));
    let subject = SubjectHint {
        name: ".NET".to_string(),
        kind: SubjectKind::Software,
        identifiers: Vec::new(),
    };

    let (origins, outcome) = finder.resolve_subject(&subject).await;

    assert_eq!(outcome.result, FinderResult::Answered);
    assert_eq!(
        origins,
        vec![
            Origin::new("learn.microsoft.com", None),
            Origin::new("github.com", Some("dotnet")),
        ]
    );
    assert_eq!(outcome.leads.len(), 2);
    assert_eq!(
        outcome.leads[0].url.as_str(),
        "https://learn.microsoft.com/en-us/dotnet"
    );
    assert_eq!(outcome.leads[0].kind, LeadKind::OfficialSite);
    assert_eq!(
        outcome.leads[0].owner,
        Some(Origin::new("learn.microsoft.com", None))
    );
    assert_eq!(outcome.leads[1].url.as_str(), "https://github.com/dotnet");
    assert_eq!(outcome.leads[1].kind, LeadKind::Repository);
    assert_eq!(
        outcome.leads[1].owner,
        Some(Origin::new("github.com", Some("dotnet")))
    );
}

#[tokio::test]
async fn resolve_subject_with_direct_qid_bypasses_search() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/w/api.php"))
        .and(query_param("action", "wbsearchentities"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "search": [] })))
        .expect(0)
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path("/w/api.php"))
        .and(query_param("action", "wbgetentities"))
        .and(query_param("ids", "Q47547466"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "entities": {
                "Q47547466": {
                    "id": "Q47547466",
                    "labels": { "en": { "value": ".NET" } },
                    "claims": {
                        "P856": [
                            {
                                "mainsnak": {
                                    "datavalue": {
                                        "value": "https://learn.microsoft.com/en-us/dotnet"
                                    }
                                }
                            }
                        ]
                    }
                }
            }
        })))
        .expect(1)
        .mount(&server)
        .await;

    let finder = Wikidata::with_api(http(), format!("{}/w/api.php", server.uri()));
    let subject = SubjectHint {
        name: ".NET".to_string(),
        kind: SubjectKind::Software,
        identifiers: vec![Identifier::WikidataQid("Q47547466".to_string())],
    };

    let (origins, outcome) = finder.resolve_subject(&subject).await;
    assert_eq!(outcome.result, FinderResult::Answered);
    assert_eq!(origins, vec![Origin::new("learn.microsoft.com", None)]);
}

#[tokio::test]
async fn wikidata_source_finder_find_pages() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/w/api.php"))
        .and(query_param("action", "wbsearchentities"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "search": [{ "id": "Q1", "title": "Q1" }]
        })))
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path("/w/api.php"))
        .and(query_param("action", "wbgetentities"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "entities": {
                "Q1": {
                    "id": "Q1",
                    "claims": {
                        "P856": [{ "mainsnak": { "datavalue": { "value": "https://example.org" } } }]
                    }
                }
            }
        })))
        .mount(&server)
        .await;

    let finder = Wikidata::with_api(http(), format!("{}/w/api.php", server.uri()));
    let need = SourceNeed {
        kind: NeedKind::Encyclopedia,
        query: "example".to_string(),
        subject: None,
        scope: None,
        freshness: Freshness::Any,
        want: 5,
        alternatives: Vec::new(),
        also: Vec::new(),
    };

    let page1 = finder.find(&need, 1, &budget()).await;
    assert_eq!(page1.result, FinderResult::Answered);
    assert_eq!(page1.leads.len(), 1);

    let page2 = finder.find(&need, 2, &budget()).await;
    assert_eq!(page2.result, FinderResult::Empty);
    assert!(page2.leads.is_empty());
}

#[tokio::test]
async fn wikidata_api_ratelimited_error() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/w/api.php"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "error": { "code": "ratelimited" }
        })))
        .mount(&server)
        .await;

    let finder = Wikidata::with_api(http(), format!("{}/w/api.php", server.uri()));
    let subject = SubjectHint {
        name: "test".to_string(),
        kind: SubjectKind::Other,
        identifiers: Vec::new(),
    };

    let (origins, outcome) = finder.resolve_subject(&subject).await;
    assert!(origins.is_empty());
    assert_eq!(
        outcome.result,
        FinderResult::Failed(FailureCause::Status {
            code: 429,
            retry_after: None,
        })
    );
}

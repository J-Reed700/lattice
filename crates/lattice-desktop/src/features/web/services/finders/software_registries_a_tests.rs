//! Tests for package registry finders (crates.io, PyPI, npm, NuGet).

use std::sync::Arc;
use std::time::{Duration, Instant};

use lattice_core::application::ports::{Budget, SourceFinder};
use lattice_core::domain::sourcing::{
    Ecosystem, FinderResult, Freshness, Identifier, Identity, LeadKind, NeedKind, Origin, Reach,
    SourceNeed, SubjectHint, SubjectKind,
};
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::crates_io::{self, CratesIo};
use super::npm::{self, Npm};
use super::nuget::{self, Nuget};
use super::pypi::{self, PyPi};
use super::FinderHttp;
use crate::features::web::services::web::{SitePacer, MIN_SITE_REQUEST_INTERVAL};

fn http() -> FinderHttp {
    FinderHttp {
        client: reqwest::Client::new(),
        pacer: Arc::new(SitePacer::with_interval(Duration::ZERO)),
        parsers: Arc::new(Semaphore::new(2)),
        contact: None,
        public_only: false,
    }
}

fn need(eco: Ecosystem, q: &str, exact: bool) -> SourceNeed {
    SourceNeed {
        kind: NeedKind::Package,
        query: q.to_string(),
        subject: exact.then(|| SubjectHint {
            name: q.to_string(),
            kind: SubjectKind::Software,
            identifiers: vec![Identifier::PackageName {
                ecosystem: eco,
                name: q.to_string(),
            }],
        }),
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

#[tokio::test]
async fn registry_descriptors_and_rate_policies() {
    let c = &crates_io::DESCRIPTOR;
    assert_eq!(c.id.as_str(), "crates_io");
    assert_eq!(c.reaches, Reach::Registry(Ecosystem::Crates));
    assert_eq!(c.identity, Identity::DeclaredClient);
    assert_eq!(c.rate.min_interval, Duration::from_secs(1));
    assert_eq!(c.rate.source, "https://crates.io/data-access");

    let p = &pypi::DESCRIPTOR;
    assert_eq!(p.id.as_str(), "pypi");
    assert_eq!(p.reaches, Reach::Registry(Ecosystem::PyPi));
    assert_eq!(p.rate.min_interval, MIN_SITE_REQUEST_INTERVAL);
    assert_eq!(p.rate.source, "https://docs.pypi.org/api/json/");

    let n = &npm::DESCRIPTOR;
    assert_eq!(n.id.as_str(), "npm");
    assert_eq!(n.reaches, Reach::Registry(Ecosystem::Npm));
    assert_eq!(n.rate.min_interval, MIN_SITE_REQUEST_INTERVAL);

    let g = &nuget::DESCRIPTOR;
    assert_eq!(g.id.as_str(), "nuget");
    assert_eq!(g.reaches, Reach::Registry(Ecosystem::NuGet));
    assert_eq!(g.rate.min_interval, MIN_SITE_REQUEST_INTERVAL);
}

#[tokio::test]
async fn crates_io_lookup_and_leads() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/crates/serde"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "crate": {
                "name": "serde",
                "documentation": "https://docs.rs/serde",
                "repository": "https://github.com/serde-rs/serde"
            }
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v1/crates/missing"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;

    let finder = CratesIo::with_api(http(), server.uri());
    let res = finder
        .find(&need(Ecosystem::Crates, "serde", true), 1, &budget())
        .await;
    assert_eq!(res.result, FinderResult::Answered);
    assert_eq!(res.leads.len(), 3);
    assert_eq!(res.leads[0].kind, LeadKind::OfficialSite);
    assert_eq!(res.leads[0].url.as_str(), "https://docs.rs/serde");
    assert_eq!(
        res.leads[0].owner,
        Some(Origin::new("github.com", Some("serde-rs/serde")))
    );
    assert_eq!(res.leads[1].kind, LeadKind::Repository);
    assert_eq!(
        res.leads[2].kind,
        LeadKind::Document(Identifier::PackageName {
            ecosystem: Ecosystem::Crates,
            name: "serde".to_string(),
        })
    );

    let empty = finder
        .find(&need(Ecosystem::Crates, "missing", true), 1, &budget())
        .await;
    assert_eq!(empty.result, FinderResult::Empty);
}

#[tokio::test]
async fn crates_io_search_query_homepage_owner() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/crates"))
        .and(query_param("q", "tokio"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "crates": [{
                "name": "tokio",
                "homepage": "https://tokio.rs",
                "repository": "https://github.com/tokio-rs/tokio"
            }]
        })))
        .mount(&server)
        .await;

    let finder = CratesIo::with_api(http(), server.uri());
    let res = finder
        .find(&need(Ecosystem::Crates, "tokio", false), 1, &budget())
        .await;
    assert_eq!(res.result, FinderResult::Answered);
    assert_eq!(res.leads[1].kind, LeadKind::OfficialSite);
    assert_eq!(res.leads[1].url.as_str(), "https://tokio.rs/");
    assert_eq!(res.leads[1].owner, Some(Origin::new("tokio.rs", None)));
}

#[tokio::test]
async fn pypi_package_lookup_and_leads() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/pypi/requests/json"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "info": {
                "name": "requests",
                "home_page": "https://requests.readthedocs.io",
                "project_urls": {
                    "Documentation": "https://requests.readthedocs.io",
                    "Source": "https://github.com/psf/requests"
                }
            }
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/pypi/missing/json"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;

    let finder = PyPi::with_api(http(), server.uri());
    let res = finder
        .find(&need(Ecosystem::PyPi, "requests", true), 1, &budget())
        .await;
    assert_eq!(res.result, FinderResult::Answered);
    assert_eq!(res.leads.len(), 3);
    assert_eq!(res.leads[0].kind, LeadKind::OfficialSite);
    assert_eq!(
        res.leads[0].owner,
        Some(Origin::new("requests.readthedocs.io", None))
    );
    assert_eq!(res.leads[1].kind, LeadKind::Repository);
    assert_eq!(res.leads[1].url.as_str(), "https://github.com/psf/requests");
    assert_eq!(
        res.leads[2].kind,
        LeadKind::Document(Identifier::PackageName {
            ecosystem: Ecosystem::PyPi,
            name: "requests".to_string(),
        })
    );

    let empty = finder
        .find(&need(Ecosystem::PyPi, "missing", true), 1, &budget())
        .await;
    assert_eq!(empty.result, FinderResult::Empty);
    let p2 = finder
        .find(&need(Ecosystem::PyPi, "requests", true), 2, &budget())
        .await;
    assert_eq!(p2.result, FinderResult::Empty);

    // PyPI has no search: a need that names no package asks it nothing,
    // rather than looking up the question's first word.
    let unnamed = finder
        .find(
            &need(Ecosystem::PyPi, "requests timeout", false),
            1,
            &budget(),
        )
        .await;
    assert_eq!(unnamed.result, FinderResult::Empty);
    assert_eq!(server.received_requests().await.unwrap().len(), 2);
}

#[tokio::test]
async fn npm_search_and_pagination() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/-/v1/search"))
        .and(query_param("text", "express"))
        .and(query_param("size", "10"))
        .and(query_param("from", "0"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "objects": [{
                "package": {
                    "name": "express",
                    "links": {
                        "homepage": "http://expressjs.com/",
                        "repository": "https://github.com/expressjs/express",
                        "npm": "https://www.npmjs.com/package/express"
                    }
                }
            }]
        })))
        .mount(&server)
        .await;

    let finder = Npm::with_api(http(), server.uri());
    let res = finder
        .find(&need(Ecosystem::Npm, "express", false), 1, &budget())
        .await;
    assert_eq!(res.result, FinderResult::Answered);
    assert_eq!(res.leads.len(), 3);
    assert_eq!(res.leads[0].kind, LeadKind::OfficialSite);
    assert_eq!(res.leads[0].owner, Some(Origin::new("expressjs.com", None)));
    assert_eq!(res.leads[1].kind, LeadKind::Repository);
    assert_eq!(
        res.leads[2].kind,
        LeadKind::Document(Identifier::PackageName {
            ecosystem: Ecosystem::Npm,
            name: "express".to_string(),
        })
    );
    assert_eq!(res.leads[2].found_by[0].1, 3);
}

#[tokio::test]
async fn npm_keeps_only_the_named_package_from_a_search() {
    let server = MockServer::start().await;
    let package = |name: &str| {
        serde_json::json!({"package": {
            "name": name,
            "links": {"homepage": format!("https://{name}.example/")}
        }})
    };
    // The search is by the package's name, and the registry ranks packages
    // that only share words alongside it.
    Mock::given(method("GET"))
        .and(path("/-/v1/search"))
        .and(query_param("text", "request"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "objects": [package("request-promise"), package("Request"), package("got")]
        })))
        .mount(&server)
        .await;

    let finder = Npm::with_api(http(), server.uri());
    let mut asked = need(Ecosystem::Npm, "request", true);
    asked.query = "retry a request after a timeout".to_string();
    let res = finder.find(&asked, 1, &budget()).await;
    assert_eq!(res.result, FinderResult::Answered);
    let owners: Vec<_> = res.leads.iter().map(|lead| lead.owner.clone()).collect();
    assert!(!owners.is_empty());
    assert!(
        owners
            .iter()
            .all(|owner| *owner == Some(Origin::new("request.example", None))),
        "only the record named `request` (npm folds case) is kept: {owners:?}"
    );
}

#[tokio::test]
async fn nuget_search_and_github_owner() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/query"))
        .and(query_param("q", "Newtonsoft.Json"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "data": [{
                "id": "Newtonsoft.Json",
                "projectUrl": "https://github.com/JamesNK/Newtonsoft.Json"
            }]
        })))
        .mount(&server)
        .await;

    let finder = Nuget::with_api(http(), server.uri());
    let res = finder
        .find(
            &need(Ecosystem::NuGet, "Newtonsoft.Json", false),
            1,
            &budget(),
        )
        .await;
    assert_eq!(res.result, FinderResult::Answered);
    assert_eq!(res.leads.len(), 2);
    assert_eq!(res.leads[0].kind, LeadKind::OfficialSite);
    assert_eq!(
        res.leads[0].owner,
        Some(Origin::new("github.com", Some("JamesNK/Newtonsoft.Json")))
    );
    assert_eq!(
        res.leads[1].kind,
        LeadKind::Document(Identifier::PackageName {
            ecosystem: Ecosystem::NuGet,
            name: "Newtonsoft.Json".to_string(),
        })
    );
    assert_eq!(
        res.leads[1].url.as_str(),
        "https://www.nuget.org/packages/Newtonsoft.Json"
    );
}

//! Wiremock tests for RubyGems, Maven Central, Go Module Proxy, and GitHub finders.

use std::sync::Arc;
use std::time::{Duration, Instant};

use lattice_core::application::ports::{Budget, SourceFinder};
use lattice_core::domain::sourcing::{
    Ecosystem, FailureCause, FinderResult, Freshness, Identifier, LeadKind, NeedKind, Origin,
    Quota, SourceNeed, SubjectHint, SubjectKind,
};
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;
use wiremock::matchers::{header, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::github::GitHub;
use super::go_module::GoModule;
use super::maven_pom::MavenPom;
use super::rubygems::RubyGems;
use super::FinderHttp;
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
        kind: NeedKind::Package,
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

#[tokio::test]
async fn rubygems_finds_gems_and_derives_owner() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/search.json"))
        .and(query_param("query", "rack"))
        .and(query_param("page", "1"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::json!([{
                "name": "rack",
                "info": "A modular Ruby webserver interface.",
                "homepage_uri": "https://rack.github.io",
                "documentation_uri": "https://rubydoc.info/gems/rack",
                "source_code_uri": "https://github.com/rack/rack",
                "project_uri": "https://rubygems.org/gems/rack"
            }])),
        )
        .mount(&server)
        .await;

    let finder = RubyGems::with_api(http(), server.uri());
    let outcome = finder.find(&need("rack"), 1, &budget()).await;

    assert_eq!(outcome.result, FinderResult::Answered);
    assert_eq!(outcome.leads.len(), 3);
    assert_eq!(outcome.leads[0].kind, LeadKind::OfficialSite);
    assert_eq!(outcome.leads[0].url.as_str(), "https://rack.github.io/");
    assert_eq!(outcome.leads[1].kind, LeadKind::Repository);
    assert_eq!(
        outcome.leads[1].url.as_str(),
        "https://github.com/rack/rack"
    );
    assert_eq!(
        outcome.leads[2].kind,
        LeadKind::Document(Identifier::PackageName {
            ecosystem: Ecosystem::RubyGems,
            name: "rack".to_string(),
        })
    );
    assert_eq!(
        outcome.leads[2].url.as_str(),
        "https://rubygems.org/gems/rack"
    );
    let expected_owner = Origin::parse("https://rack.github.io");
    for lead in &outcome.leads {
        assert_eq!(lead.owner, expected_owner);
    }
}

#[tokio::test]
async fn rubygems_empty_and_error() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/search.json"))
        .and(query_param("query", "nonexistent"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v1/search.json"))
        .and(query_param("query", "fail"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&server)
        .await;

    let finder = RubyGems::with_api(http(), server.uri());
    let empty_outcome = finder.find(&need("nonexistent"), 1, &budget()).await;
    assert_eq!(empty_outcome.result, FinderResult::Empty);
    assert!(empty_outcome.leads.is_empty());

    let fail_outcome = finder.find(&need("fail"), 1, &budget()).await;
    assert_eq!(
        fail_outcome.result,
        FinderResult::Failed(FailureCause::Status {
            code: 500,
            retry_after: None,
        })
    );
}

#[tokio::test]
async fn maven_pom_emits_javadoc_and_central_leads() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/solrsearch/select"))
        .and(query_param("q", "guava"))
        .and(query_param("rows", "20"))
        .and(query_param("start", "0"))
        .and(query_param("wt", "json"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "response": { "docs": [{ "g": "com.google.guava", "a": "guava", "latestVersion": "33.0.0-jre" }] }
        })))
        .mount(&server)
        .await;

    let finder = MavenPom::with_api(http(), server.uri());
    let outcome = finder.find(&need("guava"), 1, &budget()).await;

    assert_eq!(outcome.result, FinderResult::Answered);
    assert_eq!(outcome.leads.len(), 2);

    let expected_owner = Some(Origin::new(
        "javadoc.io",
        Some("doc/com.google.guava/guava"),
    ));
    assert_eq!(outcome.leads[0].kind, LeadKind::OfficialSite);
    assert_eq!(
        outcome.leads[0].url.as_str(),
        "https://javadoc.io/doc/com.google.guava/guava"
    );
    assert_eq!(outcome.leads[0].owner, expected_owner);

    assert_eq!(
        outcome.leads[1].kind,
        LeadKind::Document(Identifier::PackageName {
            ecosystem: Ecosystem::Maven,
            name: "com.google.guava:guava".to_string(),
        })
    );
    assert_eq!(
        outcome.leads[1].url.as_str(),
        "https://central.sonatype.com/artifact/com.google.guava/guava"
    );
    assert_eq!(outcome.leads[1].owner, expected_owner);
}

#[tokio::test]
async fn go_module_proxy_github_and_404() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/github.com/spf13/cobra/@latest"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "Version": "v1.8.0",
            "Time": "2023-11-20T00:00:00Z"
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/nonexistent/@latest"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;

    let finder = GoModule::with_api(http(), server.uri());
    let outcome = finder
        .find(&need("github.com/spf13/cobra"), 1, &budget())
        .await;
    assert_eq!(outcome.result, FinderResult::Answered);
    assert_eq!(outcome.leads.len(), 2);

    let expected_owner = Some(Origin::new("github.com", Some("spf13/cobra")));
    assert_eq!(outcome.leads[0].kind, LeadKind::OfficialSite);
    assert_eq!(
        outcome.leads[0].url.as_str(),
        "https://pkg.go.dev/github.com/spf13/cobra"
    );
    assert_eq!(outcome.leads[0].owner, expected_owner);

    assert_eq!(outcome.leads[1].kind, LeadKind::Repository);
    assert_eq!(
        outcome.leads[1].url.as_str(),
        "https://github.com/spf13/cobra"
    );
    assert_eq!(outcome.leads[1].owner, expected_owner);

    let outcome_404 = finder.find(&need("nonexistent"), 1, &budget()).await;
    assert_eq!(outcome_404.result, FinderResult::Empty);
    assert!(outcome_404.leads.is_empty());
}

#[test]
fn github_descriptor_quota_60_vs_5000() {
    let unauthenticated = GitHub::new(http());
    assert_eq!(
        unauthenticated.descriptor().rate.quota,
        Some(Quota {
            requests: 60,
            per: Duration::from_secs(3600)
        })
    );

    let authenticated = GitHub::with_token(http(), Some("ghp_test123".to_string()));
    assert_eq!(
        authenticated.descriptor().rate.quota,
        Some(Quota {
            requests: 5000,
            per: Duration::from_secs(3600)
        })
    );
}

#[tokio::test]
async fn github_search_dedicated_owner_and_bearer_token() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/search/repositories"))
        .and(query_param("q", "tokio"))
        .and(query_param("per_page", "10"))
        .and(query_param("page", "1"))
        .and(header("authorization", "Bearer ghp_token123"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "items": [{
                "full_name": "tokio-rs/tokio",
                "html_url": "https://github.com/tokio-rs/tokio",
                "description": "An asynchronous runtime for Rust",
                "homepage": "https://tokio.rs"
            }]
        })))
        .mount(&server)
        .await;

    let finder = GitHub::with_api(http(), server.uri(), Some("ghp_token123".to_string()));
    let outcome = finder.find(&need("tokio"), 1, &budget()).await;

    assert_eq!(outcome.result, FinderResult::Answered);
    assert_eq!(outcome.leads.len(), 2);

    let expected_owner = Origin::parse("https://tokio.rs");
    assert_eq!(outcome.leads[0].kind, LeadKind::OfficialSite);
    assert_eq!(outcome.leads[0].url.as_str(), "https://tokio.rs/");
    assert_eq!(outcome.leads[0].owner, expected_owner);

    assert_eq!(outcome.leads[1].kind, LeadKind::Repository);
    assert_eq!(
        outcome.leads[1].url.as_str(),
        "https://github.com/tokio-rs/tokio"
    );
    assert_eq!(outcome.leads[1].owner, expected_owner);
}

#[tokio::test]
async fn github_direct_repo_lookup_and_github_owner_extraction() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/repos/serde-rs/serde"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "full_name": "serde-rs/serde",
            "html_url": "https://github.com/serde-rs/serde",
            "description": "Serialization framework for Rust",
            "homepage": "https://github.com/serde-rs/serde"
        })))
        .mount(&server)
        .await;

    let finder = GitHub::with_api(http(), server.uri(), None);
    let mut repo_need = need("serde");
    repo_need.subject = Some(SubjectHint {
        name: "serde".to_string(),
        kind: SubjectKind::Software,
        identifiers: vec![Identifier::Repo {
            host: "github.com".to_string(),
            owner: "serde-rs".to_string(),
            name: "serde".to_string(),
        }],
    });

    let outcome = finder.find(&repo_need, 1, &budget()).await;

    assert_eq!(outcome.result, FinderResult::Answered);
    assert_eq!(outcome.leads.len(), 2);

    let expected_owner = Some(Origin::new("github.com", Some("serde-rs/serde")));
    assert_eq!(outcome.leads[0].kind, LeadKind::OfficialSite);
    assert_eq!(outcome.leads[0].owner, expected_owner);

    assert_eq!(outcome.leads[1].kind, LeadKind::Repository);
    assert_eq!(outcome.leads[1].owner, expected_owner);
}

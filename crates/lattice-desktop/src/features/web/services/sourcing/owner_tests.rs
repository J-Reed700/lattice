//! Unit and integration tests for OwnerResolver: identifier short-circuiting,
//! Wikidata resolution, in-memory and SQLite cache reuse.

use std::sync::Arc;
use std::time::Duration;

use chrono::{TimeZone, Utc};
use futures::future::BoxFuture;
use lattice_core::application::ports::Budget;
use lattice_core::domain::sourcing::{
    FinderResult, Identifier, Origin, OwnerSource, SourcingPolicy, SubjectHint, SubjectKind,
};
use serde_json::json;
use sqlx::SqlitePool;
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::*;
use crate::features::web::services::finders::FinderHttp;
use crate::features::web::services::web::SitePacer;

struct FakeClock {
    now: parking_lot::Mutex<DateTime<Utc>>,
}

impl FakeClock {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            now: parking_lot::Mutex::new(Utc.with_ymd_and_hms(2026, 10, 18, 12, 0, 0).unwrap()),
        })
    }

    fn advance(&self, duration: Duration) {
        *self.now.lock() += chrono::TimeDelta::from_std(duration).unwrap();
    }
}

impl Clock for FakeClock {
    fn now(&self) -> DateTime<Utc> {
        *self.now.lock()
    }

    fn sleep(&self, wait: Duration) -> BoxFuture<'static, ()> {
        self.advance(wait);
        Box::pin(async {})
    }
}

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

async fn setup_db() -> SqlitePool {
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
    sqlx::query(
        r#"CREATE TABLE IF NOT EXISTS sourcing_owner_cache (
            cache_key TEXT PRIMARY KEY NOT NULL,
            subject_name TEXT NOT NULL,
            subject_kind TEXT NOT NULL,
            origins_json TEXT NOT NULL,
            resolved_by TEXT,
            updated_at INTEGER NOT NULL
        );"#,
    )
    .execute(&pool)
    .await
    .unwrap();
    pool
}

fn resolver(
    server: Option<&MockServer>,
    pool: Option<SqlitePool>,
    clock: Arc<FakeClock>,
) -> OwnerResolver {
    let wikidata = match server {
        Some(s) => Arc::new(Wikidata::with_api(http(), format!("{}/w/api.php", s.uri()))),
        None => Arc::new(Wikidata::new(http())),
    };
    OwnerResolver::new(
        wikidata,
        Arc::new(Wikipedia::new(http())),
        pool,
        SourcingPolicy::default(),
        clock,
    )
}

fn subject(name: &str, ids: Vec<Identifier>) -> SubjectHint {
    SubjectHint {
        name: name.to_string(),
        kind: SubjectKind::Software,
        identifiers: ids,
    }
}

async fn mock_wikidata(server: &MockServer, qid: &str, site_url: &str, expected: u64) {
    Mock::given(method("GET"))
        .and(path("/w/api.php"))
        .and(query_param("action", "wbsearchentities"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "search": [{ "id": qid, "title": qid }]
        })))
        .expect(expected)
        .mount(server)
        .await;

    Mock::given(method("GET")).and(path("/w/api.php")).and(query_param("action", "wbgetentities"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "entities": {
                qid: { "id": qid, "claims": { "P856": [{ "mainsnak": { "datavalue": { "value": site_url } } }] } }
            }
        })))
        .expect(expected)
        .mount(server)
        .await;
}

#[tokio::test]
async fn owner_resolution_short_circuits_repo_identifier() {
    let res = resolver(None, None, FakeClock::new());
    let sub = subject(
        "tokio",
        vec![Identifier::Repo {
            host: "github.com".into(),
            owner: "tokio-rs".into(),
            name: "tokio".into(),
        }],
    );

    let r = res.resolve(&sub, &budget()).await;
    assert_eq!(
        r.origins,
        vec![Origin::new("github.com", Some("tokio-rs/tokio"))]
    );
    assert_eq!(r.resolved_by, Some(OwnerSource::GitHub));
    assert!(r.outcomes.is_empty());
}

#[tokio::test]
async fn owner_resolution_short_circuits_url_identifier() {
    let res = resolver(None, None, FakeClock::new());
    let sub = subject(
        "tokio",
        vec![Identifier::Url(
            Url::parse("https://crates.io/crates/tokio").unwrap(),
        )],
    );

    let r = res.resolve(&sub, &budget()).await;
    assert_eq!(r.origins, vec![Origin::new("crates.io", None)]);
    assert_eq!(r.resolved_by, Some(OwnerSource::Registry));
    assert!(r.outcomes.is_empty());
}

#[tokio::test]
async fn owner_resolution_queries_wikidata_and_caches_in_memory() {
    let server = MockServer::start().await;
    mock_wikidata(&server, "Q100", "https://tokio.rs", 1).await;

    let res = resolver(Some(&server), None, FakeClock::new());
    let sub = subject("tokio", Vec::new());

    // First call: queries Wikidata
    let r1 = res.resolve(&sub, &budget()).await;
    assert_eq!(r1.origins, vec![Origin::new("tokio.rs", None)]);
    assert_eq!(r1.resolved_by, Some(OwnerSource::Wikidata));
    assert_eq!(r1.outcomes.len(), 1);
    assert_eq!(r1.outcomes[0].result, FinderResult::Answered);

    // Second call: served from in-memory cache, no new HTTP request
    let r2 = res.resolve(&sub, &budget()).await;
    assert_eq!(r2.origins, vec![Origin::new("tokio.rs", None)]);
    assert_eq!(r2.resolved_by, Some(OwnerSource::Wikidata));
    assert!(r2.outcomes.is_empty());
}

#[tokio::test]
async fn owner_resolution_persists_to_and_reads_from_sqlite_cache() {
    let server = MockServer::start().await;
    let pool = setup_db().await;
    mock_wikidata(&server, "Q200", "https://rust-lang.org", 1).await;

    let clock = FakeClock::new();
    let res1 = resolver(Some(&server), Some(pool.clone()), clock.clone());
    let sub = subject("Rust", Vec::new());

    // First call with res1 writes to SQLite
    let r1 = res1.resolve(&sub, &budget()).await;
    assert_eq!(r1.origins, vec![Origin::new("rust-lang.org", None)]);
    assert_eq!(r1.resolved_by, Some(OwnerSource::Wikidata));

    // Fresh res2 with empty in-memory cache reads from SQLite
    let res2 = resolver(Some(&server), Some(pool), clock);
    let r2 = res2.resolve(&sub, &budget()).await;
    assert_eq!(r2.origins, vec![Origin::new("rust-lang.org", None)]);
    assert_eq!(r2.resolved_by, Some(OwnerSource::Wikidata));
    assert!(r2.outcomes.is_empty());
}

#[tokio::test]
async fn owner_resolution_stale_cache_refetches() {
    let server = MockServer::start().await;
    mock_wikidata(&server, "Q300", "https://example.org", 2).await;

    let clock = FakeClock::new();
    let res = resolver(Some(&server), None, clock.clone());
    let sub = subject("stale-test", Vec::new());

    let r1 = res.resolve(&sub, &budget()).await;
    assert_eq!(r1.origins, vec![Origin::new("example.org", None)]);

    // Advance clock past 30 days (owner_ttl is 30 days)
    clock.advance(Duration::from_secs(31 * 24 * 60 * 60));

    // Stale entry triggers re-fetch from Wikidata
    let r2 = res.resolve(&sub, &budget()).await;
    assert_eq!(r2.origins, vec![Origin::new("example.org", None)]);
    assert_eq!(r2.outcomes.len(), 1);
}

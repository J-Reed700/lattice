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
        public_only: false,
    }
}

fn budget() -> Budget {
    Budget {
        deadline: std::time::Instant::now() + Duration::from_secs(60),
        stop: CancellationToken::new(),
        requests: None,
    }
}

async fn setup_db() -> Database {
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
    Database::new(pool)
}

fn resolver(
    server: Option<&MockServer>,
    db: Option<Database>,
    clock: Arc<FakeClock>,
) -> OwnerResolver {
    let wikidata = match server {
        Some(s) => Arc::new(Wikidata::with_api(http(), format!("{}/w/api.php", s.uri()))),
        None => Arc::new(Wikidata::new(http())),
    };
    OwnerResolver::new(
        wikidata,
        Arc::new(Wikipedia::new(http())),
        db,
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
    let db = setup_db().await;
    mock_wikidata(&server, "Q200", "https://rust-lang.org", 1).await;

    let clock = FakeClock::new();
    let res1 = resolver(Some(&server), Some(db.clone()), clock.clone());
    let sub = subject("Rust", Vec::new());

    // First call with res1 writes to SQLite
    let r1 = res1.resolve(&sub, &budget()).await;
    assert_eq!(r1.origins, vec![Origin::new("rust-lang.org", None)]);
    assert_eq!(r1.resolved_by, Some(OwnerSource::Wikidata));

    // Fresh res2 with empty in-memory cache reads from SQLite
    let res2 = resolver(Some(&server), Some(db), clock);
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

#[tokio::test]
async fn owner_cache_evicts_least_recently_used_entries_past_capacity() {
    let db = setup_db().await;
    let clock = FakeClock::new();
    let res = resolver(None, Some(db.clone()), clock.clone());
    let now_ts = clock.now().timestamp();

    for i in 0..=OWNER_CACHE_CAPACITY {
        let name = format!("pkg-{i:04}");
        let sub = subject(&name, Vec::new());
        let key = cache_policy::owner_key(&sub);
        owner_cache_repository::upsert(
            &db,
            &key,
            &name,
            sub.kind.as_str(),
            r#"["https://example.org"]"#,
            "wikidata",
            now_ts,
        )
        .await
        .unwrap();
    }

    // Fill cache to capacity (0..512), then touch pkg-0000 so pkg-0001 becomes LRU.
    for i in 0..OWNER_CACHE_CAPACITY {
        let sub = subject(&format!("pkg-{i:04}"), Vec::new());
        let resolved = res.resolve(&sub, &budget()).await;
        assert_eq!(resolved.resolved_by, Some(OwnerSource::Wikidata));
    }
    assert_eq!(res.cache.lock().len(), OWNER_CACHE_CAPACITY);

    let first = subject("pkg-0000", Vec::new());
    let second = subject("pkg-0001", Vec::new());
    let overflow = subject(&format!("pkg-{OWNER_CACHE_CAPACITY:04}"), Vec::new());

    // Promote pkg-0000 in LRU order before inserting entry #513.
    res.resolve(&first, &budget()).await;
    res.resolve(&overflow, &budget()).await;

    let cache = res.cache.lock();
    assert_eq!(cache.len(), OWNER_CACHE_CAPACITY);
    assert!(cache.contains(&cache_policy::owner_key(&first)));
    assert!(!cache.contains(&cache_policy::owner_key(&second)));
    assert!(cache.contains(&cache_policy::owner_key(&overflow)));
}

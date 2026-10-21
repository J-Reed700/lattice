use super::*;
use crate::features::web::mocks::web::MockWebService;

/// A web whose every call fails with `error`, so a replay that went live
/// would be seen.
struct Failing(fn() -> AppError);

#[async_trait::async_trait]
impl WebServiceTrait for Failing {
    async fn search_web(&self, _input: &WebSearchInput) -> Result<WebSearchOutput> {
        Err((self.0)())
    }
    async fn fetch_url_content(&self, _url: &str) -> Result<FetchUrlContentOutput> {
        Err((self.0)())
    }
    async fn fetch_reference_page(&self, _url: &str) -> Result<ReferencePage> {
        Err((self.0)())
    }
    async fn fetch_url_content_as_of(
        &self,
        _url: &str,
        _as_of: Option<chrono::NaiveDate>,
    ) -> Result<FetchUrlContentOutput> {
        Err((self.0)())
    }
    fn validate_url(&self, _url: &str) -> Result<()> {
        Ok(())
    }
}

fn unreachable() -> AppError {
    AppError::InvalidUrl("unreachable in this test".into())
}

fn offline() -> AppError {
    AppError::Network("connection refused".into())
}

fn search(query: &str) -> WebSearchInput {
    serde_json::from_value(json!({ "query": query })).unwrap()
}

fn traced(inner: Arc<dyn WebServiceTrait>, mode: TraceMode, dir: &Path) -> TracedWeb {
    TracedWeb::new(inner, mode, dir).unwrap()
}

#[tokio::test]
async fn a_recorded_trace_replays_the_same_answers_without_the_web() {
    let dir = tempfile::tempdir().unwrap();
    let day = chrono::NaiveDate::from_ymd_opt(2024, 1, 2);
    let record = traced(
        Arc::new(MockWebService::new()),
        TraceMode::Replay,
        dir.path(),
    );
    let recorded = json!({
        "search": record.search_web(&search("explain analyze")).await.unwrap(),
        "fetch": record.fetch_url_content("https://example.org/a").await.unwrap(),
        "page": record.fetch_reference_page("https://example.org/b").await.unwrap(),
        "as_of": record.fetch_url_content_as_of("https://example.org/c", day).await.unwrap(),
    });
    assert_eq!(record.report().misses_filled, 4);

    let replay = traced(
        Arc::new(Failing(unreachable)),
        TraceMode::Replay,
        dir.path(),
    );
    let replayed = json!({
        "search": replay.search_web(&search("explain analyze")).await.unwrap(),
        "fetch": replay.fetch_url_content("https://example.org/a").await.unwrap(),
        "page": replay.fetch_reference_page("https://example.org/b").await.unwrap(),
        "as_of": replay.fetch_url_content_as_of("https://example.org/c", day).await.unwrap(),
    });
    assert_eq!(recorded, replayed);
    let report = replay.report();
    assert_eq!(
        (report.hits, report.misses_filled, report.misses_failed),
        (4, 0, 0)
    );
    assert_eq!((report.entries_at_start, report.entries_at_end), (4, 4));
    assert!(report.missed.is_empty());
}

#[tokio::test]
async fn a_replay_miss_is_filled_live_and_counted() {
    let dir = tempfile::tempdir().unwrap();
    let web = traced(
        Arc::new(MockWebService::new()),
        TraceMode::Replay,
        dir.path(),
    );
    web.search_web(&search("first")).await.unwrap();
    web.search_web(&search("first")).await.unwrap();
    web.search_web(&search("second")).await.unwrap();
    let report = web.report();
    assert_eq!((report.hits, report.misses_filled), (1, 2));
    assert_eq!(report.entries_at_end, 2);
    assert_eq!(report.missed.len(), 2);
    assert_eq!(report.missed[1].summary, "second");
    assert_eq!(report.by_method["search_web"].misses_filled, 2);
}

#[tokio::test]
async fn a_strict_replay_miss_is_an_invalid_input_and_counted() {
    let dir = tempfile::tempdir().unwrap();
    let web = traced(
        Arc::new(MockWebService::new()),
        TraceMode::ReplayStrict,
        dir.path(),
    );
    let error = web
        .fetch_url_content("https://example.org/new")
        .await
        .unwrap_err();
    assert!(
        matches!(&error, AppError::InvalidInput(message) if message.contains("no trace entry"))
    );
    let report = web.report();
    assert_eq!((report.misses_failed, report.entries_at_end), (1, 0));
    assert_eq!(report.missed[0].method, "fetch_url_content");
}

#[tokio::test]
async fn a_recorded_failure_replays_as_invalid_input_with_its_text() {
    let dir = tempfile::tempdir().unwrap();
    let record = traced(
        Arc::new(Failing(unreachable)),
        TraceMode::Replay,
        dir.path(),
    );
    let original = record
        .fetch_reference_page("https://example.org/gone")
        .await
        .unwrap_err();
    let replay = traced(
        Arc::new(MockWebService::new()),
        TraceMode::Replay,
        dir.path(),
    );
    let replayed = replay
        .fetch_reference_page("https://example.org/gone")
        .await
        .unwrap_err();
    assert!(matches!(&replayed, AppError::InvalidInput(text) if *text == original.to_string()));
    assert_eq!(replay.report().hits, 1);
}

#[tokio::test]
async fn a_transient_failure_is_not_recorded() {
    let dir = tempfile::tempdir().unwrap();
    let web = traced(Arc::new(Failing(offline)), TraceMode::Replay, dir.path());
    let error = web.search_web(&search("outage")).await.unwrap_err();
    assert!(matches!(error, AppError::Network(_)));
    let report = web.report();
    assert_eq!((report.transient_errors, report.entries_at_end), (1, 0));
}

#[test]
fn the_key_ignores_field_order_and_null_fields() {
    let a = json!({"query":"q","site":null,"nested":{"y":2,"x":1,"gone":null},"list":[{"b":1,"a":null}]});
    let b = json!({"list":[{"b":1}],"nested":{"x":1,"y":2},"query":"q"});
    assert_eq!(trace_key("search_web", &a), trace_key("search_web", &b));
    assert_ne!(
        trace_key("search_web", &a),
        trace_key("fetch_url_content", &a)
    );
    let typed = serde_json::to_value(search("q")).unwrap();
    let mut reordered = typed.clone();
    reordered["kind"] = Value::Null;
    assert_eq!(
        trace_key("search_web", &typed),
        trace_key("search_web", &reordered)
    );
}

#[tokio::test]
async fn both_reference_reads_share_one_entry() {
    let dir = tempfile::tempdir().unwrap();
    let web = traced(
        Arc::new(MockWebService::new()),
        TraceMode::Replay,
        dir.path(),
    );
    let content = web
        .fetch_reference_content("https://example.org/ref")
        .await
        .unwrap();
    let page = web
        .fetch_reference_page("https://example.org/ref")
        .await
        .unwrap();
    assert_eq!(
        serde_json::to_value(&content).unwrap(),
        serde_json::to_value(&page.output).unwrap()
    );
    let report = web.report();
    assert_eq!((report.misses_filled, report.hits), (1, 1));
    assert_eq!(report.entries_at_end, 1);
}

#[tokio::test]
async fn the_index_digest_changes_when_an_entry_is_added() {
    let dir = tempfile::tempdir().unwrap();
    let web = traced(
        Arc::new(MockWebService::new()),
        TraceMode::Replay,
        dir.path(),
    );
    let empty = web.index_sha256();
    web.fetch_url_content("https://example.org/one")
        .await
        .unwrap();
    let one = web.index_sha256();
    assert_ne!(empty, one);
    web.fetch_url_content("https://example.org/one")
        .await
        .unwrap();
    assert_eq!(one, web.index_sha256());
    let reopened = traced(
        Arc::new(Failing(unreachable)),
        TraceMode::Replay,
        dir.path(),
    );
    assert_eq!(one, reopened.index_sha256());
}

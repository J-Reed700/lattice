//! The adapter against a wiremock `/v1/rerank`.

use super::*;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

const KEY: &str = "test-key";

/// An address nothing listens on.
const NOWHERE: &str = "http://127.0.0.1:9";

/// Starts "servers" at `uri`: the first `dead` of them already gone and
/// listening nowhere, none at all when it `fails`. Remembers every launch.
struct FakeStarter {
    uri: String,
    fails: bool,
    dead: usize,
    launches: parking_lot::Mutex<Vec<RerankLaunch>>,
}

impl FakeStarter {
    fn new(uri: &str, fails: bool, dead: usize) -> Arc<Self> {
        Arc::new(Self {
            uri: uri.to_owned(),
            fails,
            dead,
            launches: parking_lot::Mutex::default(),
        })
    }

    fn at(uri: &str) -> Arc<Self> {
        Self::new(uri, false, 0)
    }

    fn starts(&self) -> usize {
        self.launches.lock().len()
    }
}

#[async_trait]
impl RerankServerStarter for FakeStarter {
    async fn accelerated(&self) -> bool {
        true
    }

    async fn start(&self, launch: &RerankLaunch) -> Result<Arc<RunningLlamaServer>> {
        let started = {
            let mut launches = self.launches.lock();
            launches.push(launch.clone());
            launches.len()
        };
        if self.fails {
            return Err(AppError::ModelLoadFailed("no server today".into()));
        }
        let alive = started > self.dead;
        let uri = if alive { self.uri.as_str() } else { NOWHERE };
        Ok(Arc::new(RunningLlamaServer::new(
            uri,
            KEY,
            None,
            Arc::new(move || alive),
        )))
    }
}

/// Scores each document by its position, highest last, listing the results
/// best first as llama-server does.
struct ByPosition;

/// [`ByPosition`], answering long after any test's request bound.
struct Delayed;

impl Respond for Delayed {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        ByPosition
            .respond(request)
            .set_delay(Duration::from_secs(5))
    }
}

impl Respond for ByPosition {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        let body: serde_json::Value = serde_json::from_slice(&request.body).unwrap();
        let count = body["documents"].as_array().unwrap().len();
        let results: Vec<serde_json::Value> = (0..count)
            .rev()
            .map(|index| {
                serde_json::json!({ "index": index, "relevance_score": index as f32 / count as f32 })
            })
            .collect();
        ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "model": "test",
            "object": "list",
            "usage": { "prompt_tokens": 10, "total_tokens": 10 },
            "results": results,
        }))
    }
}

async fn rerank_server() -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/rerank"))
        .and(header("authorization", format!("Bearer {KEY}").as_str()))
        .respond_with(ByPosition)
        .mount(&server)
        .await;
    server
}

fn gguf() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let gguf = dir.path().join("qwen3-reranker-0.6b-q8_0.gguf");
    std::fs::write(&gguf, b"GGUF").unwrap();
    (dir, gguf)
}

fn documents() -> Vec<String> {
    ["first", "second", "third"]
        .iter()
        .map(|text| text.to_string())
        .collect()
}

#[tokio::test]
async fn scores_come_back_by_document_index() {
    let server = rerank_server().await;
    let (_dir, gguf) = gguf();
    let starter = FakeStarter::at(&server.uri());
    let reranker = LlamaReranker::new(&gguf, starter.clone());

    assert!(reranker.is_available());
    let mut results = reranker.rerank("which", documents(), 3).await.unwrap();
    results.sort_by_key(|result| result.index);

    let scores: Vec<f32> = results.iter().map(|result| result.score).collect();
    assert_eq!(scores, [0.0, 1.0 / 3.0, 2.0 / 3.0]);
    let sent: serde_json::Value =
        serde_json::from_slice(&server.received_requests().await.unwrap()[0].body).unwrap();
    assert_eq!(sent["query"], "which");
    assert_eq!(sent["documents"][2], "third");
    let launches = starter.launches.lock();
    assert_eq!(launches.len(), 1);
    assert!(launches[0].gpu, "a machine with an accelerator asks for it");
    assert_eq!(launches[0].gguf, gguf);
}

#[tokio::test]
async fn one_server_answers_every_query_while_it_runs() {
    let server = rerank_server().await;
    let (_dir, gguf) = gguf();
    let starter = FakeStarter::at(&server.uri());
    let reranker = LlamaReranker::new(&gguf, starter.clone());

    reranker.rerank("one", documents(), 3).await.unwrap();
    reranker.rerank("two", documents(), 3).await.unwrap();

    assert_eq!(starter.starts(), 1);
}

#[tokio::test]
async fn a_reranker_that_is_not_downloaded_is_unavailable_and_starts_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let starter = FakeStarter::at(NOWHERE);
    let reranker = LlamaReranker::new(dir.path().join("missing.gguf"), starter.clone());

    assert!(!reranker.is_available());
    assert!(!reranker.is_available());
    assert_eq!(starter.starts(), 0);
}

#[tokio::test]
async fn a_server_that_will_not_start_leaves_the_reranker_unavailable_for_a_while() {
    let (_dir, gguf) = gguf();
    let starter = FakeStarter::new(NOWHERE, true, 0);
    let reranker = LlamaReranker::new(&gguf, starter.clone());

    assert!(reranker.is_available());
    assert!(reranker.rerank("which", documents(), 3).await.is_err());

    assert!(!reranker.is_available(), "no start is tried per query");
    assert_eq!(starter.starts(), 1);
}

#[tokio::test]
async fn a_server_whose_process_is_gone_is_started_again() {
    let server = rerank_server().await;
    let (_dir, gguf) = gguf();
    let starter = FakeStarter::new(&server.uri(), false, 1);
    let reranker = LlamaReranker::new(&gguf, starter.clone());

    reranker.rerank("which", documents(), 3).await.unwrap();

    assert_eq!(starter.starts(), 2);
}

/// A server too slow for the bound costs one turn its wait, not every turn.
#[tokio::test]
async fn a_server_too_slow_to_answer_rests_the_reranker() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/rerank"))
        .respond_with(Delayed)
        .mount(&server)
        .await;
    let (_dir, gguf) = gguf();
    let starter = FakeStarter::at(&server.uri());
    let mut reranker = LlamaReranker::new(&gguf, starter.clone());
    reranker.request_timeout = Duration::from_millis(50);

    assert!(reranker.is_available());
    assert!(reranker.rerank("which", documents(), 3).await.is_err());

    assert!(!reranker.is_available());
    assert_eq!(starter.starts(), 1, "a slow server is not a dead one");
    assert!(
        reranker.server.lock().await.is_none(),
        "it is let go, so the next try places it afresh"
    );
}

#[tokio::test]
async fn a_server_error_is_reported_in_its_own_words() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/rerank"))
        .respond_with(ResponseTemplate::new(500).set_body_json(serde_json::json!({
            "error": { "code": 500, "message": "input is too large to process", "type": "server_error" }
        })))
        .mount(&server)
        .await;
    let (_dir, gguf) = gguf();
    let reranker = LlamaReranker::new(&gguf, FakeStarter::at(&server.uri()));

    let error = reranker
        .rerank("which", documents(), 3)
        .await
        .unwrap_err()
        .to_string();

    assert!(error.contains("input is too large to process"), "{error}");
}

#[tokio::test]
async fn serving_from_another_gguf_starts_the_next_server_from_it() {
    let server = rerank_server().await;
    let (_dir, gguf) = gguf();
    let (_other_dir, other) = self::gguf();
    let first = FakeStarter::at(&server.uri());
    let second = FakeStarter::at(&server.uri());
    let reranker = LlamaReranker::new(&gguf, first.clone());

    reranker.rerank("which", documents(), 3).await.unwrap();
    reranker.serve_from(&other, second.clone()).await;
    reranker.rerank("which", documents(), 3).await.unwrap();

    assert_eq!(first.starts(), 1);
    assert_eq!(second.launches.lock()[0].gguf, other);
}

/// The query takes at most a quarter of the window and each passage what
/// the query and the prompt leave, at a token a character.
#[test]
fn inputs_are_cut_to_one_slot_window() {
    let query = "q".repeat(5_000);
    let long = "é".repeat(10_000);
    let (query, documents) = client::fitted(&query, vec![long, "short".into()], 4096);

    assert_eq!(query.chars().count(), 1024);
    assert_eq!(documents[0].chars().count(), 4096 - 128 - 1024);
    assert_eq!(documents[1], "short");
}

#[test]
fn a_launch_sizes_its_window_from_its_slots() {
    let launch = RerankLaunch::new(
        PathBuf::from("/models/qwen3-reranker-0.6b-q8_0.gguf"),
        false,
    );

    assert_eq!(launch.name, "qwen3-reranker-0.6b-q8_0.gguf");
    assert_eq!(launch.context_tokens(), RERANK_SLOTS * RERANK_WINDOW_TOKENS);
}

/// The bundled build's CPU path, with the flags the app launches it with.
struct OnCpu(crate::interfaces::di::modules::standalone::StandaloneServers);

#[async_trait]
impl RerankServerStarter for OnCpu {
    async fn accelerated(&self) -> bool {
        false
    }

    async fn start(&self, launch: &RerankLaunch) -> Result<Arc<RunningLlamaServer>> {
        RerankServerStarter::start(&self.0, launch).await
    }
}

/// `LATTICE_RERANKER_GGUF` names the model; run with `--ignored`.
#[tokio::test]
#[ignore = "needs the reranker GGUF and a bundled llama-server"]
async fn the_real_server_scores_the_answering_passage_highest() {
    let gguf = std::env::var("LATTICE_RERANKER_GGUF").unwrap();
    let starter = Arc::new(OnCpu(
        crate::interfaces::di::modules::standalone::StandaloneServers::from_env(),
    ));
    let reranker = LlamaReranker::new(gguf, starter);
    let passages = vec![
        "The wool market in Ghent reopened after the spring floods.".to_string(),
        "Either party may prevent renewal by giving 12 working days of written notice.".to_string(),
    ];

    assert!(reranker.is_available());
    let mut results = reranker
        .rerank("How much notice prevents renewal?", passages, 2)
        .await
        .unwrap();
    results.sort_by_key(|result| result.index);

    assert!(results[1].score > 0.5, "{results:?}");
    assert!(results[0].score < 0.5, "{results:?}");
}

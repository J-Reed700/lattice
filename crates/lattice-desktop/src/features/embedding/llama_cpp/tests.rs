//! The adapter against a wiremock `/v1/embeddings`.

use super::*;
use crate::features::embedding::prefixes::{E5, NONE, QWEN3_INSTRUCT};
use lattice_core::domain::models::downloaded::StoredDownloadedModel;
use lattice_core::domain::models::metadata::ModelType;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

const DIMENSION: usize = 4;
const KEY: &str = "test-key";

fn identity() -> ArtifactIdentity {
    ArtifactIdentity::from_digest(&[7; 32])
}

fn tokenizer() -> tokenizers::Tokenizer {
    use tokenizers::models::wordlevel::WordLevel;
    use tokenizers::pre_tokenizers::whitespace::Whitespace;
    let vocab = [("[UNK]".to_owned(), 0), ("word".to_owned(), 1)]
        .into_iter()
        .collect();
    let mut tokenizer = tokenizers::Tokenizer::new(
        WordLevel::builder()
            .vocab(vocab)
            .unk_token("[UNK]".into())
            .build()
            .unwrap(),
    );
    tokenizer.with_pre_tokenizer(Whitespace);
    tokenizer
}

fn header_for(architecture: &str, pooling_type: Option<u32>) -> EmbeddingHeader {
    EmbeddingHeader {
        architecture: Some(architecture.to_owned()),
        trained_tokens: Some(32_768),
        dimension: Some(DIMENSION as u32),
        pooling_type,
        file_type: Some(1),
    }
}

fn facts() -> EmbeddingModelFacts {
    EmbeddingModelFacts::from_header(header_for("qwen3", Some(3)), identity(), "Test embedder")
        .unwrap()
}

fn running(uri: &str, alive: bool) -> Arc<RunningLlamaServer> {
    Arc::new(RunningLlamaServer::new(
        uri,
        KEY,
        Some(1),
        Arc::new(move || alive),
    ))
}

fn service(uri: &str, prefixes: &'static EmbeddingPrefixes) -> LlamaEmbeddingService {
    LlamaEmbeddingService::connect(
        running(uri, true),
        facts(),
        InputPolicy::new(tokenizer(), 2048).unwrap(),
        prefixes,
    )
}

/// Answers every input with a vector whose first element is the input's
/// position, `width` wide; `shuffled` lists them in reverse, as the server is
/// free to.
struct Echo {
    width: usize,
    shuffled: bool,
}

impl Respond for Echo {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        let body: serde_json::Value = serde_json::from_slice(&request.body).unwrap();
        let inputs = body["input"].as_array().unwrap();
        let mut data: Vec<serde_json::Value> = (0..inputs.len())
            .map(|index| {
                let mut embedding = vec![0.0_f32; self.width];
                embedding[0] = index as f32;
                serde_json::json!({ "index": index, "object": "embedding", "embedding": embedding })
            })
            .collect();
        if self.shuffled {
            data.reverse();
        }
        ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "model": "test",
            "object": "list",
            "usage": { "prompt_tokens": inputs.len(), "total_tokens": inputs.len() },
            "data": data,
        }))
    }
}

async fn echo_server(width: usize, shuffled: bool) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/embeddings"))
        .and(header("authorization", format!("Bearer {KEY}").as_str()))
        .respond_with(Echo { width, shuffled })
        .mount(&server)
        .await;
    server
}

/// The `input` arrays the server received, request by request.
async fn sent(server: &MockServer) -> Vec<Vec<String>> {
    server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .map(|request| {
            let body: serde_json::Value = serde_json::from_slice(&request.body).unwrap();
            serde_json::from_value(body["input"].clone()).unwrap()
        })
        .collect()
}

fn texts(items: &[&str]) -> Vec<String> {
    items.iter().map(|text| (*text).to_owned()).collect()
}

#[tokio::test]
async fn a_batch_comes_back_in_input_order_whatever_order_the_server_lists() {
    let server = echo_server(DIMENSION, true).await;
    let service = service(&server.uri(), &NONE);

    let vectors = service
        .embed_batch(&texts(&["one", "two", "three"]))
        .await
        .unwrap();

    let firsts: Vec<f32> = vectors.iter().map(|v| v[0]).collect();
    assert_eq!(firsts, vec![0.0, 1.0, 2.0]);
    assert_eq!(sent(&server).await.len(), 1, "one request for the batch");
}

#[tokio::test]
async fn queries_carry_the_instruction_and_are_cached_while_documents_go_bare() {
    let server = echo_server(DIMENSION, false).await;
    let service = service(&server.uri(), &QWEN3_INSTRUCT);

    service.embed_query("capital of france").await.unwrap();
    service.embed_query("capital of france").await.unwrap();
    service
        .embed_batch(&texts(&["Paris is the capital."]))
        .await
        .unwrap();

    let sent = sent(&server).await;
    assert_eq!(sent.len(), 2, "the repeated query comes from the cache");
    assert_eq!(
        sent[0],
        vec![QWEN3_INSTRUCT.query_input("capital of france").into_owned()]
    );
    assert!(sent[0][0].starts_with("Instruct: "));
    assert_eq!(sent[1], vec!["Paris is the capital.".to_owned()]);
    assert_eq!(service.model_identity(), identity().as_str());
}

#[tokio::test]
async fn a_family_that_prefixes_both_sides_sends_both_and_names_its_space() {
    let server = echo_server(DIMENSION, false).await;
    let service = service(&server.uri(), &E5);

    service.embed_query("capital of france").await.unwrap();
    service.embed_batch(&texts(&["Paris is"])).await.unwrap();

    let sent = sent(&server).await;
    assert_eq!(sent[0], vec!["query: capital of france".to_owned()]);
    assert_eq!(sent[1], vec!["passage: Paris is".to_owned()]);
    assert_eq!(
        service.model_identity(),
        format!("{}+prefix-e5", identity().as_str())
    );
}

#[tokio::test]
async fn an_over_long_input_is_a_bug_reported_with_both_numbers() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/embeddings"))
        .respond_with(ResponseTemplate::new(400).set_body_json(serde_json::json!({
            "error": {
                "code": 400,
                "message": "input is too large",
                "type": "exceed_context_size_error",
                "n_prompt_tokens": 3002,
                "n_ctx": 2048
            }
        })))
        .expect(1)
        .mount(&server)
        .await;
    let service = service(&server.uri(), &NONE);

    let error = service.embed_batch(&texts(&["long"])).await.unwrap_err();

    let AppError::InvalidInput(message) = error else {
        panic!("expected InvalidInput, got {error:?}");
    };
    assert!(
        message.contains("3002") && message.contains("2048"),
        "{message}"
    );
}

#[tokio::test]
async fn a_server_error_is_returned_with_its_text_and_never_retried() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/embeddings"))
        .respond_with(ResponseTemplate::new(500).set_body_json(serde_json::json!({
            "error": { "code": 500, "message": "slot crashed", "type": "server_error" }
        })))
        .expect(1)
        .mount(&server)
        .await;
    let service = service(&server.uri(), &NONE);

    let error = service.embed_batch(&texts(&["text"])).await.unwrap_err();

    let AppError::EmbeddingFailed { reason } = error else {
        panic!("expected EmbeddingFailed, got {error:?}");
    };
    assert!(reason.contains("slot crashed"), "{reason}");
}

#[tokio::test]
async fn vectors_narrower_than_the_header_says_are_a_hard_error() {
    let server = echo_server(DIMENSION - 1, false).await;
    let service = service(&server.uri(), &NONE);

    let error = service.embed_batch(&texts(&["text"])).await.unwrap_err();

    let AppError::EmbeddingFailed { reason } = error else {
        panic!("expected EmbeddingFailed, got {error:?}");
    };
    assert!(
        reason.contains(&DIMENSION.to_string()) && reason.contains(&(DIMENSION - 1).to_string()),
        "{reason}"
    );
}

#[tokio::test]
async fn empty_text_is_refused_before_any_request() {
    let server = echo_server(DIMENSION, false).await;
    let service = service(&server.uri(), &NONE);

    assert!(matches!(
        service.embed_batch(&texts(&["text", ""])).await,
        Err(AppError::InvalidInput(_))
    ));
    assert!(matches!(
        service.embed_query("").await,
        Err(AppError::InvalidInput(_))
    ));
    assert!(sent(&server).await.is_empty());
}

/// A loopback URL nothing listens on: connecting is refused.
fn refusing_uri() -> String {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    format!("http://127.0.0.1:{port}")
}

/// Hands out `servers` in order, one a start, and fails once they run out when
/// `refuses` says to.
struct FakeStarter {
    servers: parking_lot::Mutex<Vec<Arc<RunningLlamaServer>>>,
    starts: AtomicUsize,
    /// What each start was wanted for.
    works: parking_lot::Mutex<Vec<EmbeddingWork>>,
    accelerated: bool,
    refuses: bool,
}

impl FakeStarter {
    fn new(servers: Vec<Arc<RunningLlamaServer>>) -> Arc<Self> {
        Arc::new(Self::with(servers, false, false))
    }

    /// The same on a machine with a GPU.
    fn with_gpu(servers: Vec<Arc<RunningLlamaServer>>) -> Arc<Self> {
        Arc::new(Self::with(servers, true, false))
    }

    fn with(servers: Vec<Arc<RunningLlamaServer>>, accelerated: bool, refuses: bool) -> Self {
        Self {
            servers: parking_lot::Mutex::new(servers),
            starts: AtomicUsize::new(0),
            works: parking_lot::Mutex::default(),
            accelerated,
            refuses,
        }
    }
}

#[async_trait]
impl EmbeddingServerStarter for FakeStarter {
    async fn header(&self, _: &Path) -> Option<EmbeddingHeader> {
        Some(header_for("qwen3", Some(3)))
    }
    async fn accelerated(&self) -> bool {
        self.accelerated
    }
    async fn start(&self, launch: &EmbeddingLaunch) -> Result<Arc<RunningLlamaServer>> {
        self.starts.fetch_add(1, Ordering::SeqCst);
        self.works.lock().push(launch.work);
        let mut servers = self.servers.lock();
        if servers.is_empty() && self.refuses {
            return Err(AppError::ModelLoadFailed("refused".into()));
        }
        assert!(!servers.is_empty(), "more starts than servers");
        Ok(servers.remove(0))
    }
}

/// A live server holding nothing of the GPU: on the integrated one or the CPU.
fn off_the_gpu(uri: &str) -> Arc<RunningLlamaServer> {
    Arc::new(RunningLlamaServer::new(uri, KEY, None, Arc::new(|| true)))
}

/// A batch big enough to count as indexing.
fn full_batch() -> Vec<String> {
    (0..INDEXING_PASSAGES)
        .map(|passage| format!("passage {passage}"))
        .collect()
}

/// A local GGUF row with a `tokenizer.json` beside it.
fn local_model(dir: &Path) -> DownloadedModel {
    let gguf = dir.join("Qwen3-Embedding-0.6B-f16.gguf");
    std::fs::write(&gguf, b"GGUF").unwrap();
    tokenizer().save(dir.join("tokenizer.json"), false).unwrap();
    DownloadedModel::from_db(StoredDownloadedModel {
        id: "row-1".into(),
        model_name: "Qwen3 Embedding 0.6B".into(),
        model_id: "qwen3-embedding-0.6b".into(),
        location: ModelLocation::LocalFile { path: gguf },
        file_size_bytes: 4,
        model_type: ModelType::TextEmbeddings,
        architecture: "qwen3".into(),
        downloaded_at: chrono::Utc::now(),
        last_used_at: None,
        use_count: 0,
        is_active_for_chat: false,
        is_active_for_embedding: true,
        metadata: None,
        is_active_for_utility: false,
        embedding_artifact_identity: Some(identity()),
    })
}

#[tokio::test]
async fn a_dead_server_is_started_again_once_and_the_batch_sent_again() {
    let dir = tempfile::tempdir().unwrap();
    let live = echo_server(DIMENSION, false).await;
    let starter = FakeStarter::new(vec![
        running(&refusing_uri(), false),
        running(&live.uri(), true),
    ]);
    let service =
        LlamaEmbeddingService::start(starter.clone(), &local_model(dir.path()), identity())
            .await
            .unwrap();

    let vectors = service.embed_batch(&texts(&["one", "two"])).await.unwrap();

    assert_eq!(vectors.len(), 2);
    assert_eq!(
        starter.starts.load(Ordering::SeqCst),
        2,
        "one start, one restart"
    );
    assert_eq!(sent(&live).await.len(), 1);
    assert_eq!(service.model_identity(), identity().as_str());
}

#[tokio::test]
async fn a_server_that_dies_again_after_its_restart_is_reported_not_retried() {
    let dir = tempfile::tempdir().unwrap();
    let starter = FakeStarter::new(vec![
        running(&refusing_uri(), false),
        running(&refusing_uri(), false),
    ]);
    let service =
        LlamaEmbeddingService::start(starter.clone(), &local_model(dir.path()), identity())
            .await
            .unwrap();

    let error = service.embed_batch(&texts(&["one"])).await.unwrap_err();

    let AppError::EmbeddingFailed { reason } = error else {
        panic!("expected EmbeddingFailed, got {error:?}");
    };
    assert!(reason.contains("started again"), "{reason}");
    assert_eq!(starter.starts.load(Ordering::SeqCst), 2, "no loop");
}

#[tokio::test]
async fn a_refused_connection_to_a_live_process_is_returned_without_a_restart() {
    let dir = tempfile::tempdir().unwrap();
    let starter = FakeStarter::new(vec![running(&refusing_uri(), true)]);
    let service =
        LlamaEmbeddingService::start(starter.clone(), &local_model(dir.path()), identity())
            .await
            .unwrap();

    assert!(matches!(
        service.embed_batch(&texts(&["one"])).await,
        Err(AppError::EmbeddingFailed { .. })
    ));
    assert_eq!(starter.starts.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn a_full_batch_brings_a_server_kept_off_the_gpu_onto_it() {
    let dir = tempfile::tempdir().unwrap();
    let (off, on) = (
        echo_server(DIMENSION, false).await,
        echo_server(DIMENSION, false).await,
    );
    let starter = FakeStarter::with_gpu(vec![off_the_gpu(&off.uri()), running(&on.uri(), true)]);
    let service =
        LlamaEmbeddingService::start(starter.clone(), &local_model(dir.path()), identity())
            .await
            .unwrap();

    service.embed_batch(&full_batch()).await.unwrap();
    service.embed_batch(&full_batch()).await.unwrap();

    assert_eq!(
        *starter.works.lock(),
        [EmbeddingWork::Light, EmbeddingWork::Indexing],
        "moved once, by the first batch"
    );
    assert!(sent(&off).await.is_empty());
    assert_eq!(sent(&on).await.len(), 2);
}

/// A search or a few passages are not worth a chat model's reload.
#[tokio::test]
async fn a_query_and_a_small_batch_stay_where_the_server_is() {
    let dir = tempfile::tempdir().unwrap();
    let server = echo_server(DIMENSION, false).await;
    let starter = FakeStarter::with_gpu(vec![off_the_gpu(&server.uri())]);
    let service =
        LlamaEmbeddingService::start(starter.clone(), &local_model(dir.path()), identity())
            .await
            .unwrap();

    service.embed_query("query").await.unwrap();
    service.embed_single("passage").await.unwrap();
    let mut small = full_batch();
    small.pop();
    service.embed_batch(&small).await.unwrap();

    assert_eq!(starter.starts.load(Ordering::SeqCst), 1);
    assert_eq!(sent(&server).await.len(), 3);
}

/// Off the GPU is where a machine with none runs by design.
#[tokio::test]
async fn a_machine_without_a_gpu_indexes_where_it_is() {
    let dir = tempfile::tempdir().unwrap();
    let server = echo_server(DIMENSION, false).await;
    let starter = FakeStarter::new(vec![off_the_gpu(&server.uri())]);
    let service =
        LlamaEmbeddingService::start(starter.clone(), &local_model(dir.path()), identity())
            .await
            .unwrap();

    service.embed_batch(&full_batch()).await.unwrap();

    assert_eq!(starter.starts.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn a_server_that_cannot_move_keeps_indexing_where_it_is() {
    let dir = tempfile::tempdir().unwrap();
    let server = echo_server(DIMENSION, false).await;
    let starter = Arc::new(FakeStarter::with(
        vec![off_the_gpu(&server.uri())],
        true,
        true,
    ));
    let service =
        LlamaEmbeddingService::start(starter.clone(), &local_model(dir.path()), identity())
            .await
            .unwrap();

    let vectors = service.embed_batch(&full_batch()).await.unwrap();

    assert_eq!(vectors.len(), INDEXING_PASSAGES);
    assert_eq!(starter.starts.load(Ordering::SeqCst), 2);
    assert_eq!(sent(&server).await.len(), 1);
}

/// What brings a dead server back decides what it may take for the GPU: a chat
/// model unloaded for one query would reload for the next message.
#[tokio::test]
async fn a_dead_server_comes_back_for_the_work_that_found_it() {
    let dir = tempfile::tempdir().unwrap();
    let live = echo_server(DIMENSION, false).await;
    let starter = FakeStarter::with_gpu(vec![
        running(&refusing_uri(), false),
        running(&refusing_uri(), false),
        running(&live.uri(), true),
    ]);
    let service =
        LlamaEmbeddingService::start(starter.clone(), &local_model(dir.path()), identity())
            .await
            .unwrap();

    assert!(service.embed_query("query").await.is_err());
    service.embed_batch(&full_batch()).await.unwrap();

    assert_eq!(
        *starter.works.lock(),
        [
            EmbeddingWork::Light,
            EmbeddingWork::Light,
            EmbeddingWork::Indexing
        ]
    );
}

#[test]
fn a_header_without_pooled_output_is_refused_with_the_reason() {
    for pooling_type in [None, Some(0), Some(4)] {
        let error = EmbeddingModelFacts::from_header(
            header_for("qwen3", pooling_type),
            identity(),
            "Test embedder",
        )
        .unwrap_err();
        let AppError::ModelLoadFailed(message) = error else {
            panic!("expected ModelLoadFailed, got {error:?}");
        };
        assert!(message.contains("pool"), "{message}");
    }
    assert_eq!(facts().pooling, Pooling::Last);
}

#[test]
fn the_window_is_the_trained_one_up_to_the_longest_passage() {
    assert_eq!(facts().window_tokens(), 2048);
    let small = EmbeddingModelFacts::from_header(
        EmbeddingHeader {
            trained_tokens: Some(512),
            ..header_for("bert", Some(2))
        },
        identity(),
        "MiniLM",
    )
    .unwrap();
    assert_eq!(small.window_tokens(), 512);
    let launch = EmbeddingLaunch::new("/m.gguf".into(), &facts(), true);
    assert_eq!(launch.context_tokens(), SERVER_SLOTS * 2048);
}

/// The one function search keys its index with before the model loads, and
/// the loaded model reports: the file hash, plus the document prefix when
/// the family has one.
#[test]
fn the_space_identity_is_the_file_hash_plus_any_document_prefix() {
    let qwen = resolve_prefixes("qwen3-embedding-0.6b", "qwen3");
    assert_eq!(qwen, &QWEN3_INSTRUCT);
    assert_eq!(space_identity(&identity(), qwen), identity().as_str());
    assert_eq!(
        resolve_prefixes("Qwen3-Embedding-0.6B-f16.gguf", "qwen3"),
        &QWEN3_INSTRUCT
    );
    assert_eq!(
        resolve_prefixes("my-own-embedder", "qwen3"),
        &QWEN3_INSTRUCT
    );
    assert_eq!(resolve_prefixes("my-own-embedder", "bert"), &NONE);
    let e5 = resolve_prefixes("intfloat/e5-base-v2", "bert");
    assert_eq!(
        space_identity(&identity(), e5),
        format!("{}+prefix-e5", identity().as_str())
    );
}

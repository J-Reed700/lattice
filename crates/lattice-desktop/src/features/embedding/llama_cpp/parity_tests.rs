//! The parity contract, live: llama-server's vectors of the reference set
//! against the vectors Candle wrote for it before Candle was removed. Ignored
//! by default; run on every sidecar bump and every curated GGUF pin change.
//!
//! ```text
//! LATTICE_EMBED_GGUF=/path/to/Qwen3-Embedding-0.6B-f16.gguf \
//!   cargo test --lib embedding_parity -- --ignored --nocapture --test-threads=1
//! ```
//!
//! `tokenizer.json` must sit beside the GGUF. `LATTICE_LLAMA_SERVER` names
//! another binary than the bundled one; `LATTICE_EMBED_FLASH_ATTN=off`
//! compares flash attention off against the default `auto`.

use super::*;
use crate::features::llm::engine::gguf_metadata::read_model_info;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

/// The contract (design §5.3): every pair at least this similar.
const MIN_COSINE: f32 = 0.9999;
/// Batches as the passage index sends them.
const BATCH: usize = 16;
const REFERENCE_FILES: [&str; 2] = ["candle-metal-f16.f32", "candle-cpu-f32.f32"];

fn env_path(name: &str) -> PathBuf {
    PathBuf::from(std::env::var(name).unwrap_or_else(|_| panic!("set {name}")))
}

/// The bundled build for this platform, as `binaries/` holds it.
fn bundled_binary() -> PathBuf {
    let triple = if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        "aarch64-apple-darwin"
    } else if cfg!(target_os = "macos") {
        "x86_64-apple-darwin"
    } else if cfg!(target_os = "windows") {
        "x86_64-pc-windows-msvc.exe"
    } else {
        "x86_64-unknown-linux-gnu"
    };
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("binaries")
        .join(format!("llama-server-{triple}"))
}

/// A llama-server this test spawned with the embedding flags of design §5.1,
/// killed on drop.
pub(super) struct LiveServer {
    child: Arc<parking_lot::Mutex<Child>>,
    pub(super) server: Arc<RunningEmbeddingServer>,
}

impl LiveServer {
    /// Serve `gguf` with `slots` slots of `window` tokens each, every layer
    /// offloaded, once `/health` answers.
    pub(super) async fn start(gguf: &Path, slots: u32, window: u32) -> Self {
        let binary = std::env::var_os("LATTICE_LLAMA_SERVER")
            .map(PathBuf::from)
            .unwrap_or_else(bundled_binary);
        let flash_attn =
            std::env::var("LATTICE_EMBED_FLASH_ATTN").unwrap_or_else(|_| "auto".to_owned());
        let port = {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            listener.local_addr().unwrap().port()
        };
        let key = "parity-test-key";
        let child = Command::new(&binary)
            .args(["-m", &gguf.to_string_lossy(), "--embedding"])
            .args(["-c", &(slots * window).to_string()])
            .args(["-np", &slots.to_string()])
            .args(["-b", &window.to_string(), "-ub", &window.to_string()])
            .args(["-ngl", "99", "-fa", &flash_attn])
            .args([
                "--host",
                "127.0.0.1",
                "--port",
                &port.to_string(),
                "--no-webui",
            ])
            .env("LLAMA_API_KEY", key)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap_or_else(|e| panic!("{} did not spawn: {e}", binary.display()));
        let child = Arc::new(parking_lot::Mutex::new(child));
        let endpoint = format!("http://127.0.0.1:{port}");
        let http = reqwest::Client::new();
        loop {
            if let Some(status) = child.lock().try_wait().unwrap() {
                panic!("llama-server exited before it was ready: {status}");
            }
            let health = http
                .get(format!("{endpoint}/health"))
                .bearer_auth(key)
                .send()
                .await;
            if health.is_ok_and(|response| response.status().is_success()) {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(250)).await;
        }
        let watched = Arc::clone(&child);
        let server = Arc::new(RunningEmbeddingServer::new(
            endpoint,
            key,
            None,
            Arc::new(move || matches!(watched.lock().try_wait(), Ok(None))),
        ));
        Self { child, server }
    }
}

impl Drop for LiveServer {
    fn drop(&mut self) {
        let mut child = self.child.lock();
        let _ = child.kill();
        let _ = child.wait();
    }
}

/// The facts of the model at `gguf`, from its header.
pub(super) async fn live_facts(gguf: &Path) -> (EmbeddingModelFacts, InputPolicy) {
    let info = read_model_info(gguf).expect("a readable GGUF header");
    let header = EmbeddingHeader {
        architecture: info.architecture,
        trained_tokens: info.trained_context_length,
        dimension: info.embedding_length,
        pooling_type: info.pooling_type,
        file_type: info.file_type,
    };
    let identity = crate::infrastructure::ml::artifact_identity::compute(gguf).unwrap();
    let facts = EmbeddingModelFacts::from_header(header, identity, "parity").unwrap();
    let policy = tokenizer_policy(gguf, &facts).await.unwrap();
    (facts, policy)
}

#[derive(serde::Deserialize)]
struct Passage {
    text: String,
    tokens: usize,
}

fn rows(path: &Path, width: usize) -> Vec<Vec<f32>> {
    let bytes = std::fs::read(path).unwrap();
    assert_eq!(
        bytes.len() % (width * 4),
        0,
        "{} is not whole rows",
        path.display()
    );
    bytes
        .chunks_exact(width * 4)
        .map(|row| {
            row.chunks_exact(4)
                .map(|f| f32::from_le_bytes([f[0], f[1], f[2], f[3]]))
                .collect()
        })
        .collect()
}

fn cosine(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
    let norm = |v: &[f32]| v.iter().map(|x| x * x).sum::<f32>().sqrt();
    dot / (norm(a) * norm(b))
}

/// The passage each query ranks first.
fn top_ones(queries: &[Vec<f32>], passages: &[Vec<f32>]) -> Vec<usize> {
    queries
        .iter()
        .map(|query| {
            passages
                .iter()
                .enumerate()
                .map(|(i, passage)| (i, cosine(query, passage)))
                .max_by(|a, b| a.1.total_cmp(&b.1))
                .map(|(i, _)| i)
                .unwrap()
        })
        .collect()
}

#[tokio::test]
#[ignore = "requires LATTICE_EMBED_GGUF and a runnable llama-server"]
async fn embedding_parity_with_the_candle_reference() {
    let gguf = env_path("LATTICE_EMBED_GGUF");
    let reference = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../evals/embedding/reference/qwen3-embedding-0.6b");
    let passages: Vec<Passage> =
        serde_json::from_slice(&std::fs::read(reference.join("passages.json")).unwrap()).unwrap();
    let queries: Vec<String> =
        serde_json::from_slice(&std::fs::read(reference.join("queries.json")).unwrap()).unwrap();

    let (facts, policy) = live_facts(&gguf).await;
    let live = LiveServer::start(&gguf, SERVER_SLOTS, facts.window_tokens()).await;
    let prefixes = resolve_prefixes("qwen3-embedding-0.6b", &facts.architecture);
    let service = LlamaEmbeddingService::connect(Arc::clone(&live.server), facts, policy, prefixes);

    // The server counts the tokens the chunker sized each passage with.
    for (i, passage) in passages.iter().enumerate() {
        let input = prefixes.document_input(&passage.text).into_owned();
        let embedded = client::embed(&service.http, &live.server, &[input])
            .await
            .unwrap();
        assert_eq!(
            embedded.prompt_tokens as usize, passage.tokens,
            "passage {i}: server and chunker disagree about its length"
        );
    }

    let texts: Vec<String> = passages.iter().map(|p| p.text.clone()).collect();
    let mut vectors = Vec::with_capacity(texts.len() + queries.len());
    for batch in texts.chunks(BATCH) {
        vectors.extend(service.embed_batch(batch).await.unwrap());
    }
    for query in &queries {
        vectors.push(service.embed_query(query).await.unwrap());
    }

    let mut compared = 0;
    for file in REFERENCE_FILES {
        let path = reference.join(file);
        if !path.exists() {
            println!("{file}: absent, skipped");
            continue;
        }
        let expected = rows(&path, service.dimension());
        assert_eq!(expected.len(), vectors.len(), "{file}: rows");
        let cosines: Vec<f32> = vectors
            .iter()
            .zip(&expected)
            .map(|(a, b)| cosine(a, b))
            .collect();
        let (worst, min) = cosines
            .iter()
            .copied()
            .enumerate()
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .unwrap();
        let split = passages.len();
        let ours = top_ones(&vectors[split..], &vectors[..split]);
        let theirs = top_ones(&expected[split..], &expected[..split]);
        let agree = ours.iter().zip(&theirs).filter(|(a, b)| a == b).count();
        println!(
            "{file}: cosine min {min:.6} (row {worst}), top-1 agreement {agree}/{}",
            queries.len()
        );
        assert!(min >= MIN_COSINE, "{file}: row {worst} has cosine {min}");
        assert_eq!(agree, queries.len(), "{file}: top-1 disagrees");
        compared += 1;
    }
    assert!(
        compared > 0,
        "no reference vectors under {}",
        reference.display()
    );
}

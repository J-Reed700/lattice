//! Embedding throughput against real weights and real passages, ignored by
//! default: how many passages a second an index embeds, and where the time
//! goes. One run measures, per set of passages:
//!
//! - their length in tokens, as embedded (the document prefix included);
//! - the model alone, handed the passage index's batches (`BATCH_SIZE` per
//!   call): passages, real tokens and padded tokens a second, and forward
//!   passes per call;
//! - the model alone, handed the whole set in one call, which its own
//!   planner cuts into forward passes by length;
//! - the passage index's embedding loop end to end (store reads, vectors,
//!   saves), at the same batches as the model-alone run, so the difference
//!   is the loop's own cost;
//! - tokenising every passage once, which each batch does three times.
//!
//! ```text
//! LATTICE_QWEN3_EMBEDDING_DIR=/path/to/Qwen3-Embedding-0.6B \
//! LATTICE_EMBED_BENCH_PASSAGES=/path/to/passages.json \
//!   cargo test --lib embedding_throughput -- --ignored --nocapture --test-threads=1
//! ```
//!
//! `passages.json` is an object of named sets, each an array of passage
//! texts as an index stores them.

use super::config::weights_dtype;
use super::*;
use crate::infrastructure::passage_index::build::{self, Embedding, Progress, BATCH_SIZE};
use crate::infrastructure::passage_index::chunking::NewPassage;
use crate::infrastructure::passage_index::{PassageStore, Vectors};
use lattice_core::application::ports::EmbeddingPort;
use std::collections::BTreeMap;
use std::time::Instant;
use tokio_util::sync::CancellationToken;

const MODEL_DIR_ENV: &str = "LATTICE_QWEN3_EMBEDDING_DIR";
const PASSAGES_ENV: &str = "LATTICE_EMBED_BENCH_PASSAGES";

fn env_path(name: &str) -> PathBuf {
    PathBuf::from(std::env::var(name).unwrap_or_else(|_| panic!("set {name}")))
}

fn percentile(sorted: &[usize], share: f64) -> usize {
    let index = ((sorted.len() as f64 - 1.0) * share).round() as usize;
    sorted[index.min(sorted.len() - 1)]
}

/// Forward passes and padded tokens the model's planner makes of `texts`.
fn planned(service: &CandleEmbeddingService, texts: &[String], tokens: &[usize]) -> (usize, usize) {
    let plan = service.plan_batches(texts).unwrap();
    let padded = plan
        .iter()
        .map(|batch| batch.len() * batch.iter().map(|&i| tokens[i]).max().unwrap_or(0))
        .sum();
    (plan.len(), padded)
}

struct Quiet;

impl Progress for Quiet {
    fn embedded(&mut self, _count: usize) {}
    fn saved(&mut self, _documents: usize) {}
}

async fn model_alone(
    service: &CandleEmbeddingService,
    texts: &[String],
    tokens: &[usize],
    per_call: usize,
    label: &str,
) {
    let documents = service.as_documents(texts);
    let (mut forwards, mut padded) = (0, 0);
    for (chunk, counts) in documents.chunks(per_call).zip(tokens.chunks(per_call)) {
        let (f, p) = planned(service, chunk, counts);
        forwards += f;
        padded += p;
    }
    let started = Instant::now();
    for chunk in texts.chunks(per_call) {
        EmbeddingPort::embed_batch(service, chunk).await.unwrap();
    }
    let seconds = started.elapsed().as_secs_f64();
    let real: usize = tokens.iter().sum();
    println!(
        "  {label:<26} {:>7.2} passages/s {:>8.0} tokens/s {:>8.0} padded tokens/s \
         {forwards:>4} forwards ({:.2} per call) padding {:.0}%",
        texts.len() as f64 / seconds,
        real as f64 / seconds,
        padded as f64 / seconds,
        forwards as f64 / texts.len().div_ceil(per_call) as f64,
        100.0 * (padded as f64 - real as f64) / padded as f64,
    );
}

async fn index_loop(service: Arc<CandleEmbeddingService>, texts: &[String]) {
    let temp = tempfile::tempdir().unwrap();
    let store = PassageStore::open(temp.path()).await.unwrap();
    // A document of a few passages each, as a file's or a page's would be.
    for (number, group) in texts.chunks(4).enumerate() {
        let passages: Vec<NewPassage> = group
            .iter()
            .enumerate()
            .map(|(i, text)| NewPassage {
                start: i,
                end: i + 1,
                text: text.clone(),
            })
            .collect();
        store
            .replace(&format!("doc-{number:05}"), "1", "h", &passages)
            .await
            .unwrap();
    }
    let embedder: Arc<dyn EmbeddingPort> = service;
    let vectors = Vectors::open(&store, &embedder.model_identity(), embedder.dimension())
        .await
        .unwrap();
    let embedding = Embedding {
        embedder,
        batch_size: BATCH_SIZE,
        save_every: build::SAVE_EVERY,
        embed_text: Arc::new(|passage| passage.text.clone()),
        turns: Arc::default(),
    };
    let round = build::embed_pending(
        &store,
        &vectors,
        &embedding,
        "",
        &CancellationToken::new(),
        &mut Quiet,
    )
    .await
    .unwrap();
    assert_eq!(round.passages, texts.len());
    println!(
        "  {:<26} {:>7.2} passages/s",
        "index loop, end to end",
        texts.len() as f64 / round.elapsed.as_secs_f64()
    );
    store.close().await;
}

#[tokio::test]
#[ignore = "requires LATTICE_QWEN3_EMBEDDING_DIR and LATTICE_EMBED_BENCH_PASSAGES"]
async fn embedding_throughput_on_real_passages() {
    let sets: BTreeMap<String, Vec<String>> =
        serde_json::from_slice(&std::fs::read(env_path(PASSAGES_ENV)).unwrap()).unwrap();
    let service = Arc::new(
        CandleEmbeddingService::open(
            env_path(MODEL_DIR_ENV),
            ArtifactIdentity::from_digest(&[0; 32]),
        )
        .unwrap(),
    );
    println!(
        "device {:?}, dtype {:?}, window {} tokens, padded-token budget {} per forward, \
         at most {MAX_BATCH_ROWS} rows; the index hands {BATCH_SIZE} passages per call",
        service.device.location(),
        weights_dtype(service.architecture, &service.device),
        service.input_policy.max_tokens,
        service.padded_token_budget(),
    );
    // Loading and the first Metal pipelines are not anyone's throughput.
    let warm: Vec<String> = sets.values().flatten().take(BATCH_SIZE).cloned().collect();
    EmbeddingPort::embed_batch(&*service, &warm).await.unwrap();

    for (name, texts) in &sets {
        let documents = service.as_documents(texts);
        let started = Instant::now();
        let tokens: Vec<usize> = documents
            .iter()
            .map(|text| service.input_policy.count(text).unwrap())
            .collect();
        let tokenising = started.elapsed().as_secs_f64() / texts.len() as f64;
        let mut sorted = tokens.clone();
        sorted.sort_unstable();
        let chars: usize = texts.iter().map(|text| text.chars().count()).sum();
        println!(
            "\n{name}: {} passages, {:.0} chars on average; tokens mean {:.0}, p10 {}, p50 {}, \
             p90 {}, max {}; tokenising one passage {:.2} ms",
            texts.len(),
            chars as f64 / texts.len() as f64,
            tokens.iter().sum::<usize>() as f64 / texts.len() as f64,
            percentile(&sorted, 0.1),
            percentile(&sorted, 0.5),
            percentile(&sorted, 0.9),
            sorted.last().copied().unwrap_or(0),
            tokenising * 1000.0,
        );
        model_alone(&service, texts, &tokens, BATCH_SIZE, "model, index batches").await;
        model_alone(
            &service,
            texts,
            &tokens,
            texts.len(),
            "model, whole set per call",
        )
        .await;
        index_loop(Arc::clone(&service), texts).await;
    }

    // The same index batches in full precision, on a slice of each set: what
    // half precision buys on this device.
    let full = CandleEmbeddingService::open_as(
        env_path(MODEL_DIR_ENV),
        ArtifactIdentity::from_digest(&[0; 32]),
        candle_core::DType::F32,
    )
    .unwrap();
    EmbeddingPort::embed_batch(&full, &warm).await.unwrap();
    for (name, texts) in &sets {
        let slice = &texts[..texts.len().min(BATCH_SIZE * 4)];
        let tokens: Vec<usize> = service
            .as_documents(slice)
            .iter()
            .map(|text| service.input_policy.count(text).unwrap())
            .collect();
        println!("\n{name}, first {} passages:", slice.len());
        model_alone(&service, slice, &tokens, BATCH_SIZE, "half precision").await;
        model_alone(&full, slice, &tokens, BATCH_SIZE, "full precision").await;
    }
}

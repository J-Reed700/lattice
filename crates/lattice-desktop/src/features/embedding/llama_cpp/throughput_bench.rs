//! Embedding throughput on the bundled llama-server against real passages,
//! ignored by default. For each set of passages, at each slot count:
//!
//! - the model alone, handed the passage index's batches (`BATCH_SIZE` per
//!   call), and handed the whole set in one call;
//! - the passage index's embedding loop end to end (store reads, vectors,
//!   saves), at the same batches, so the difference is the loop's own cost.
//!
//! The slot sweep (`-np` 1, 2, 4) records what the default of four costs in
//! memory and buys in pace.
//!
//! ```text
//! LATTICE_EMBED_GGUF=/path/to/Qwen3-Embedding-0.6B-f16.gguf \
//! LATTICE_EMBED_BENCH_PASSAGES=/path/to/passages.json \
//!   cargo test --lib embedding_throughput -- --ignored --nocapture --test-threads=1
//! ```
//!
//! `passages.json` is an object of named sets, each an array of passage
//! texts as an index stores them. `tokenizer.json` must sit beside the GGUF.

use super::parity_tests::{live_facts, LiveServer};
use super::*;
use crate::infrastructure::passage_index::build::{self, Embedding, Progress, BATCH_SIZE};
use crate::infrastructure::passage_index::chunking::NewPassage;
use crate::infrastructure::passage_index::{PassageStore, Vectors};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Instant;
use tokio_util::sync::CancellationToken;

const GGUF_ENV: &str = "LATTICE_EMBED_GGUF";
const PASSAGES_ENV: &str = "LATTICE_EMBED_BENCH_PASSAGES";
const SLOT_SWEEP: [u32; 3] = [1, 2, 4];

fn env_path(name: &str) -> PathBuf {
    PathBuf::from(std::env::var(name).unwrap_or_else(|_| panic!("set {name}")))
}

struct Quiet;

impl Progress for Quiet {
    fn embedded(&mut self, _count: usize) {}
    fn saved(&mut self, _documents: usize) {}
}

/// One line in the format of the benchmark logs: passages/s, tokens/s,
/// tokens, seconds.
fn report(label: &str, passages: usize, tokens: usize, seconds: f64) {
    println!(
        "  {label:<26} {:>7.2} passages/s {:>8.0} tok/s {tokens:>8} tokens {seconds:>8.2} s",
        passages as f64 / seconds,
        tokens as f64 / seconds,
    );
}

async fn model_alone(
    service: &LlamaEmbeddingService,
    texts: &[String],
    tokens: usize,
    per_call: usize,
    label: &str,
) {
    let started = Instant::now();
    for chunk in texts.chunks(per_call) {
        EmbeddingPort::embed_batch(service, chunk).await.unwrap();
    }
    report(label, texts.len(), tokens, started.elapsed().as_secs_f64());
}

async fn index_loop(service: Arc<LlamaEmbeddingService>, texts: &[String], tokens: usize) {
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
    report(
        "index loop, end to end",
        texts.len(),
        tokens,
        round.elapsed.as_secs_f64(),
    );
    store.close().await;
}

#[tokio::test]
#[ignore = "requires LATTICE_EMBED_GGUF and LATTICE_EMBED_BENCH_PASSAGES"]
async fn embedding_throughput_on_the_llama_server() {
    let gguf = env_path(GGUF_ENV);
    let sets: BTreeMap<String, Vec<String>> =
        serde_json::from_slice(&std::fs::read(env_path(PASSAGES_ENV)).unwrap()).unwrap();

    for slots in SLOT_SWEEP {
        let (facts, policy) = live_facts(&gguf).await;
        let window = facts.window_tokens();
        let live = LiveServer::start(&gguf, slots, window).await;
        let prefixes = resolve_prefixes("qwen3-embedding-0.6b", &facts.architecture);
        let service = Arc::new(LlamaEmbeddingService::connect(
            Arc::clone(&live.server),
            facts,
            policy,
            prefixes,
        ));
        println!(
            "\n-np {slots}, window {window} tokens a slot, {BATCH_SIZE} passages per index call"
        );
        for (name, texts) in &sets {
            let tokens: usize = texts
                .iter()
                .map(|text| {
                    service
                        .policy
                        .count(&prefixes.document_input(text))
                        .unwrap()
                })
                .sum();
            println!("{name}: {} passages", texts.len());
            model_alone(&service, texts, tokens, BATCH_SIZE, "model, index batches").await;
            model_alone(
                &service,
                texts,
                tokens,
                texts.len().max(1),
                "model, whole set",
            )
            .await;
            index_loop(Arc::clone(&service), texts, tokens).await;
        }
    }
}

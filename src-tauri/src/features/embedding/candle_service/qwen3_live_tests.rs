//! Live checks against real Qwen3-Embedding weights, ignored by default.

use super::*;
use candle_core::DType;
use lattice_core::application::ports::EmbeddingPort;
use std::path::Path;
use std::time::Instant;

const MODEL_DIR_ENV: &str = "LATTICE_QWEN3_EMBEDDING_DIR";

fn model_dir() -> PathBuf {
    PathBuf::from(
        std::env::var(MODEL_DIR_ENV)
            .unwrap_or_else(|_| panic!("set {MODEL_DIR_ENV} to the Qwen3 weights directory")),
    )
}

fn open(dtype: DType) -> CandleEmbeddingService {
    CandleEmbeddingService::open_as(model_dir(), ArtifactIdentity::from_digest(&[0; 32]), dtype)
        .expect("open Qwen3 weights")
}

/// Twelve passages spanning three orders of magnitude in length, which is
/// what makes the batching interesting: the short ones share a wide batch
/// and pick up hundreds of pad tokens they must be unaffected by.
fn sample_texts() -> Vec<String> {
    let sentence = "Vector search indexes an embedding per chunk and ranks by cosine \
                        similarity against the query vector. ";
    let paragraph = "Retrieval augmented generation splits a corpus into passages, \
                         embeds each one, and retrieves the nearest neighbours of the \
                         question before the model writes a word. ";
    vec![
        "ok".to_owned(),
        "What is Matryoshka representation learning?".to_owned(),
        "Le chat dort sur le canapé pendant que la pluie tombe.".to_owned(),
        "分词器把文本切成子词单元。".to_owned(),
        sentence.to_owned(),
        sentence.repeat(3),
        paragraph.to_owned(),
        paragraph.repeat(4),
        paragraph.repeat(12),
        sentence.repeat(40),
        paragraph.repeat(30),
        format!("{paragraph}{}", sentence.repeat(60)),
    ]
}

/// Queries from the v2 retrieval fixture, if it is where it usually is.
fn fixture_queries() -> Vec<String> {
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../evals/retrieval/synthetic-library-v2.json");
    let Ok(bytes) = std::fs::read(&path) else {
        return Vec::new();
    };
    let Ok(fixture) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
        return Vec::new();
    };
    fixture
        .get("queries")
        .and_then(|queries| queries.as_array())
        .map(|queries| {
            queries
                .iter()
                .filter_map(|query| {
                    query
                        .get("text")
                        .or_else(|| query.get("query"))
                        .and_then(|text| text.as_str())
                        .map(str::to_owned)
                })
                .collect()
        })
        .unwrap_or_default()
}

fn cosine(left: &[f32], right: &[f32]) -> f32 {
    assert_eq!(left.len(), right.len());
    left.iter().zip(right).map(|(a, b)| a * b).sum()
}

fn assert_finite(vectors: &[Vec<f32>], label: &str) {
    for (row, vector) in vectors.iter().enumerate() {
        assert!(
            vector.iter().all(|value| value.is_finite()),
            "{label} row {row} has a NaN or Inf"
        );
    }
}

#[tokio::test]
#[ignore = "requires LATTICE_QWEN3_EMBEDDING_DIR to point at Qwen3-Embedding-0.6B weights"]
async fn qwen3_live_batching_matches_one_input_at_a_time() {
    let service = open(DType::F32);
    let texts = sample_texts();

    let started = Instant::now();
    let mut individually = Vec::with_capacity(texts.len());
    for text in &texts {
        individually.push(EmbeddingPort::embed_single(&service, text).await.unwrap());
    }
    let unbatched_secs = started.elapsed().as_secs_f64();

    let started = Instant::now();
    let batched = EmbeddingPort::embed_batch(&service, &texts).await.unwrap();
    let batched_secs = started.elapsed().as_secs_f64();

    assert_finite(&batched, "batched");
    let mut worst = f32::INFINITY;
    for (index, (one, many)) in individually.iter().zip(&batched).enumerate() {
        let similarity = cosine(one, many);
        worst = worst.min(similarity);
        assert!(
            similarity >= 0.9999,
            "text {index} ({} chars): cosine {similarity} between the batched and \
                 unbatched vectors",
            texts[index].len()
        );
    }

    println!(
        "qwen3 f32: {:.2} texts/s one at a time, {:.2} texts/s batched ({:.2}x), \
             worst cosine {worst:.6}",
        texts.len() as f64 / unbatched_secs,
        texts.len() as f64 / batched_secs,
        unbatched_secs / batched_secs,
    );
}

#[tokio::test]
#[ignore = "requires LATTICE_QWEN3_EMBEDDING_DIR to point at Qwen3-Embedding-0.6B weights"]
async fn qwen3_live_f16_matches_f32() {
    let mut texts = sample_texts();
    let queries = fixture_queries();
    println!(
        "comparing {} passages + {} queries",
        texts.len(),
        queries.len()
    );
    texts.extend(queries);

    let full = {
        let service = open(DType::F32);
        let started = Instant::now();
        let vectors = EmbeddingPort::embed_batch(&service, &texts).await.unwrap();
        println!(
            "qwen3 f32 batched: {:.2} texts/s",
            texts.len() as f64 / started.elapsed().as_secs_f64()
        );
        vectors
    };

    let half = {
        let service = open(DType::F16);
        let started = Instant::now();
        let vectors = EmbeddingPort::embed_batch(&service, &texts).await.unwrap();
        println!(
            "qwen3 f16 batched: {:.2} texts/s",
            texts.len() as f64 / started.elapsed().as_secs_f64()
        );
        vectors
    };

    assert_finite(&full, "f32");
    assert_finite(&half, "f16");
    let mut worst = f32::INFINITY;
    for (index, (f32_vector, f16_vector)) in full.iter().zip(&half).enumerate() {
        let similarity = cosine(f32_vector, f16_vector);
        worst = worst.min(similarity);
        assert!(
            similarity >= 0.999,
            "text {index} ({} chars): cosine {similarity} between f32 and f16",
            texts[index].len()
        );
    }
    println!(
        "worst f16-vs-f32 cosine over {} texts: {worst:.6}",
        texts.len()
    );
}

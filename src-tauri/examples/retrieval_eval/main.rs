//! Retrieval evaluation harness.
//!
//! Run:
//! ```text
//! cargo run --example retrieval_eval -- \
//!     [--mode embedding|production] [--top-k N] \
//!     [--strategy chunk-first|late-chunking] \
//!     [--compression none|i8|mrl<DIMS>|mrl<DIMS>-i8] [--sparse on|off|auto] \
//!     EMBEDDING_DIR DATASET_JSON [RERANKER_DIR]
//! ```
//!
//! `--mode embedding` (the default) keeps the historical behaviour: rank documents by the
//! maximum passage cosine of the raw embedding model, optionally reranking the top-48 pool.
//!
//! `--mode production` runs the app's real retrieval path instead: the indexer's contextual
//! chunking, a USearch HNSW index, an in-memory SQLite FTS5 table, `HybridSearchService`
//! reciprocal-rank fusion (k = 10), the shared cross-encoder blend, and the chat feature's
//! neighbouring-section evidence expansion.
//!
//! `--strategy`, `--compression` and `--sparse` select the retrieval features under
//! measurement. `--strategy` applies to both modes; the other two are production-only,
//! because they describe the index and the fusion, neither of which exists in embedding
//! mode. Every default reproduces the historical behaviour exactly.

mod production;

use lattice::application::ports::EmbeddingPort;
use lattice::features::embedding::candle_service::CandleEmbeddingService;
use lattice::features::embedding::late_chunking::EmbeddingStrategy;
use lattice::features::search::engine::reranker::{blend_rerank_scores, load_reranker, Reranker};
use lattice::features::search::engine::vector_search::{
    VectorIndexCompression, VectorQuantization,
};
use serde::Deserialize;
use std::{
    collections::{HashMap, HashSet},
    ops::Range,
    path::PathBuf,
    sync::Arc,
    time::Instant,
};

#[derive(Deserialize)]
pub struct Document {
    pub id: String,
    /// Absent in the starter fixture; the synthetic library supplies one per document.
    #[serde(default)]
    pub title: Option<String>,
    pub text: String,
}

#[derive(Deserialize)]
pub struct Query {
    pub id: String,
    pub text: String,
}

#[derive(Deserialize)]
pub struct Dataset {
    pub documents: Vec<Document>,
    pub queries: Vec<Query>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Embedding,
    Production,
}

impl Mode {
    pub fn label(self) -> &'static str {
        match self {
            Mode::Embedding => "embedding",
            Mode::Production => "production",
        }
    }
}

/// How the index stores vectors, before the embedding dimension is known.
///
/// `--compression i8` means "full dimension, quantized", and the full dimension
/// is a property of the loaded checkpoint, so the CLI token cannot be turned
/// into a [`VectorIndexCompression`] until the model is open.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompressionSpec {
    /// Store the embedding as produced: full dimension, `f32`.
    None,
    /// `dims: None` keeps the model's full dimension and quantizes only.
    Truncated {
        dims: Option<usize>,
        quantization: VectorQuantization,
    },
}

impl CompressionSpec {
    pub fn resolve(self, dimension: usize) -> VectorIndexCompression {
        match self {
            CompressionSpec::None => VectorIndexCompression::None,
            CompressionSpec::Truncated { dims, quantization } => {
                VectorIndexCompression::truncated(dims.unwrap_or(dimension), quantization)
            }
        }
    }
}

/// Whether the learned sparse branch joins the fusion.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SparseSetting {
    On,
    Off,
    /// On exactly when the loaded checkpoint has a sparse head. A model without
    /// one is not an error here — the branch simply never starts.
    Auto,
}

#[derive(Debug, PartialEq)]
pub struct Args {
    pub mode: Mode,
    pub top_k: usize,
    pub strategy: EmbeddingStrategy,
    pub compression: CompressionSpec,
    pub sparse: SparseSetting,
    pub fusion: FusionSpec,
    pub embedding_dir: PathBuf,
    pub dataset: PathBuf,
    pub reranker: Option<PathBuf>,
}

const USAGE: &str = "usage: retrieval_eval [--mode embedding|production] [--top-k N] \
                     [--strategy chunk-first|late-chunking] \
                     [--compression none|i8|mrl<DIMS>|mrl<DIMS>-i8] [--sparse on|off|auto] \
                     [--rrf-k K] [--weights VECTOR,KEYWORD] \
                     EMBEDDING_DIR DATASET_JSON [RERANKER_DIR]";

/// How the production hybrid path fuses its vector and BM25 branches.
///
/// The defaults are the application's (`features::search::di::build`): RRF at
/// `k = 10` with the 0.7 / 0.3 branch weights `SearchConfig` carries. Weights
/// of `1,1` reproduce plain unweighted RRF.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FusionSpec {
    pub rrf_k: f32,
    pub vector_weight: f32,
    pub keyword_weight: f32,
}

impl Default for FusionSpec {
    fn default() -> Self {
        Self {
            rrf_k: lattice::shared::constants::DEFAULT_RRF_K,
            vector_weight: 0.7,
            keyword_weight: 0.3,
        }
    }
}

fn parse_weights(value: &str) -> Result<(f32, f32), String> {
    let unknown =
        || format!("--weights expects VECTOR,KEYWORD as two non-negative numbers, got '{value}'");
    let (vector, keyword) = value.split_once(',').ok_or_else(unknown)?;
    let vector: f32 = vector.trim().parse().map_err(|_| unknown())?;
    let keyword: f32 = keyword.trim().parse().map_err(|_| unknown())?;
    if vector.is_nan() || keyword.is_nan() || vector < 0.0 || keyword < 0.0 {
        return Err(unknown());
    }
    if vector + keyword <= 0.0 {
        return Err(unknown());
    }
    Ok((vector, keyword))
}

/// Chunk candidates requested per query in production mode. Document-level metrics need
/// several distinct documents in the pool, so this is deliberately wider than the reported k.
const DEFAULT_TOP_K: usize = 50;

/// Extra room the chat retrieval path leaves for neighbouring-section evidence.
const NEIGHBOR_HEADROOM: usize = 8;

/// Row label for an embedding strategy. Snake case, like every other run field.
pub fn strategy_label(strategy: EmbeddingStrategy) -> &'static str {
    match strategy {
        EmbeddingStrategy::ChunkFirst => "chunk_first",
        EmbeddingStrategy::LateChunking => "late_chunking",
    }
}

/// Parse a `--compression` token. `i8` alone quantizes at the model's own
/// dimension; `mrl<DIMS>` truncates and keeps `f32`; `mrl<DIMS>-i8` does both.
pub fn parse_compression(value: &str) -> Result<CompressionSpec, String> {
    let unknown = || {
        format!(
            "unknown compression '{value}'; expected 'none', 'i8', 'mrl<DIMS>', or 'mrl<DIMS>-i8'"
        )
    };
    if value == "none" {
        return Ok(CompressionSpec::None);
    }
    if value == "i8" {
        return Ok(CompressionSpec::Truncated {
            dims: None,
            quantization: VectorQuantization::I8,
        });
    }
    let (head, quantization) = match value.strip_suffix("-i8") {
        Some(head) => (head, VectorQuantization::I8),
        None => (value, VectorQuantization::F32),
    };
    let dims = head
        .strip_prefix("mrl")
        .ok_or_else(unknown)?
        .parse::<usize>()
        .map_err(|_| unknown())?;
    if dims == 0 {
        return Err("--compression dims must be greater than zero".to_owned());
    }
    Ok(CompressionSpec::Truncated {
        dims: Some(dims),
        quantization,
    })
}

pub fn parse_args<I: IntoIterator<Item = String>>(raw: I) -> Result<Args, String> {
    let mut mode = Mode::Embedding;
    let mut top_k = DEFAULT_TOP_K;
    let mut strategy = EmbeddingStrategy::ChunkFirst;
    // Left unset until the whole line is read: `--mode` may follow the flag it
    // constrains, so "production only" cannot be decided inside the loop.
    let mut compression: Option<CompressionSpec> = None;
    let mut sparse: Option<SparseSetting> = None;
    let mut rrf_k: Option<f32> = None;
    let mut weights: Option<(f32, f32)> = None;
    let mut positional: Vec<String> = Vec::new();
    let mut remaining = raw.into_iter();
    while let Some(argument) = remaining.next() {
        let (flag, inline) = match argument.split_once('=') {
            Some((flag, value)) if flag.starts_with("--") => {
                (flag.to_owned(), Some(value.to_owned()))
            }
            _ => (argument.clone(), None),
        };
        match flag.as_str() {
            "--mode" => {
                let value = inline
                    .or_else(|| remaining.next())
                    .ok_or_else(|| "--mode requires a value".to_owned())?;
                mode = match value.as_str() {
                    "embedding" => Mode::Embedding,
                    "production" => Mode::Production,
                    other => {
                        return Err(format!(
                            "unknown mode '{other}'; expected 'embedding' or 'production'"
                        ))
                    }
                };
            }
            "--top-k" => {
                let value = inline
                    .or_else(|| remaining.next())
                    .ok_or_else(|| "--top-k requires a value".to_owned())?;
                top_k = value
                    .parse()
                    .map_err(|_| format!("--top-k expects a positive integer, got '{value}'"))?;
                if top_k == 0 {
                    return Err("--top-k must be greater than zero".to_owned());
                }
            }
            "--strategy" => {
                let value = inline
                    .or_else(|| remaining.next())
                    .ok_or_else(|| "--strategy requires a value".to_owned())?;
                strategy = match value.as_str() {
                    "chunk-first" => EmbeddingStrategy::ChunkFirst,
                    "late-chunking" => EmbeddingStrategy::LateChunking,
                    other => {
                        return Err(format!(
                            "unknown strategy '{other}'; expected 'chunk-first' or 'late-chunking'"
                        ))
                    }
                };
            }
            "--compression" => {
                let value = inline
                    .or_else(|| remaining.next())
                    .ok_or_else(|| "--compression requires a value".to_owned())?;
                compression = Some(parse_compression(&value)?);
            }
            "--rrf-k" => {
                let value = inline
                    .or_else(|| remaining.next())
                    .ok_or_else(|| "--rrf-k requires a value".to_owned())?;
                let parsed: f32 = value
                    .parse()
                    .map_err(|_| format!("--rrf-k expects a positive number, got '{value}'"))?;
                if parsed.is_nan() || parsed <= 0.0 {
                    return Err("--rrf-k must be greater than zero".to_owned());
                }
                rrf_k = Some(parsed);
            }
            "--weights" => {
                let value = inline
                    .or_else(|| remaining.next())
                    .ok_or_else(|| "--weights requires a value".to_owned())?;
                weights = Some(parse_weights(&value)?);
            }
            "--sparse" => {
                let value = inline
                    .or_else(|| remaining.next())
                    .ok_or_else(|| "--sparse requires a value".to_owned())?;
                sparse = Some(match value.as_str() {
                    "on" => SparseSetting::On,
                    "off" => SparseSetting::Off,
                    "auto" => SparseSetting::Auto,
                    other => {
                        return Err(format!(
                            "unknown sparse setting '{other}'; expected 'on', 'off', or 'auto'"
                        ))
                    }
                });
            }
            other if other.starts_with("--") => {
                return Err(format!("unknown flag '{other}'\n{USAGE}"))
            }
            _ => positional.push(argument),
        }
    }
    if !(2..=3).contains(&positional.len()) {
        return Err(USAGE.to_owned());
    }
    // Both describe the index and the fusion, and embedding mode builds
    // neither. Silently ignoring them would report a run the flags did not ask
    // for under a row that claims they were set.
    if mode == Mode::Embedding {
        if compression.is_some() {
            return Err("--compression is only available in --mode production".to_owned());
        }
        if sparse.is_some() {
            return Err("--sparse is only available in --mode production".to_owned());
        }
        if rrf_k.is_some() || weights.is_some() {
            return Err("--rrf-k and --weights are only available in --mode production".to_owned());
        }
    }
    let default_fusion = FusionSpec::default();
    let (vector_weight, keyword_weight) =
        weights.unwrap_or((default_fusion.vector_weight, default_fusion.keyword_weight));
    let embedding_dir = positional.first().ok_or_else(|| USAGE.to_owned())?.clone();
    let dataset = positional.get(1).ok_or_else(|| USAGE.to_owned())?.clone();
    Ok(Args {
        mode,
        top_k,
        strategy,
        compression: compression.unwrap_or(CompressionSpec::None),
        sparse: sparse.unwrap_or(SparseSetting::Auto),
        fusion: FusionSpec {
            rrf_k: rrf_k.unwrap_or(default_fusion.rrf_k),
            vector_weight,
            keyword_weight,
        },
        embedding_dir: PathBuf::from(embedding_dir),
        dataset: PathBuf::from(dataset),
        reranker: positional.get(2).map(PathBuf::from),
    })
}

/// Retrieval-side abstention signal: the best score and how far it stands above rank five.
/// A confident answer has a high top score and a wide gap; a corpus miss has neither.
pub fn score_summary(scores: &[f32]) -> (f32, f32) {
    let top = scores.first().copied().unwrap_or(0.0);
    let fifth = scores
        .get(4)
        .or_else(|| scores.last())
        .copied()
        .unwrap_or(0.0);
    (top, top - fifth)
}

/// Collapse a chunk-level ranking to its document ranking, keeping the best-ranked chunk
/// per document. This is how every downstream metric attributes a chunk hit to a document.
pub fn documents_in_order<'a, I: IntoIterator<Item = (&'a str, f32)>>(
    chunks: I,
) -> (Vec<String>, Vec<f32>) {
    let mut seen = HashSet::new();
    let mut ids = Vec::new();
    let mut scores = Vec::new();
    for (document_id, score) in chunks {
        if seen.insert(document_id.to_owned()) {
            ids.push(document_id.to_owned());
            scores.push(score);
        }
    }
    (ids, scores)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = parse_args(std::env::args().skip(1))?;
    let dataset: Dataset = serde_json::from_slice(&std::fs::read(&args.dataset)?)?;
    // `with_strategy(ChunkFirst)` is the constructor's own default, so the
    // chunk-first path is untouched; late chunking also changes
    // `model_identity()`, which is what keeps the two vector spaces apart.
    let model = Arc::new(
        CandleEmbeddingService::open_unregistered(&args.embedding_dir)?
            .with_strategy(args.strategy),
    );
    let reranker: Option<Arc<dyn Reranker>> = match &args.reranker {
        Some(path) => Some(load_reranker(path).await?),
        None => None,
    };
    match args.mode {
        Mode::Embedding => run_embedding(&model, &dataset, reranker.as_ref(), args.strategy).await,
        Mode::Production => run_production(model, &dataset, &args, reranker).await,
    }
}

async fn run_embedding(
    model: &CandleEmbeddingService,
    dataset: &Dataset,
    reranker: Option<&Arc<dyn Reranker>>,
    strategy: EmbeddingStrategy,
) -> Result<(), Box<dyn std::error::Error>> {
    let late_chunking = EmbeddingPort::uses_late_chunking(model);
    let mut passages = Vec::new();
    for doc in &dataset.documents {
        let parts = model.split_text(&doc.text, "")?;
        let inputs: Vec<String> = parts.iter().map(|part| part.text.clone()).collect();
        let embeddings = if late_chunking {
            // The passages are byte ranges of the document itself and there is
            // no context prefix in this mode, so the whole document is the
            // span: one forward pass, pooled per passage. `embed_span_chunks`
            // drops back to per-chunk embedding when the span does not fit the
            // model window, which is the documented behaviour, not a failure.
            let ranges: Vec<Range<usize>> = parts.iter().map(|part| part.start..part.end).collect();
            EmbeddingPort::embed_span_chunks(model, &doc.text, &ranges).await?
        } else {
            model.embed_batch(&inputs).await?
        };
        for (text, vector) in inputs.into_iter().zip(embeddings) {
            passages.push((doc.id.clone(), text, vector));
        }
    }
    for query in &dataset.queries {
        let start = Instant::now();
        let query_vector = model.embed_query(&query.text).await?;
        let mut scores = HashMap::<String, f32>::new();
        let mut passage_scores = Vec::with_capacity(passages.len());
        for (passage_index, (id, _, vector)) in passages.iter().enumerate() {
            let cosine = query_vector
                .iter()
                .zip(vector)
                .map(|(a, b)| a * b)
                .sum::<f32>();
            scores
                .entry(id.clone())
                .and_modify(|v| *v = v.max(cosine))
                .or_insert(cosine);
            passage_scores.push((passage_index, cosine));
        }
        let mut ranked: Vec<_> = scores.into_iter().collect();
        ranked.sort_by(|(a, av), (b, bv)| bv.total_cmp(av).then(a.cmp(b)));
        let first_stage_ids: Vec<String> = ranked.iter().map(|(id, _)| id.clone()).collect();
        let first_stage_scores: Vec<f32> = ranked.iter().map(|(_, score)| *score).collect();

        if let Some(reranker) = reranker {
            passage_scores.sort_by(|(a_idx, a_score), (b_idx, b_score)| {
                b_score.total_cmp(a_score).then(a_idx.cmp(b_idx))
            });
            passage_scores.truncate(48.min(passage_scores.len()));
            let mut candidate_texts = Vec::with_capacity(passage_scores.len());
            for (index, _) in &passage_scores {
                let passage = passages
                    .get(*index)
                    .ok_or("reranker candidate index was out of range")?;
                candidate_texts.push(passage.1.clone());
            }
            let candidate_scores: Vec<f32> =
                passage_scores.iter().map(|(_, score)| *score).collect();
            let reranked = reranker
                .rerank(&query.text, candidate_texts, candidate_scores.len())
                .await?;
            let blended = blend_rerank_scores(&candidate_scores, &reranked)?;
            let mut reranked_passages: Vec<(usize, f32)> = passage_scores
                .iter()
                .zip(blended)
                .map(|((passage_index, _), score)| (*passage_index, score))
                .collect();
            reranked_passages.sort_by(|(a_idx, a_score), (b_idx, b_score)| {
                b_score.total_cmp(a_score).then(a_idx.cmp(b_idx))
            });

            let mut seen = HashSet::new();
            ranked.clear();
            for (passage_index, score) in reranked_passages {
                let passage = passages
                    .get(passage_index)
                    .ok_or("reranker result index was out of range")?;
                let id = &passage.0;
                if seen.insert(id.clone()) {
                    ranked.push((id.clone(), score));
                }
            }
            for (id, score) in first_stage_ids.iter().zip(&first_stage_scores) {
                if seen.insert(id.clone()) {
                    ranked.push((id.clone(), *score));
                }
            }
        }

        let scores: Vec<f32> = ranked.iter().map(|(_, score)| *score).collect();
        let (top_score, score_spread) = score_summary(&scores);
        let row = serde_json::json!({
            "query_id": query.id,
            "mode": Mode::Embedding.label(),
            "ranked_ids": ranked.iter().map(|(id,_)| id).collect::<Vec<_>>(),
            "scores": scores,
            "first_stage_ranked_ids": first_stage_ids,
            "top_score": top_score,
            "score_spread": score_spread,
            "latency_ms": start.elapsed().as_secs_f64()*1000.0,
            "model_identity": model.model_identity(),
            "reranked": reranker.is_some(),
            "embedding_strategy": strategy_label(strategy),
        });
        println!("{}", serde_json::to_string(&row)?);
    }
    Ok(())
}

async fn run_production(
    model: Arc<CandleEmbeddingService>,
    dataset: &Dataset,
    args: &Args,
    reranker: Option<Arc<dyn Reranker>>,
) -> Result<(), Box<dyn std::error::Error>> {
    let compression = args.compression.resolve(EmbeddingPort::dimension(&*model));
    // `auto` is the honest default: a checkpoint without a sparse head simply
    // keeps two-way fusion. `on` is a claim about the model, so it fails loudly
    // rather than reporting a run whose sparse row field would be a lie.
    let sparse_enabled = match args.sparse {
        SparseSetting::On => {
            if !EmbeddingPort::supports_sparse(&*model) {
                return Err(format!(
                    "--sparse on needs an embedding model with a sparse head; {} has none",
                    model.model_identity()
                )
                .into());
            }
            true
        }
        SparseSetting::Off => false,
        SparseSetting::Auto => EmbeddingPort::supports_sparse(&*model),
    };
    let index = production::ProductionIndex::build(
        Arc::clone(&model),
        &args.embedding_dir,
        &dataset.documents,
        reranker.clone(),
        args.top_k,
        production::IndexOptions {
            compression: compression.clone(),
            sparse_enabled,
            rrf_k: args.fusion.rrf_k,
            vector_weight: args.fusion.vector_weight,
            keyword_weight: args.fusion.keyword_weight,
        },
    )
    .await?;
    let compression_token = compression
        .layout_token()
        .unwrap_or_else(|| "none".to_owned());
    let evidence_limit = args.top_k + NEIGHBOR_HEADROOM;
    for query in &dataset.queries {
        let start = Instant::now();
        let query_vector = model.embed_query(&query.text).await?;
        let (mut chunks, sufficiency) = index.rank(&query.text, &query_vector).await?;
        // Fused (and, when enabled, reranked) ordering before section neighbours join the
        // evidence set. Neighbours are appended context, never first-stage ranking evidence.
        let (first_stage_ids, _) =
            documents_in_order(chunks.iter().map(|c| (c.document_id.as_str(), c.score)));
        index.expand(&mut chunks, evidence_limit).await?;
        let latency_ms = start.elapsed().as_secs_f64() * 1000.0;

        // Branch diagnostics run outside the timed region so latency stays comparable
        // with embedding mode: one query embedding plus one retrieval.
        let vector_branch = index
            .branch_documents(&query.text, &query_vector, production::VECTOR_BRANCH)
            .await?;
        let bm25_branch = index
            .branch_documents(&query.text, &query_vector, production::BM25_BRANCH)
            .await?;
        let mut branches = serde_json::Map::new();
        branches.insert("vector".to_owned(), serde_json::json!(vector_branch));
        branches.insert("bm25".to_owned(), serde_json::json!(bm25_branch));
        if let Some(sparse_branch) = index.sparse_documents(&query.text).await? {
            branches.insert("sparse".to_owned(), serde_json::json!(sparse_branch));
        }

        let (ranked_ids, scores) =
            documents_in_order(chunks.iter().map(|c| (c.document_id.as_str(), c.score)));
        let (top_score, score_spread) = score_summary(&scores);
        let row = serde_json::json!({
            "query_id": query.id,
            "mode": Mode::Production.label(),
            "ranked_ids": ranked_ids,
            "scores": scores,
            "first_stage_ranked_ids": first_stage_ids,
            "chunk_ranked_ids": chunks.iter().map(production::RankedChunk::locator).collect::<Vec<_>>(),
            "branch_ranked_ids": branches,
            "top_score": top_score,
            "score_spread": score_spread,
            "latency_ms": latency_ms,
            "model_identity": model.model_identity(),
            "reranked": reranker.is_some(),
            "embedding_strategy": strategy_label(args.strategy),
            "compression": compression_token,
            "sparse": sparse_enabled,
            "rrf_k": args.fusion.rrf_k,
            "fusion_weights": [args.fusion.vector_weight, args.fusion.keyword_weight],
            "sufficient": sufficiency.sufficient,
            "sufficiency_reasons": sufficiency.reasons,
            "term_coverage": sufficiency.term_coverage,
        });
        println!("{}", serde_json::to_string(&row)?);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(raw: &[&str]) -> Result<Args, String> {
        parse_args(raw.iter().map(|s| (*s).to_owned()))
    }

    #[test]
    fn existing_positional_invocations_keep_working() {
        let parsed = args(&["/models/minilm", "starter.json"]).expect("positional args parse");
        assert_eq!(parsed.mode, Mode::Embedding);
        assert_eq!(parsed.embedding_dir, PathBuf::from("/models/minilm"));
        assert_eq!(parsed.dataset, PathBuf::from("starter.json"));
        assert_eq!(parsed.reranker, None);
        assert_eq!(parsed.top_k, DEFAULT_TOP_K);
        // Every new knob defaults to the behaviour that existed before it.
        assert_eq!(parsed.strategy, EmbeddingStrategy::ChunkFirst);
        assert_eq!(parsed.compression, CompressionSpec::None);
        assert_eq!(parsed.sparse, SparseSetting::Auto);
        assert_eq!(parsed.fusion, FusionSpec::default());

        let with_reranker =
            args(&["/models/minilm", "starter.json", "/models/rr"]).expect("reranker arg parses");
        assert_eq!(with_reranker.reranker, Some(PathBuf::from("/models/rr")));
    }

    #[test]
    fn mode_and_top_k_accept_both_spellings() {
        for raw in [
            vec!["--mode", "production", "--top-k", "12", "/m", "d.json"],
            vec!["--mode=production", "--top-k=12", "/m", "d.json"],
        ] {
            let parsed = args(&raw).expect("flag spelling parses");
            assert_eq!(parsed.mode, Mode::Production);
            assert_eq!(parsed.top_k, 12);
            assert_eq!(parsed.embedding_dir, PathBuf::from("/m"));
        }
    }

    #[test]
    fn bad_flags_and_arities_are_rejected() {
        for raw in [
            vec!["--mode", "hybrid", "/m", "d.json"],
            vec!["--top-k", "0", "/m", "d.json"],
            vec!["--top-k", "many", "/m", "d.json"],
            vec!["--unknown", "/m", "d.json"],
            vec!["/m"],
            vec!["/m", "d.json", "/rr", "extra"],
            vec!["--strategy", "late", "/m", "d.json"],
            vec![
                "--mode",
                "production",
                "--compression",
                "mrl",
                "/m",
                "d.json",
            ],
            vec![
                "--mode",
                "production",
                "--compression",
                "mrl0",
                "/m",
                "d.json",
            ],
            vec![
                "--mode",
                "production",
                "--compression",
                "int8",
                "/m",
                "d.json",
            ],
            vec!["--mode", "production", "--sparse", "yes", "/m", "d.json"],
            // The index and the fusion are production-only, so silently ignoring
            // these in embedding mode would report a run nobody asked for.
            vec!["--compression", "i8", "/m", "d.json"],
            vec!["--sparse", "off", "/m", "d.json"],
            vec!["--rrf-k", "10", "/m", "d.json"],
            vec!["--weights", "1,1", "/m", "d.json"],
            vec!["--mode", "production", "--rrf-k", "0", "/m", "d.json"],
            vec!["--mode", "production", "--weights", "0,0", "/m", "d.json"],
            vec!["--mode", "production", "--weights", "0.7", "/m", "d.json"],
            // Order must not matter: `--mode` may follow the flag it constrains.
            vec!["--sparse", "off", "--mode", "embedding", "/m", "d.json"],
        ] {
            assert!(args(&raw).is_err(), "expected {raw:?} to be rejected");
        }
    }

    #[test]
    fn strategy_and_production_only_flags_parse() {
        let parsed = args(&[
            "--mode",
            "production",
            "--strategy",
            "late-chunking",
            "--compression",
            "mrl256-i8",
            "--sparse",
            "on",
            "/m",
            "d.json",
        ])
        .expect("production flags parse");
        assert_eq!(parsed.strategy, EmbeddingStrategy::LateChunking);
        assert_eq!(
            parsed.compression,
            CompressionSpec::Truncated {
                dims: Some(256),
                quantization: VectorQuantization::I8,
            }
        );
        assert_eq!(parsed.sparse, SparseSetting::On);

        // `--strategy` is the one new flag embedding mode also honours.
        let embedding = args(&["--strategy=late-chunking", "/m", "d.json"])
            .expect("strategy parses in embedding mode");
        assert_eq!(embedding.strategy, EmbeddingStrategy::LateChunking);
        assert_eq!(embedding.mode, Mode::Embedding);
    }

    #[test]
    fn compression_tokens_name_the_stored_vector_space() {
        // `i8` alone quantizes at whatever dimension the loaded model reports.
        assert_eq!(
            parse_compression("i8").expect("i8").resolve(384),
            VectorIndexCompression::truncated(384, VectorQuantization::I8)
        );
        assert_eq!(
            parse_compression("mrl256").expect("mrl256").resolve(1024),
            VectorIndexCompression::truncated(256, VectorQuantization::F32)
        );
        assert_eq!(
            parse_compression("mrl256-i8")
                .expect("mrl256-i8")
                .resolve(1024),
            VectorIndexCompression::truncated(256, VectorQuantization::I8)
        );
        // `none` keeps the historical layout, which deliberately has no token.
        assert_eq!(
            parse_compression("none").expect("none").resolve(384),
            VectorIndexCompression::None
        );
        assert_eq!(VectorIndexCompression::None.layout_token(), None);
        assert_eq!(
            parse_compression("mrl256-i8")
                .expect("mrl256-i8")
                .resolve(1024)
                .layout_token()
                .as_deref(),
            Some("mrl256i8")
        );
    }

    #[test]
    fn strategy_labels_are_the_row_spellings() {
        assert_eq!(strategy_label(EmbeddingStrategy::ChunkFirst), "chunk_first");
        assert_eq!(
            strategy_label(EmbeddingStrategy::LateChunking),
            "late_chunking"
        );
    }

    #[test]
    fn fusion_flags_parse_in_production_mode() {
        let parsed = args(&[
            "--mode=production",
            "--rrf-k=10",
            "--weights=1, 1",
            "/m",
            "d.json",
        ])
        .expect("fusion flags parse");
        assert_eq!(
            parsed.fusion,
            FusionSpec {
                rrf_k: 10.0,
                vector_weight: 1.0,
                keyword_weight: 1.0
            }
        );
    }

    #[test]
    fn score_summary_reports_top_and_gap_to_rank_five() {
        assert_eq!(
            score_summary(&[0.9, 0.8, 0.7, 0.6, 0.5, 0.4]),
            (0.9, 0.9 - 0.5)
        );
        // Short rankings fall back to the last score rather than inventing a zero floor.
        assert_eq!(score_summary(&[0.9, 0.4]), (0.9, 0.9 - 0.4));
        assert_eq!(score_summary(&[]), (0.0, 0.0));
    }

    #[test]
    fn document_ranking_keeps_the_best_chunk_per_document() {
        let chunks = [("a", 0.9_f32), ("b", 0.7), ("a", 0.6), ("c", 0.5)];
        let (ids, scores) = documents_in_order(chunks.iter().map(|(id, s)| (*id, *s)));
        assert_eq!(ids, ["a", "b", "c"]);
        assert_eq!(scores, [0.9, 0.7, 0.5]);
    }
}

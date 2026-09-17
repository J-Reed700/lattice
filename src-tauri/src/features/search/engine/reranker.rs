//! # Cross-Encoder Reranker (Candle)
//!
//! Uses Candle on CPU with cross-encoder/ms-marco-MiniLM-L-6-v2.
//!
//! ## Why we changed both runtime and model
//!
//! The old setup combined two latent bugs:
//!
//! 1. **Wrong tensor index.** mxbai-rerank-base-v2 outputs shape
//!    `[batch, 1]` (single regression score), but the old code did
//!    `output_tensor.get([i, 1])` which read the second column — which
//!    doesn't exist. The fallback `.get([i, 0])` then provided the
//!    actual score, so ranking still ~worked, but in a non-obvious way
//!    that masked an out-of-bounds access.
//!
//! 2. **Wrong tokenization.** mxbai uses a Qwen2 tokenizer with no
//!    `[SEP]` token. The old code's `format!("{} [SEP] {}", query, doc)`
//!    silently fell back to splitting `[`, `S`, `E`, `P`, `]` into
//!    separate tokens, polluting attention with irrelevant junk.
//!
//! Both bugs were invisible because reranking only re-orders existing
//! BM25/vector hits — the user can't see the score axis directly, only
//! which docs end up first. Result quality degrades subtly.
//!
//! ms-marco-MiniLM-L-6-v2 is a 22M-param classic BERT cross-encoder
//! trained on MS MARCO. Its tokenizer DOES have `[SEP]`. Its config
//! correctly declares `num_labels=1`. Architecture matches candle's
//! `BertModel` directly.
//!
//! ## Output semantics
//!
//! Sigmoided to `[0, 1]`. Higher = more relevant. The model's HF
//! config technically says `sbert_ce_default_activation_function:
//! Identity` — meaning the trained logit isn't a calibrated
//! probability. But the existing downstream consumer
//! (`apply_cross_encoder_rerank` in `chat/retrieval/rerank.rs`)
//! `.clamp(0.0, 1.0)`s the score before blending it with normalized
//! BM25/vector scores. Returning raw logits would make the clamp
//! collapse all positive logits to 1.0 (destroying ranking), so we
//! sigmoid here to give the consumer a smooth `[0, 1]` curve.
//!
//! Sigmoid is monotonic, so the ordering candle would have produced
//! from raw logits is preserved. The blend weights the consumer
//! applies (0.65 reranker, 0.35 original) work because both inputs
//! are now on the same scale.

use crate::shared::error::{AppError, Result, ResultExt};
use crate::shared::utils::with_autorelease_pool;
use candle_core::{DType, Device, IndexOp, Tensor};
use candle_nn::{linear, Linear, Module, VarBuilder};
use candle_transformers::models::bert::{BertModel, Config};
use parking_lot::Mutex;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokenizers::{Encoding, Tokenizer, TruncationParams, TruncationStrategy};

use super::qwen3_reranker::Qwen3RerankerService;

/// Per-query reranking result.
#[derive(Debug, Clone)]
pub struct RerankResult {
    /// Index into the original `documents` vec.
    pub index: usize,
    /// Normalized cross-encoder score. Higher = more relevant.
    pub score: f32,
}

/// Shared reranking contract used by every retrieval entry point.
#[async_trait::async_trait]
pub trait Reranker: Send + Sync + std::fmt::Debug {
    /// Whether the backing model artifacts are currently available.
    fn is_available(&self) -> bool;

    /// Score and order the supplied documents for a query.
    async fn rerank(
        &self,
        query: &str,
        documents: Vec<String>,
        top_k: usize,
    ) -> Result<Vec<RerankResult>>;
}

const DEFAULT_INIT_RETRY_DELAY: Duration = Duration::from_secs(5 * 60);

#[derive(Default)]
struct LazyRerankerState {
    instance: Option<Arc<dyn Reranker>>,
    last_init_failure: Option<Instant>,
}

/// Lazily loads one shared reranker and notices artifacts downloaded after boot.
pub struct LazyReranker {
    model_path: PathBuf,
    state: tokio::sync::Mutex<LazyRerankerState>,
    retry_delay: Duration,
}

impl std::fmt::Debug for LazyReranker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LazyReranker")
            .field("model_path", &self.model_path)
            .field("retry_delay", &self.retry_delay)
            .finish_non_exhaustive()
    }
}

impl LazyReranker {
    pub fn new(model_path: impl Into<PathBuf>) -> Self {
        Self {
            model_path: model_path.into(),
            state: tokio::sync::Mutex::new(LazyRerankerState::default()),
            retry_delay: DEFAULT_INIT_RETRY_DELAY,
        }
    }

    fn artifacts_available(&self) -> bool {
        let Some(model_dir) = self.model_path.parent() else {
            return false;
        };
        self.model_path.is_file()
            && model_dir.join("tokenizer.json").is_file()
            && model_dir.join("config.json").is_file()
    }

    async fn get_or_init(&self) -> Result<Arc<dyn Reranker>> {
        if !self.artifacts_available() {
            return Err(AppError::InvalidState(format!(
                "Reranker artifacts are not available at {}",
                self.model_path.display()
            )));
        }

        // Keep initialization under one lock so concurrent searches cannot load
        // duplicate model copies. Inference itself uses RerankerService's own lock.
        let mut state = self.state.lock().await;
        if let Some(service) = state.instance.as_ref() {
            return Ok(Arc::clone(service));
        }
        if state
            .last_init_failure
            .is_some_and(|failed_at| failed_at.elapsed() < self.retry_delay)
        {
            return Err(AppError::InvalidState(
                "Reranker initialization is cooling down after a load failure".to_string(),
            ));
        }

        match load_reranker(&self.model_path).await {
            Ok(service) => {
                state.instance = Some(Arc::clone(&service));
                state.last_init_failure = None;
                Ok(service)
            }
            Err(error) => {
                state.last_init_failure = Some(Instant::now());
                Err(error)
            }
        }
    }
}

#[derive(Debug, serde::Deserialize)]
struct RerankerModelConfig {
    model_type: String,
    #[serde(default)]
    architectures: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RerankerArchitecture {
    BertSequenceClassifier,
    Qwen3CausalClassifier,
}

fn detect_reranker_architecture(config: &RerankerModelConfig) -> Result<RerankerArchitecture> {
    match config.model_type.to_ascii_lowercase().as_str() {
        "bert" => Ok(RerankerArchitecture::BertSequenceClassifier),
        "qwen3"
            if config
                .architectures
                .iter()
                .any(|architecture| architecture == "Qwen3ForCausalLM") =>
        {
            Ok(RerankerArchitecture::Qwen3CausalClassifier)
        }
        "qwen3" => Err(AppError::InvalidConfig(
            "Qwen3 reranker artifacts must declare Qwen3ForCausalLM; an embedding checkpoint cannot be used as a reranker"
                .to_string(),
        )),
        other => Err(AppError::InvalidConfig(format!(
            "Unsupported reranker architecture `{other}`"
        ))),
    }
}

/// Load the correct local reranker implementation from its checkpoint metadata.
///
/// Both chat retrieval and direct search call through the same [`Reranker`]
/// contract. Model selection belongs here so callers cannot accidentally apply
/// different prompt, scoring, or fallback policies.
pub async fn load_reranker(model_path: impl AsRef<Path>) -> Result<Arc<dyn Reranker>> {
    let resolved = resolve_model_dir(model_path.as_ref())?;
    let config_path = resolved.dir.join("config.json");
    let config_json =
        std::fs::read_to_string(&config_path).context("Failed to read reranker config.json")?;
    let config: RerankerModelConfig = serde_json::from_str(&config_json).map_err(|error| {
        AppError::InvalidConfig(format!("Invalid reranker config.json: {error}"))
    })?;

    match detect_reranker_architecture(&config)? {
        RerankerArchitecture::BertSequenceClassifier => {
            Ok(Arc::new(RerankerService::new(&resolved.dir).await?))
        }
        RerankerArchitecture::Qwen3CausalClassifier => {
            Ok(Arc::new(Qwen3RerankerService::new(&resolved.dir).await?))
        }
    }
}

#[async_trait::async_trait]
impl Reranker for LazyReranker {
    fn is_available(&self) -> bool {
        self.artifacts_available()
    }

    async fn rerank(
        &self,
        query: &str,
        documents: Vec<String>,
        top_k: usize,
    ) -> Result<Vec<RerankResult>> {
        self.get_or_init()
            .await?
            .rerank(query, documents, top_k)
            .await
    }
}

/// Blend reranker scores with the first-stage retrieval scores.
///
/// The reranker must return exactly one finite score for each candidate. Keeping
/// this policy here prevents chat and direct search from drifting apart.
pub fn blend_rerank_scores(original_scores: &[f32], reranked: &[RerankResult]) -> Result<Vec<f32>> {
    if original_scores.is_empty() {
        return Ok(Vec::new());
    }
    if reranked.len() != original_scores.len() {
        return Err(AppError::InvalidState(format!(
            "Reranker returned {} scores for {} candidates",
            reranked.len(),
            original_scores.len()
        )));
    }

    let mut seen = HashSet::with_capacity(reranked.len());
    let mut rerank_scores = vec![0.0_f32; original_scores.len()];
    for entry in reranked {
        if !entry.score.is_finite() {
            return Err(AppError::InvalidState(
                "Reranker returned a non-finite score".to_string(),
            ));
        }
        let Some(score) = rerank_scores.get_mut(entry.index) else {
            return Err(AppError::InvalidState(format!(
                "Reranker returned out-of-range candidate index {}",
                entry.index
            )));
        };
        if !seen.insert(entry.index) {
            return Err(AppError::InvalidState(format!(
                "Reranker returned duplicate candidate index {}",
                entry.index
            )));
        }
        *score = entry.score.clamp(0.0, 1.0);
    }

    if original_scores.iter().any(|score| !score.is_finite()) {
        return Err(AppError::InvalidState(
            "First-stage retrieval returned a non-finite score".to_string(),
        ));
    }
    let max_original = original_scores
        .iter()
        .copied()
        .fold(0.0_f32, f32::max)
        .max(f32::EPSILON);

    Ok(original_scores
        .iter()
        .zip(rerank_scores)
        .map(|(original, rerank)| {
            let normalized = (*original / max_original).clamp(0.0, 1.0);
            (0.35 * normalized + 0.65 * rerank).clamp(0.0, 1.0)
        })
        .collect())
}

/// Cross-encoder reranker. Loaded once on demand and kept warm.
pub struct RerankerService {
    /// `parking_lot::Mutex` because Candle inference is sync-only and
    /// holding a `tokio::sync::Mutex` across blocking compute is the
    /// kind of thing that wedges runtimes. The reranker call goes
    /// through `tokio::task::spawn_blocking` so the lock contention is
    /// the only point of serialization, and reranking is fast enough
    /// (~40ms) that throughput isn't a concern.
    inner: Arc<Mutex<Inner>>,
    tokenizer: Arc<Tokenizer>,
    max_length: usize,
}

/// Explicit name for the compact BERT implementation. Existing callers may
/// continue to use the original `RerankerService` name.
pub type MiniLmRerankerService = RerankerService;

/// Holds the model and the score-projection head. `Mutex`-guarded
/// because `BertModel::forward` mutates internal state via the
/// `tracing::Span`.
struct Inner {
    model: BertModel,
    /// Linear `[hidden_size, 1]`. Loaded from `classifier.{weight,bias}`
    /// in the safetensors file (the convention HF
    /// `BertForSequenceClassification` exports use).
    classifier: Linear,
    device: Device,
}

impl std::fmt::Debug for RerankerService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RerankerService")
            .field("max_length", &self.max_length)
            .finish_non_exhaustive()
    }
}

impl RerankerService {
    /// Load the reranker from a directory containing `model.safetensors`,
    /// `tokenizer.json`, and `config.json`.
    ///
    /// `model_path` is accepted as either the directory or the
    /// `model.safetensors` file inside it; we normalize internally.
    /// This matches the path shape the rest of Lattice's model storage
    /// uses (most callers pass the file path directly).
    pub async fn new(model_path: impl AsRef<Path>) -> Result<Self> {
        let resolved = resolve_model_dir(model_path.as_ref())?;
        let model_dir = resolved.dir;
        let safetensors_path = resolved.weights;
        let tokenizer_path = model_dir.join("tokenizer.json");
        let config_path = model_dir.join("config.json");

        if !tokenizer_path.exists() {
            return Err(AppError::Other(format!(
                "Reranker tokenizer.json not found at {}",
                tokenizer_path.display()
            )));
        }
        if !config_path.exists() {
            return Err(AppError::Other(format!(
                "Reranker config.json not found at {}",
                config_path.display()
            )));
        }

        // Tokenizer is platform-agnostic; Tokenizer::from_file is
        // synchronous and fast (~ms), no need to spawn_blocking.
        let max_length = 512;
        let mut tokenizer = Tokenizer::from_file(&tokenizer_path)
            .map_err(|e| AppError::Other(format!("Failed to load tokenizer: {e}")))?;
        // Let the tokenizer truncate the pair before it adds special tokens. A
        // raw `.take(512)` can discard the final [SEP] and produce input the
        // cross-encoder never saw during training.
        tokenizer
            .with_truncation(Some(TruncationParams {
                max_length,
                strategy: TruncationStrategy::LongestFirst,
                ..TruncationParams::default()
            }))
            .map_err(|e| AppError::TokenizationError {
                reason: format!("Failed to configure reranker truncation: {e}"),
            })?;
        tokenizer.with_padding(None);

        // The Config + weight load are also fast for a 90MB BERT-tiny
        // model, but we run them in spawn_blocking anyway because
        // mmap-loading + tensor allocation can briefly spike CPU.
        let safetensors_owned = safetensors_path.clone();
        let config_owned = config_path.clone();
        let inner = tokio::task::spawn_blocking(move || -> Result<Inner> {
            with_autorelease_pool(move || -> Result<Inner> {
                let config_json = std::fs::read_to_string(&config_owned)
                    .context("Failed to read reranker config.json")?;
                let config: Config = serde_json::from_str(&config_json).map_err(|e| {
                    AppError::Other(format!("Failed to parse reranker config.json: {e}"))
                })?;

                let device = best_reranker_device();

                // SAFETY: VarBuilder::from_mmaped_safetensors is unsafe
                // because it mmap-loads the safetensors file — Rust can't
                // statically guarantee the file won't be modified while
                // the mapping is live. We rely on three invariants:
                //   1. The model file lives in the app's models directory;
                //      no other Lattice code path writes to it after the
                //      one-time download.
                //   2. The file is read-only mmap'd (PROT_READ); accidental
                //      writes through the mapping would segfault, not
                //      corrupt.
                //   3. The download flow uses atomic temp+rename
                //      (model_manager.rs), so any future re-download
                //      replaces the inode, not its contents.
                // Standard candle pattern; same shape used by the
                // embedding loader at features/embedding/candle_service.rs.
                #[allow(unsafe_code)]
                let vb = unsafe {
                    VarBuilder::from_mmaped_safetensors(&[&safetensors_owned], DType::F32, &device)
                }
                .map_err(|e| AppError::Other(format!("Failed to mmap reranker weights: {e}")))?;

                // BertModel::load tries `embeddings`/`encoder` first, then
                // falls back to `bert.embeddings`/`bert.encoder` if a
                // `model_type` is set in the config (which it is —
                // `model_type: "bert"`). Cross-encoder safetensors use the
                // `bert.*` prefix, so the fallback is what'll fire.
                let model = BertModel::load(vb.clone(), &config)
                    .map_err(|e| AppError::Other(format!("Failed to build BertModel: {e}")))?;

                // The classifier head: a single linear from hidden_size → 1.
                // HF `BertForSequenceClassification` exports the head at
                // `classifier.{weight, bias}` (no `bert.` prefix because it
                // sits next to, not under, the encoder).
                // Candle Metal currently rejects this degenerate hidden_size×1
                // matrix shape. Keep the expensive encoder on the accelerator and
                // load the tiny classifier head on CPU.
                let classifier_device = Device::Cpu;
                #[allow(unsafe_code)]
                let classifier_vb = unsafe {
                    VarBuilder::from_mmaped_safetensors(
                        &[&safetensors_owned],
                        DType::F32,
                        &classifier_device,
                    )
                }
                .map_err(|e| AppError::Other(format!("Failed to mmap classifier weights: {e}")))?;
                let classifier = linear(config.hidden_size, 1, classifier_vb.pp("classifier"))
                    .map_err(|e| {
                        AppError::Other(format!(
                            "Failed to load classifier head (expected `classifier.weight`/\
                     `classifier.bias` in safetensors): {e}"
                        ))
                    })?;

                Ok(Inner {
                    model,
                    classifier,
                    device,
                })
            })
        })
        .await
        .map_err(|e| AppError::Other(format!("Reranker load task panicked: {e}")))??;

        tracing::info!(
            "RerankerService initialized (BERT cross-encoder, candle CPU): {}",
            safetensors_path.display()
        );

        Ok(Self {
            inner: Arc::new(Mutex::new(inner)),
            tokenizer: Arc::new(tokenizer),
            max_length,
        })
    }

    /// Score the query against each document and return top-k results
    /// sorted by score descending.
    pub async fn rerank(
        &self,
        query: &str,
        documents: Vec<String>,
        top_k: usize,
    ) -> Result<Vec<RerankResult>> {
        if documents.is_empty() {
            return Ok(vec![]);
        }

        let inner = Arc::clone(&self.inner);
        let tokenizer = Arc::clone(&self.tokenizer);
        let query = query.to_string();
        let max_length = self.max_length;

        tokio::task::spawn_blocking(move || {
            with_autorelease_pool(|| {
                rerank_sync(&inner, &tokenizer, &query, documents, top_k, max_length)
            })
        })
        .await
        .map_err(|e| AppError::Other(format!("Reranking task failed: {e}")))?
    }

    pub fn with_max_length(mut self, max_length: usize) -> Result<Self> {
        if max_length < 3 {
            return Err(AppError::InvalidConfig(
                "Reranker max length must leave room for special tokens".to_string(),
            ));
        }
        Arc::make_mut(&mut self.tokenizer)
            .with_truncation(Some(TruncationParams {
                max_length,
                strategy: TruncationStrategy::LongestFirst,
                ..TruncationParams::default()
            }))
            .map_err(|e| AppError::TokenizationError {
                reason: format!("Failed to configure reranker truncation: {e}"),
            })?;
        self.max_length = max_length;
        Ok(self)
    }
}

#[async_trait::async_trait]
impl Reranker for RerankerService {
    fn is_available(&self) -> bool {
        true
    }

    async fn rerank(
        &self,
        query: &str,
        documents: Vec<String>,
        top_k: usize,
    ) -> Result<Vec<RerankResult>> {
        RerankerService::rerank(self, query, documents, top_k).await
    }
}

pub(crate) struct ResolvedModel {
    /// Directory containing `model.safetensors`, `tokenizer.json`, `config.json`.
    pub(crate) dir: std::path::PathBuf,
    /// Path to `model.safetensors`.
    pub(crate) weights: std::path::PathBuf,
}

/// Accept either a directory or a `model.safetensors` file path; return
/// both resolved consistently. Older call sites passed the legacy
/// `model.onnx` filename — we treat that as "the directory it sits in"
/// and look for `model.safetensors` next to it. The ONNX fallback supports
/// users upgrading from older releases.
pub(crate) fn resolve_model_dir(path: &Path) -> Result<ResolvedModel> {
    let (dir, weights) = if path.is_dir() {
        (path.to_path_buf(), path.join("model.safetensors"))
    } else {
        let dir = path
            .parent()
            .context("Reranker model path has no parent directory")?
            .to_path_buf();
        let weights = if path.extension().and_then(|e| e.to_str()) == Some("safetensors") {
            path.to_path_buf()
        } else {
            // Old `model.onnx` path — repoint at the safetensors
            // sibling. If it doesn't exist the .new() check below will
            // surface a clean error.
            dir.join("model.safetensors")
        };
        (dir, weights)
    };

    if !weights.exists() {
        return Err(AppError::Other(format!(
            "Reranker model.safetensors not found at {}",
            weights.display()
        )));
    }

    Ok(ResolvedModel { dir, weights })
}

/// Match embedding inference: prefer the platform accelerator and retain a CPU fallback.
pub(crate) fn best_reranker_device() -> Device {
    crate::shared::utils::best_available_compute_device("reranking")
}

fn rerank_sync(
    inner: &Arc<Mutex<Inner>>,
    tokenizer: &Tokenizer,
    query: &str,
    documents: Vec<String>,
    top_k: usize,
    max_length: usize,
) -> Result<Vec<RerankResult>> {
    let scores = score_batch(inner, tokenizer, query, &documents, max_length)?;

    let mut ranked: Vec<RerankResult> = scores
        .into_iter()
        .enumerate()
        .map(|(idx, score)| RerankResult { index: idx, score })
        .collect();

    ranked.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    ranked.truncate(top_k);
    Ok(ranked)
}

/// Encode `(query, doc)` pairs and run the cross-encoder forward pass
/// in mini-batches. Returns one logit per document.
fn score_batch(
    inner: &Arc<Mutex<Inner>>,
    tokenizer: &Tokenizer,
    query: &str,
    documents: &[String],
    max_length: usize,
) -> Result<Vec<f32>> {
    if documents.is_empty() {
        return Ok(vec![]);
    }

    // Build (query, doc) pairs. The tokenizer's `encode_batch` with
    // a `(String, String)` input produces the standard
    // `[CLS] query [SEP] doc [SEP]` format with correct
    // `token_type_ids` (0 for query, 1 for doc) — this is what BERT
    // cross-encoders are trained against. Don't try to roll our own
    // `format!("{q} [SEP] {d}")` — the tokenizer handles segment IDs
    // and special tokens correctly.
    let pairs: Vec<(String, String)> = documents
        .iter()
        .map(|d| (query.to_string(), d.clone()))
        .collect();

    let encodings =
        tokenizer
            .encode_batch(pairs, true)
            .map_err(|e| AppError::TokenizationError {
                reason: format!("Tokenization failed: {e}"),
            })?;
    if encodings.len() != documents.len() {
        return Err(AppError::InvalidState(format!(
            "Reranker tokenizer returned {} encodings for {} documents",
            encodings.len(),
            documents.len()
        )));
    }

    // BERT attention cost is quadratic in padded sequence length. Sorting by
    // encoded length prevents one 512-token passage from making every short
    // passage in the same batch pay the maximum cost.
    let mut indexed_encodings: Vec<(usize, Encoding)> = encodings.into_iter().enumerate().collect();
    indexed_encodings.sort_by_key(|(_, encoding)| encoding.len());

    let batch_size = 16usize;
    let mut all_scores = vec![0.0_f32; documents.len()];
    let mut assigned = vec![false; documents.len()];
    let mut batch_start = 0usize;
    while batch_start < indexed_encodings.len() {
        let shortest = indexed_encodings
            .get(batch_start)
            .map(|(_, encoding)| encoding.len().max(1))
            .ok_or_else(|| {
                AppError::InvalidState("Reranker batch start was out of range".to_string())
            })?;
        let bucket_max = shortest.saturating_mul(2);
        let mut batch_end = batch_start + 1;
        while batch_end < indexed_encodings.len()
            && batch_end - batch_start < batch_size
            && indexed_encodings
                .get(batch_end)
                .is_some_and(|(_, encoding)| encoding.len() <= bucket_max)
        {
            batch_end += 1;
        }
        let batch = indexed_encodings
            .get(batch_start..batch_end)
            .ok_or_else(|| {
                AppError::InvalidState("Reranker batch bounds were inconsistent".to_string())
            })?;
        let batch_scores = score_encoded_batch(inner, batch, max_length)?;
        if batch_scores.len() != batch.len() {
            return Err(AppError::InvalidState(
                "Reranker produced an incomplete batch".to_string(),
            ));
        }
        for ((original_index, _), score) in batch.iter().zip(batch_scores) {
            let Some(output) = all_scores.get_mut(*original_index) else {
                return Err(AppError::InvalidState(
                    "Reranker batch index was out of range".to_string(),
                ));
            };
            let Some(was_assigned) = assigned.get_mut(*original_index) else {
                return Err(AppError::InvalidState(
                    "Reranker assignment index was out of range".to_string(),
                ));
            };
            if *was_assigned {
                return Err(AppError::InvalidState(
                    "Reranker assigned a candidate twice".to_string(),
                ));
            }
            *output = score;
            *was_assigned = true;
        }
        batch_start = batch_end;
    }
    if assigned.iter().any(|assigned| !assigned) {
        return Err(AppError::InvalidState(
            "Reranker did not score every candidate".to_string(),
        ));
    }

    Ok(all_scores)
}

fn score_encoded_batch(
    inner: &Arc<Mutex<Inner>>,
    encodings: &[(usize, Encoding)],
    max_length: usize,
) -> Result<Vec<f32>> {
    let batch_size = encodings.len();
    if batch_size == 0 {
        return Ok(vec![]);
    }

    let sequence_length = encodings
        .iter()
        .map(|(_, encoding)| encoding.len())
        .max()
        .unwrap_or(1)
        .min(max_length)
        .max(1);

    let mut input_ids_flat = Vec::with_capacity(batch_size * sequence_length);
    let mut attention_mask_flat = Vec::with_capacity(batch_size * sequence_length);
    let mut token_type_ids_flat = Vec::with_capacity(batch_size * sequence_length);

    for (_, encoding) in encodings {
        let ids = encoding.get_ids();
        let mask = encoding.get_attention_mask();
        let types = encoding.get_type_ids();

        // Pair-aware truncation already preserved the tokenizer's special
        // tokens. Pad to this batch's sequence length with 0 because the
        // BERT tokenizer's pad token id is 0 (per config.json
        // `pad_token_id: 0`). The attention_mask is 0 for pads, which
        // makes BertModel's softmax mask them out.
        let mut row_ids: Vec<u32> = ids.iter().take(sequence_length).copied().collect();
        let mut row_mask: Vec<u32> = mask.iter().take(sequence_length).copied().collect();
        let mut row_types: Vec<u32> = types.iter().take(sequence_length).copied().collect();
        let pad = sequence_length.saturating_sub(row_ids.len());
        row_ids.extend(std::iter::repeat_n(0u32, pad));
        row_mask.extend(std::iter::repeat_n(0u32, pad));
        row_types.extend(std::iter::repeat_n(0u32, pad));

        input_ids_flat.extend(row_ids);
        attention_mask_flat.extend(row_mask);
        token_type_ids_flat.extend(row_types);
    }

    let guard = inner.lock();
    let device = guard.device.clone();

    // BERT expects i64 input_ids. candle_nn::Embedding handles the
    // dtype conversion if we feed u32, but matching HF's published
    // shape exactly avoids surprises.
    let input_ids = Tensor::from_vec(input_ids_flat, (batch_size, sequence_length), &device)
        .map_err(|e| AppError::Other(format!("Failed to build input_ids tensor: {e}")))?
        .to_dtype(DType::U32)
        .map_err(|e| AppError::Other(format!("input_ids dtype cast failed: {e}")))?;
    let attention_mask =
        Tensor::from_vec(attention_mask_flat, (batch_size, sequence_length), &device)
            .map_err(|e| AppError::Other(format!("Failed to build attention_mask tensor: {e}")))?
            .to_dtype(DType::U32)
            .map_err(|e| AppError::Other(format!("attention_mask dtype cast failed: {e}")))?;
    let token_type_ids =
        Tensor::from_vec(token_type_ids_flat, (batch_size, sequence_length), &device)
            .map_err(|e| AppError::Other(format!("Failed to build token_type_ids tensor: {e}")))?
            .to_dtype(DType::U32)
            .map_err(|e| AppError::Other(format!("token_type_ids dtype cast failed: {e}")))?;

    // Forward pass: [batch, seq, hidden]. We need the [CLS] hidden
    // state — index 0 along seq.
    let hidden_states = guard
        .model
        .forward(&input_ids, &token_type_ids, Some(&attention_mask))
        .map_err(|e| AppError::Other(format!("BertModel forward failed: {e}")))?;

    // [batch, hidden]
    let cls_states = hidden_states
        .i((.., 0, ..))
        .map_err(|e| AppError::Other(format!("Failed to slice CLS token: {e}")))?
        .to_device(&Device::Cpu)
        .map_err(|e| AppError::Other(format!("Failed to transfer CLS state to CPU: {e}")))?;

    // [batch, 1] — single logit per pair
    let logits = guard
        .classifier
        .forward(&cls_states)
        .map_err(|e| AppError::Other(format!("Classifier forward failed: {e}")))?;

    drop(guard);

    let logits_shape = logits.dims();
    if logits_shape.len() != 2 || logits_shape.get(1) != Some(&1) {
        return Err(AppError::Other(format!(
            "Reranker output has unexpected shape {:?}; expected [batch, 1] for a \
             single-label cross-encoder",
            logits_shape
        )));
    }

    let raw_logits: Vec<f32> = logits
        .squeeze(1)
        .map_err(|e| AppError::Other(format!("Failed to squeeze logits: {e}")))?
        .to_vec1()
        .map_err(|e| AppError::Other(format!("Failed to materialize logits: {e}")))?;

    // Sigmoid into [0, 1] so the downstream blend (which clamps and
    // mixes with BM25/vector scores) preserves rank ordering. See
    // module-level "Output semantics" docs.
    let scores = raw_logits.into_iter().map(sigmoid).collect();
    Ok(scores)
}

#[inline]
fn sigmoid(x: f32) -> f32 {
    1.0 / (1.0 + (-x).exp())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rerank_result_ordering() {
        // Sanity: sort descending by score.
        let mut results = [
            RerankResult {
                index: 0,
                score: 0.5,
            },
            RerankResult {
                index: 1,
                score: 0.9,
            },
            RerankResult {
                index: 2,
                score: 0.3,
            },
        ];
        results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        assert_eq!(results[0].index, 1);
        assert_eq!(results[1].index, 0);
        assert_eq!(results[2].index, 2);
    }

    #[test]
    fn shared_blend_maps_scores_back_to_original_candidates() {
        let reranked = vec![
            RerankResult {
                index: 1,
                score: 0.9,
            },
            RerankResult {
                index: 0,
                score: 0.2,
            },
        ];

        let scores = blend_rerank_scores(&[1.0, 0.5], &reranked).unwrap();

        assert!((scores[0] - 0.48).abs() < 0.0001);
        assert!((scores[1] - 0.76).abs() < 0.0001);
    }

    #[test]
    fn shared_blend_rejects_duplicate_indices() {
        let reranked = vec![
            RerankResult {
                index: 0,
                score: 0.9,
            },
            RerankResult {
                index: 0,
                score: 0.2,
            },
        ];

        assert!(blend_rerank_scores(&[1.0, 0.5], &reranked).is_err());
    }

    #[test]
    fn shared_blend_rejects_incomplete_or_non_finite_output() {
        let incomplete = vec![RerankResult {
            index: 0,
            score: 0.9,
        }];
        assert!(blend_rerank_scores(&[1.0, 0.5], &incomplete).is_err());

        let non_finite = vec![RerankResult {
            index: 0,
            score: f32::NAN,
        }];
        assert!(blend_rerank_scores(&[1.0], &non_finite).is_err());
    }

    #[test]
    fn loader_distinguishes_bert_and_qwen3_causal_rerankers() {
        let bert = RerankerModelConfig {
            model_type: "bert".to_string(),
            architectures: vec!["BertForSequenceClassification".to_string()],
        };
        assert_eq!(
            detect_reranker_architecture(&bert).unwrap(),
            RerankerArchitecture::BertSequenceClassifier
        );

        let qwen = RerankerModelConfig {
            model_type: "qwen3".to_string(),
            architectures: vec!["Qwen3ForCausalLM".to_string()],
        };
        assert_eq!(
            detect_reranker_architecture(&qwen).unwrap(),
            RerankerArchitecture::Qwen3CausalClassifier
        );
    }

    #[test]
    fn loader_rejects_qwen3_embedding_checkpoint() {
        let embedding = RerankerModelConfig {
            model_type: "qwen3".to_string(),
            architectures: vec!["Qwen3Model".to_string()],
        };
        assert!(detect_reranker_architecture(&embedding).is_err());
    }

    #[test]
    fn lazy_reranker_notices_artifacts_added_after_startup() {
        let tmp = tempfile::tempdir().unwrap();
        let model_path = tmp.path().join("model.safetensors");
        let reranker = LazyReranker::new(&model_path);
        assert!(!reranker.is_available());

        std::fs::write(&model_path, b"weights").unwrap();
        std::fs::write(tmp.path().join("tokenizer.json"), b"{}").unwrap();
        std::fs::write(tmp.path().join("config.json"), b"{}").unwrap();

        assert!(reranker.is_available());
    }

    #[test]
    fn resolve_model_dir_accepts_directory() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("model.safetensors"), b"").unwrap();
        let resolved = resolve_model_dir(tmp.path()).unwrap();
        assert_eq!(resolved.dir, tmp.path());
        assert_eq!(resolved.weights, tmp.path().join("model.safetensors"));
    }

    #[test]
    fn resolve_model_dir_accepts_safetensors_file() {
        let tmp = tempfile::tempdir().unwrap();
        let weights = tmp.path().join("model.safetensors");
        std::fs::write(&weights, b"").unwrap();
        let resolved = resolve_model_dir(&weights).unwrap();
        assert_eq!(resolved.dir, tmp.path());
        assert_eq!(resolved.weights, weights);
    }

    #[test]
    fn resolve_model_dir_redirects_legacy_onnx_path() {
        // Old callers pass `reranker/model.onnx`. We treat that as
        // "look for model.safetensors in the same directory."
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("model.safetensors"), b"").unwrap();
        let onnx_path = tmp.path().join("model.onnx");
        // model.onnx itself doesn't need to exist; we redirect to the
        // safetensors sibling regardless.
        let resolved = resolve_model_dir(&onnx_path).unwrap();
        assert_eq!(resolved.weights, tmp.path().join("model.safetensors"));
    }

    #[test]
    fn resolve_model_dir_errors_when_weights_missing() {
        let tmp = tempfile::tempdir().unwrap();
        let result = resolve_model_dir(tmp.path());
        assert!(result.is_err());
    }

    #[test]
    fn sigmoid_maps_correctly() {
        assert!((sigmoid(0.0) - 0.5).abs() < 1e-6);
        assert!(sigmoid(10.0) > 0.999);
        assert!(sigmoid(-10.0) < 0.001);
        // Monotonic on a sample of values.
        let xs = [-3.0, -1.0, 0.0, 1.0, 3.0];
        let ys: Vec<f32> = xs.iter().copied().map(sigmoid).collect();
        for w in ys.windows(2) {
            assert!(w[0] < w[1], "sigmoid not monotonic: {:?}", ys);
        }
    }
}

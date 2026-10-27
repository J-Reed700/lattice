//! # Cross-Encoder Reranker (Candle)
//!
//! [`load_reranker`] reads a checkpoint's `config.json` and loads either a
//! BERT sequence classifier (cross-encoder/ms-marco-MiniLM-L-6-v2) or a Qwen3
//! yes/no reranker ([`Qwen3RerankerService`]); the app downloads
//! Qwen/Qwen3-Reranker-0.6B. Both run on the platform accelerator.
//!
//! Scores are sigmoided to `[0, 1]`, higher meaning more relevant: the
//! consumer ([`blend_rerank_scores`]) clamps to `[0, 1]` before blending,
//! which would collapse raw positive logits to 1.0. Sigmoid is monotonic, so
//! the ordering is kept.

use candle_core::{DType, Device};
use candle_nn::{linear, Linear, VarBuilder};
use candle_transformers::models::bert::{BertModel, Config};
use lattice_core::shared::error::{AppError, Result, ResultExt};
use lattice_core::shared::runtime::with_autorelease_pool;
use parking_lot::Mutex;
use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;
use tokenizers::{Tokenizer, TruncationParams, TruncationStrategy};

use super::qwen3_reranker::Qwen3RerankerService;

mod inference;
mod loading;

use inference::rerank_sync;
pub use loading::{load_reranker, LazyReranker};

/// Per-query reranking result.
#[derive(Debug, Clone)]
pub struct RerankResult {
    /// Index into the original `documents` vec.
    pub index: usize,
    /// Normalized cross-encoder score. Higher = more relevant.
    pub score: f32,
}

/// Acquire capacity before creating a blocking task, and keep the permit in
/// the closure because Tokio cannot stop inference after it has started.
pub(super) async fn run_admitted<F, T>(admission: Arc<tokio::sync::Semaphore>, work: F) -> Result<T>
where
    F: FnOnce() -> Result<T> + Send + 'static,
    T: Send + 'static,
{
    let permit = admission
        .acquire_owned()
        .await
        .map_err(|_| AppError::InvalidState("Reranker admission closed".into()))?;
    tokio::task::spawn_blocking(move || {
        let _permit = permit;
        work()
    })
    .await
    .map_err(|error| AppError::Other(format!("Reranking task failed: {error}")))?
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
    inference_admission: Arc<tokio::sync::Semaphore>,
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
                // Standard candle pattern.
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
            inference_admission: Arc::new(tokio::sync::Semaphore::new(1)),
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

        // Admit before consuming a blocking-pool thread. Started blocking
        // inference cannot be aborted, so the permit travels into the closure.
        let inner = Arc::clone(&self.inner);
        let tokenizer = Arc::clone(&self.tokenizer);
        let query = query.to_string();
        let max_length = self.max_length;

        run_admitted(Arc::clone(&self.inference_admission), move || {
            with_autorelease_pool(|| {
                rerank_sync(&inner, &tokenizer, &query, documents, top_k, max_length)
            })
        })
        .await
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
    crate::infrastructure::ml::compute_device::best_available_compute_device("reranking")
}

#[cfg(test)]
mod tests;

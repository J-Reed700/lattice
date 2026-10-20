//! Candle-backed embedding service.
//!
//! Pure-Rust ONNX-free embedding inference. Replaces `OnnxEmbeddingService` for
//! the full-cutover migration. Loads BERT-family checkpoints directly via
//! `candle_transformers`, from `model.safetensors` when the repository
//! publishes one and from the `pytorch_model.bin` pickle when it does not.
//! Falls back from Metal → CPU if the GPU device is unavailable.
//!
//! # Architecture
//!
//! - `ModelArchitecture`: enum dispatch for the BERT variants we support today
//!   (vanilla BERT, DistilBERT, XLM-RoBERTa, MPNet, Jina v2, Nomic, ModernBERT).
//!   Qwen3 uses a separate causal forward pass with last-token pooling.
//!   Other unsupported decoder families fail clearly at load time.
//!
//! - `PoolingStrategy`: read from `1_Pooling/config.json` when present.
//!   Defaults to CLS for BGE/mxbai/etc. and Mean for sentence-transformers.
//!   We always L2-normalize the pooled vector.
//!
//! - Thread-safety: a single `tokio::sync::Mutex<ModelVariant>` serializes
//!   forward passes. Metal can crash on parallel kernel dispatch from multiple
//!   threads, so we serialize per-service (not global).
//!
//! - Dimension: read from `config.json::hidden_size`. Exposed via `dimension()`
//!   so the USearch index can be sized to match (and re-built on mismatch).

use std::path::PathBuf;

use candle_core::Device;
use candle_transformers::models::bert::BertModel;
use candle_transformers::models::distilbert::DistilBertModel;
use candle_transformers::models::jina_bert::BertModel as JinaBertModel;
use candle_transformers::models::modernbert::ModernBert;
use candle_transformers::models::nomic_bert::NomicBertModel;
use candle_transformers::models::xlm_roberta::XLMRobertaModel;
use lru::LruCache;
use serde::Deserialize;
use std::sync::Arc;
use tokenizers::Tokenizer;
use tokio::sync::Mutex;

use crate::features::embedding::late_chunking::EmbeddingStrategy;
use crate::features::embedding::prefixes::EmbeddingPrefixes;
use crate::features::embedding::qwen3_encoder::Qwen3Encoder;
use crate::features::embedding::sparse_head::SparseHead;
use lattice_core::domain::value_objects::ArtifactIdentity;
use lattice_core::shared::error::AppError;

mod batching;
mod config;
mod forward;
mod loading;
mod ports;
mod spans;

pub use config::prefixes_for_model_dir;

/// Encoder families and the Qwen3 decoder supported by the local runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelArchitecture {
    /// `model_type: "bert"` — BGE, all-MiniLM, mxbai, UAE, Granite, etc.
    Bert,
    /// `model_type: "distilbert"` — DistilBERT-based embedders.
    DistilBert,
    /// `model_type: "xlm-roberta"` — Multilingual E5, XLM-R based.
    XlmRoberta,
    /// `model_type: "mpnet"` — all-mpnet-base.
    Mpnet,
    /// `model_type: "jina_bert"` — Jina v2 (ALiBi-based).
    JinaBert,
    /// `model_type: "nomic_bert"` — Nomic (RoPE + SwiGLU).
    NomicBert,
    /// `model_type: "modernbert"` — ModernBERT (RoPE + GeGLU).
    ModernBert,
    Qwen3,
}

impl ModelArchitecture {
    fn from_model_type(model_type: &str) -> Option<Self> {
        match model_type.to_ascii_lowercase().as_str() {
            "bert" => Some(Self::Bert),
            "distilbert" => Some(Self::DistilBert),
            "xlm-roberta" | "xlm_roberta" | "roberta" => Some(Self::XlmRoberta),
            "mpnet" => Some(Self::Mpnet),
            "jina_bert" | "jina_bert_v2" => Some(Self::JinaBert),
            "nomic_bert" => Some(Self::NomicBert),
            "modernbert" => Some(Self::ModernBert),
            "qwen3" => Some(Self::Qwen3),
            _ => None,
        }
    }
}

/// Pooling strategy applied to the model's last hidden states to produce a
/// single vector per input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PoolingStrategy {
    /// Take the [CLS] token (index 0). Used by BGE, mxbai, UAE.
    Cls,
    /// Mean over all non-padding tokens. Used by all-MiniLM, all-mpnet, Nomic.
    Mean,
}

/// Subset of `1_Pooling/config.json` schema. sentence-transformers writes
/// these alongside the model when exporting; we read whichever flag is set.
#[derive(Debug, Deserialize, Default)]
struct PoolingConfig {
    #[serde(default)]
    pooling_mode_cls_token: bool,
    #[serde(default)]
    pooling_mode_mean_tokens: bool,
}

/// Subset of `config.json` we need: architecture identification + dimension.
#[derive(Debug, Deserialize)]
struct ModelConfig {
    model_type: String,
    hidden_size: usize,
}

/// Internal dispatch over the loaded Candle model. Each BERT-family arch
/// uses a different Candle module because the layer plumbing differs
/// (DistilBERT skips token_type_ids, Nomic uses RoPE, ModernBERT uses
/// global+local attention, etc.). Gemma3 is a placeholder for the
/// upcoming decoder-style PR — currently unreachable behind the
/// architecture-rejection check.
enum ModelVariant {
    Bert(BertModel),
    DistilBert(DistilBertModel),
    XlmRoberta(XLMRobertaModel),
    JinaBert(JinaBertModel),
    NomicBert(NomicBertModel),
    ModernBert(ModernBert),
    Qwen3(Qwen3Encoder),
}

/// The padded tokens (`rows × the longest row`) one encoder batch may cost.
/// Sixteen rows of a 512-token BERT, which is what the old fixed batch of 16
/// cost at its worst — now it is a ceiling on the worst case instead of the
/// shape of every case.
const ENCODER_PADDED_TOKEN_BUDGET: usize = 16 * 512;

/// The same ceiling for Qwen3, at a quarter the size. Attention materializes
/// `rows × heads × len²` scores, so its cost grows with the square of the
/// longest row rather than with the padded token count. At the 2048-token
/// window this budget is two rows and about 0.5 GB of transient scores in F32
/// (half that at F16); at a 512-token chunk it is eight rows and about 134 MB.
/// Any larger and a batch of window-length passages allocates more than a
/// gigabyte in a single pass.
const QWEN3_PADDED_TOKEN_BUDGET: usize = 4 * 1024;

/// Rows per batch regardless of the token budget. Thousands of one-token rows
/// fit any budget but cost more in per-row tensor bookkeeping than the batching
/// saves.
const MAX_BATCH_ROWS: usize = 64;

/// Queries kept in the per-service embedding cache. A chat turn re-embeds the
/// same question a handful of times — first-pass retrieval, planner rewrites,
/// tool-loop searches — so this only has to outlive a turn, not a session.
const QUERY_CACHE_CAPACITY: usize = 256;

/// Errors specific to model loading. Surfaced through `AppError::EmbeddingFailed`
/// at the trait boundary but enumerated here so callers can match if needed.
#[derive(Debug, thiserror::Error)]
pub enum LoadError {
    #[error("config.json not found at {0}")]
    MissingConfig(PathBuf),
    #[error("tokenizer.json not found at {0}")]
    MissingTokenizer(PathBuf),
    #[error("neither model.safetensors nor pytorch_model.bin found in {0}")]
    MissingWeights(PathBuf),
    #[error("unsupported embedding architecture: {0}")]
    UnsupportedArchitecture(String),
    #[error("config.json parse error: {0}")]
    BadConfig(String),
    #[error("candle error: {0}")]
    Candle(String),
    #[error("tokenizer error: {0}")]
    Tokenizer(String),
}

impl From<LoadError> for AppError {
    fn from(error: LoadError) -> Self {
        AppError::EmbeddingFailed {
            reason: error.to_string(),
        }
    }
}

/// Candle-backed embedding service.
pub struct CandleEmbeddingService {
    model: Arc<Mutex<ModelVariant>>,
    tokenizer: Arc<Tokenizer>,
    device: Device,
    dimension: usize,
    pooling: PoolingStrategy,
    architecture: ModelArchitecture,
    input_policy: super::input_policy::InputPolicy,
    /// Digest of the model artifacts. The public identity also folds in the
    /// embedding strategy, which is what actually decides vector comparability.
    identity: ArtifactIdentity,
    strategy: EmbeddingStrategy,
    /// BGE-M3's learned sparse head, when this checkpoint ships one. `None`
    /// for every other model, which is what makes `supports_sparse()` false
    /// and keeps the sparse retrieval branch from ever starting.
    sparse_head: Option<Arc<SparseHead>>,
    /// The instruction strings this model family was trained with, resolved
    /// once at load from [`super::prefixes`].
    prefixes: &'static EmbeddingPrefixes,
    /// Query vectors for queries this service already embedded, keyed by the
    /// exact prefixed string the model saw.
    ///
    /// The cache belongs to the service, so switching models cannot serve a
    /// stale vector: the new model gets a new service and an empty cache, and
    /// the old one is dropped with its entries. Documents are never cached —
    /// they are embedded once, and 256 of them would be a pointless megabyte.
    query_cache: std::sync::Mutex<LruCache<String, Arc<Vec<f32>>>>,
    /// Bytes the weights hold on `device` when it is an accelerator; 0 on
    /// the CPU. A local LLM server offloading to the same GPU shares it.
    accelerator_bytes: u64,
}

impl CandleEmbeddingService {
    /// Opt this service into a different [`EmbeddingStrategy`]. Off by default.
    ///
    /// ```ignore
    /// let service = CandleEmbeddingService::open(model_dir, identity)?
    ///     .with_strategy(EmbeddingStrategy::LateChunking);
    /// ```
    ///
    /// The returned service reports a different `model_identity()`, so its
    /// vectors live in their own generation and are never mixed with
    /// chunk-first vectors of the same model.
    #[must_use]
    pub fn with_strategy(mut self, strategy: EmbeddingStrategy) -> Self {
        self.strategy = strategy;
        self
    }

    /// The active embedding strategy.
    pub fn embedding_strategy(&self) -> EmbeddingStrategy {
        self.strategy
    }

    /// The model's output dimension (`hidden_size` from config.json).
    pub fn dimension(&self) -> usize {
        self.dimension
    }

    /// Architecture detected from `model_type`.
    pub fn architecture(&self) -> ModelArchitecture {
        self.architecture
    }
}

#[cfg(test)]
pub(crate) mod tests;

/// Checks that only real Qwen3-Embedding weights can answer: that a padded
/// batch pools to the same vectors as one pass per input, and that half
/// precision tracks full precision closely enough to share an index.
///
/// ```text
/// LATTICE_QWEN3_EMBEDDING_DIR=/path/to/Qwen3-Embedding-0.6B \
///   cargo test --lib qwen3_live -- --ignored --nocapture --test-threads=1
/// ```
#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]
mod qwen3_live_tests;

/// Passages a second through real weights, and where the time goes; see
/// the module for how to run it.
#[cfg(test)]
#[cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::indexing_slicing
    )
)]
mod throughput_bench;

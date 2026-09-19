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

use std::num::NonZeroUsize;
use std::ops::Range;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use candle_core::{DType, Device, Module, Tensor};
use candle_nn::VarBuilder;
use candle_transformers::models::bert::{BertModel, Config as BertConfig, HiddenAct};
use candle_transformers::models::distilbert::{Config as DistilBertConfig, DistilBertModel};
use candle_transformers::models::jina_bert::{
    BertModel as JinaBertModel, Config as JinaBertConfig,
};
use candle_transformers::models::modernbert::{Config as ModernBertConfig, ModernBert};
use candle_transformers::models::nomic_bert::{Config as NomicBertConfig, NomicBertModel};
use candle_transformers::models::xlm_roberta::{Config as XlmRobertaConfig, XLMRobertaModel};
use lru::LruCache;
use serde::Deserialize;
use std::sync::Arc;
use tokenizers::{PaddingDirection, PaddingParams, PaddingStrategy, Tokenizer};
use tokio::sync::Mutex;

use crate::application::ports::embedding_port::{span_chunk_texts, sparse_not_supported};
use crate::application::ports::EmbeddingPort;
use crate::domain::value_objects::{ArtifactIdentity, SparseEmbedding};
use crate::features::embedding::late_chunking::{
    l2_normalize_in_place, mean_pool_rows, pooling_token_indices, strategy_identity,
    validate_chunk_ranges, window_groups, EmbeddingStrategy, LateChunkingError,
};
use crate::features::embedding::prefixes::EmbeddingPrefixes;
use crate::features::embedding::qwen3_encoder::Qwen3Encoder;
use crate::features::embedding::sparse_head::{SparseHead, MAX_PASSAGE_TERMS, MAX_QUERY_TERMS};
use crate::features::embedding::EmbeddingServiceTrait;
use crate::shared::error::AppError;
use crate::shared::result::Result;
use crate::shared::utils::with_autorelease_pool;

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

/// The weights file Candle prefers: a memory-mappable safetensors archive.
pub const WEIGHTS_SAFETENSORS: &str = "model.safetensors";

/// The PyTorch pickle accepted when a repository never published a safetensors
/// conversion. `BAAI/bge-m3` is that case in the curated catalog — its only
/// root-level weights file is `pytorch_model.bin`, so requiring safetensors
/// made the model impossible to download or load at all.
pub const WEIGHTS_PYTORCH_BIN: &str = "pytorch_model.bin";

/// The weights file to load out of `model_dir`, or `None` when it holds
/// neither candidate.
///
/// Safetensors wins when both are present: it mmaps instead of materializing
/// every tensor, and it is the format every existing model identity was
/// computed over.
pub fn weights_path(model_dir: &Path) -> Option<PathBuf> {
    [WEIGHTS_SAFETENSORS, WEIGHTS_PYTORCH_BIN]
        .into_iter()
        .map(|name| model_dir.join(name))
        .find(|path| path.is_file())
}

/// Whether `model_dir` holds weights the Candle runtime can load.
pub fn has_loadable_weights(model_dir: &Path) -> bool {
    weights_path(model_dir).is_some()
}

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
}

impl CandleEmbeddingService {
    /// Load a model from a directory containing `config.json`,
    /// `tokenizer.json`, and either `model.safetensors` or `pytorch_model.bin`,
    /// and label it with `identity`. Does not hash anything: `identity` comes
    /// from the model's row, where activation recorded it.
    ///
    /// Tries Metal first on macOS, falls back to CPU on failure. Reads the
    /// model dimension from `config.json::hidden_size` and the pooling
    /// strategy from `1_Pooling/config.json` if present (defaults to CLS).
    ///
    /// Embedding strategy defaults to [`EmbeddingStrategy::ChunkFirst`]. Use
    /// [`CandleEmbeddingService::with_strategy`] to opt a freshly loaded
    /// service into late chunking.
    ///
    /// TODO(settings): there is no user-facing switch for the embedding
    /// strategy yet — `application/contracts/settings.rs` is owned elsewhere.
    /// When a setting lands, read it where the model is loaded
    /// (`infrastructure/embedding_loading.rs`) and call `with_strategy`.
    /// Flipping it re-generates every vector, because `model_identity()`
    /// changes with the strategy.
    pub fn open(model_dir: impl AsRef<Path>, identity: ArtifactIdentity) -> Result<Self> {
        // Uploading the weights is thousands of Metal dispatches; drain what
        // they autorelease here instead of leaving it on the loading thread.
        with_autorelease_pool(|| Self::open_in_pool(model_dir.as_ref(), identity, None))
    }

    /// `open` with the weights dtype pinned rather than chosen by
    /// [`weights_dtype`]. Only the dtype-validation tests need this; production
    /// takes what the architecture and device imply.
    #[cfg(test)]
    fn open_as(
        model_dir: impl AsRef<Path>,
        identity: ArtifactIdentity,
        dtype: DType,
    ) -> Result<Self> {
        with_autorelease_pool(|| Self::open_in_pool(model_dir.as_ref(), identity, Some(dtype)))
    }

    fn open_in_pool(dir: &Path, identity: ArtifactIdentity, dtype: Option<DType>) -> Result<Self> {
        let config_path = dir.join("config.json");
        let tokenizer_path = dir.join("tokenizer.json");

        if !config_path.exists() {
            return Err(LoadError::MissingConfig(config_path).into());
        }
        if !tokenizer_path.exists() {
            return Err(LoadError::MissingTokenizer(tokenizer_path).into());
        }
        let Some(weights_path) = weights_path(dir) else {
            return Err(LoadError::MissingWeights(dir.to_path_buf()).into());
        };

        let config_bytes = std::fs::read(&config_path).map_err(|e| AppError::FileRead {
            path: config_path.display().to_string(),
            reason: e.to_string(),
        })?;
        let config: ModelConfig = serde_json::from_slice(&config_bytes)
            .map_err(|e| LoadError::BadConfig(e.to_string()))?;

        let architecture = ModelArchitecture::from_model_type(&config.model_type)
            .ok_or_else(|| LoadError::UnsupportedArchitecture(config.model_type.clone()))?;

        // Reject known architectures without a working Candle runner.
        match architecture {
            ModelArchitecture::Bert
            | ModelArchitecture::DistilBert
            | ModelArchitecture::XlmRoberta
            | ModelArchitecture::JinaBert
            | ModelArchitecture::NomicBert
            | ModelArchitecture::ModernBert
            | ModelArchitecture::Qwen3 => {}
            ModelArchitecture::Mpnet => {
                return Err(LoadError::UnsupportedArchitecture(format!(
                    "MPNet (model_type='{}') — Candle has no MPNet loader; planned",
                    config.model_type
                ))
                .into());
            }
        }

        let pooling = read_pooling_strategy(dir);
        let prefixes = resolve_prefixes(dir, &config_bytes, architecture);

        let device = crate::shared::utils::best_available_compute_device("embedding");
        let dtype = dtype.unwrap_or_else(|| weights_dtype(architecture, &device));
        tracing::info!(
            device = ?device,
            dtype = ?dtype,
            model_type = %config.model_type,
            hidden_size = config.hidden_size,
            pooling = ?pooling,
            prefixes = prefixes.id,
            "Loading Candle embedding model"
        );

        let mut tokenizer = super::input_policy::load_tokenizer(&tokenizer_path)?;

        let input_policy = super::input_policy::InputPolicy::new(
            tokenizer.clone(),
            super::input_policy::model_token_limit(dir)?.min(2048),
        )?;
        // Preserve model-specific padding IDs (RoBERTa uses 1, BERT usually 0,
        // Qwen3 pads with its end-of-text token).
        let padding = tokenizer.get_padding().cloned().unwrap_or_else(|| {
            let pad_id = tokenizer
                .token_to_id("[PAD]")
                .or_else(|| tokenizer.token_to_id("<pad>"))
                .or_else(|| tokenizer.token_to_id("<|endoftext|>"))
                .unwrap_or(0);
            PaddingParams {
                pad_id,
                pad_token: tokenizer
                    .id_to_token(pad_id)
                    .unwrap_or_else(|| "[PAD]".into()),
                ..Default::default()
            }
        });
        // Right padding is load-bearing for both paths: the encoders mask it
        // out by attention mask, and Qwen3's causal attention makes it
        // unreachable from every position that gets pooled. Left padding would
        // silently shift every position.
        tokenizer.with_padding(Some(PaddingParams {
            strategy: PaddingStrategy::BatchLongest,
            direction: PaddingDirection::Right,
            ..padding
        }));
        tokenizer
            .with_truncation(None)
            .map_err(|e| LoadError::Tokenizer(e.to_string()))?;

        let var_builder = if weights_path
            .file_name()
            .is_some_and(|name| name == WEIGHTS_PYTORCH_BIN)
        {
            // `torch.save` pickle, read through `candle_core::pickle` — the
            // same reader `sparse_head` already uses for `sparse_linear.pt`.
            // It cannot be mmapped (tensors are pickled, not laid out flat),
            // so this materializes the weights once at load time.
            VarBuilder::from_pth(&weights_path, dtype, &device)
                .map_err(|e| LoadError::Candle(format!("pytorch_model.bin load: {}", e)))?
        } else {
            // SAFETY: we mmap a model file that we own and never mutate after
            // download. Candle's loader is built around mmap; the alternative
            // (`VarBuilder::from_buffered_safetensors`) reads the entire 100MB-1GB
            // file into RAM, which is wasteful for model weights we'll mostly
            // stream into GPU buffers anyway.
            #[allow(unsafe_code)]
            let mmaped = unsafe {
                VarBuilder::from_mmaped_safetensors(&[&weights_path], dtype, &device)
                    .map_err(|e| LoadError::Candle(format!("safetensors load: {}", e)))?
            };
            mmaped
        };

        // Each architecture has a different Candle module + Config struct.
        // `config.json` is the same file though — every loader parses the
        // same source, just keying off different fields. (HiddenAct is
        // referenced from the BertConfig branch and required to satisfy
        // some HF model variants that emit `hidden_act: "gelu_new"`.)
        let _ = HiddenAct::Gelu;
        let model = match architecture {
            ModelArchitecture::Bert => {
                let cfg: BertConfig = serde_json::from_slice(&config_bytes)
                    .map_err(|e| LoadError::BadConfig(e.to_string()))?;
                let m = BertModel::load(var_builder, &cfg)
                    .map_err(|e| LoadError::Candle(format!("BertModel::load: {}", e)))?;
                ModelVariant::Bert(m)
            }
            ModelArchitecture::DistilBert => {
                let cfg: DistilBertConfig = serde_json::from_slice(&config_bytes)
                    .map_err(|e| LoadError::BadConfig(e.to_string()))?;
                let m = DistilBertModel::load(var_builder, &cfg)
                    .map_err(|e| LoadError::Candle(format!("DistilBertModel::load: {}", e)))?;
                ModelVariant::DistilBert(m)
            }
            ModelArchitecture::XlmRoberta => {
                let cfg: XlmRobertaConfig = serde_json::from_slice(&config_bytes)
                    .map_err(|e| LoadError::BadConfig(e.to_string()))?;
                let m = XLMRobertaModel::new(&cfg, var_builder)
                    .map_err(|e| LoadError::Candle(format!("XLMRobertaModel::new: {}", e)))?;
                ModelVariant::XlmRoberta(m)
            }
            ModelArchitecture::JinaBert => {
                let cfg: JinaBertConfig = serde_json::from_slice(&config_bytes)
                    .map_err(|e| LoadError::BadConfig(e.to_string()))?;
                let m = JinaBertModel::new(var_builder, &cfg)
                    .map_err(|e| LoadError::Candle(format!("JinaBertModel::new: {}", e)))?;
                ModelVariant::JinaBert(m)
            }
            ModelArchitecture::NomicBert => {
                let cfg: NomicBertConfig = serde_json::from_slice(&config_bytes)
                    .map_err(|e| LoadError::BadConfig(e.to_string()))?;
                let m = NomicBertModel::load(var_builder, &cfg)
                    .map_err(|e| LoadError::Candle(format!("NomicBertModel::load: {}", e)))?;
                ModelVariant::NomicBert(m)
            }
            ModelArchitecture::ModernBert => {
                let cfg: ModernBertConfig = serde_json::from_slice(&config_bytes)
                    .map_err(|e| LoadError::BadConfig(e.to_string()))?;
                let m = ModernBert::load(var_builder, &cfg)
                    .map_err(|e| LoadError::Candle(format!("ModernBert::load: {}", e)))?;
                ModelVariant::ModernBert(m)
            }
            ModelArchitecture::Qwen3 => {
                let cfg: candle_transformers::models::qwen3::Config =
                    serde_json::from_slice(&config_bytes)
                        .map_err(|e| LoadError::BadConfig(e.to_string()))?;
                let vb = if var_builder.contains_tensor("embed_tokens.weight") {
                    var_builder
                        .rename_f(|name| name.strip_prefix("model.").unwrap_or(name).to_owned())
                } else {
                    var_builder
                };
                ModelVariant::Qwen3(
                    Qwen3Encoder::new(&cfg, vb).map_err(|e| LoadError::Candle(e.to_string()))?,
                )
            }
            ModelArchitecture::Mpnet => unreachable!("rejected above"),
        };

        // BGE-M3 is the one checkpoint we support that ships a learned sparse
        // head, and it is XLM-RoBERTa based. Gating on the architecture *and*
        // the file means an unrelated XLM-R embedder without the head simply
        // reports `supports_sparse() == false`, while a BGE-M3 directory whose
        // head cannot be read fails loudly instead of indexing a corpus with
        // silently empty sparse rows.
        let sparse_head = if architecture == ModelArchitecture::XlmRoberta {
            SparseHead::load(dir, config.hidden_size, &device, &tokenizer)?
        } else {
            None
        };
        if sparse_head.is_some() {
            tracing::info!(
                model_type = %config.model_type,
                "Learned sparse head loaded; sparse retrieval is available for this model"
            );
        }

        Ok(Self {
            model: Arc::new(Mutex::new(model)),
            tokenizer: Arc::new(tokenizer),
            device,
            dimension: config.hidden_size,
            pooling,
            architecture,
            input_policy,
            identity,
            strategy: EmbeddingStrategy::default(),
            sparse_head: sparse_head.map(Arc::new),
            prefixes,
            query_cache: std::sync::Mutex::new(LruCache::new(
                NonZeroUsize::new(QUERY_CACHE_CAPACITY).unwrap_or(NonZeroUsize::MIN),
            )),
        })
    }

    /// Compute the artifact identity of `model_dir`, then [`open`](Self::open)
    /// it. For directories that have no `models` row: env-configured structure
    /// models, eval tooling, tests. Never call this from a launch or
    /// activation path; it streams every weights file through SHA-256.
    pub fn open_unregistered(model_dir: impl AsRef<Path>) -> Result<Self> {
        let dir = model_dir.as_ref();
        let identity = crate::features::embedding::artifact_identity::compute(dir)?;
        Self::open(dir, identity)
    }

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

    /// Specialized embedding for chunker output. Same code path as
    /// `embed_batch` — we just unwrap the contextualized text first.
    pub async fn embed_contextualized_chunks(
        &self,
        chunks: &[crate::features::indexing::engine::chunker::ContextualizedChunk],
    ) -> Result<Vec<Vec<f32>>> {
        if chunks.is_empty() {
            return Ok(vec![]);
        }
        let texts: Vec<String> = chunks
            .iter()
            .map(|c| c.contextualized_content.clone())
            .collect();
        <Self as EmbeddingPort>::embed_batch(self, &texts).await
    }

    /// Embed every chunk of one structure span from a single forward pass —
    /// "late chunking".
    ///
    /// `span_text` is the whole span *including* its leading
    /// "[Document: title | Page | Section]" context prefix, and
    /// `chunk_char_ranges` are the UTF-8 byte ranges of the individual chunks
    /// inside it. The span is tokenized once with offsets and run through the
    /// model once; each chunk vector is the mean of the final hidden states of
    /// the tokens that start inside that chunk's range, L2-normalized. Prefix
    /// tokens and the special tokens ([CLS]/[SEP], BOS/EOS) condition the pass
    /// but are pooled into nothing — the prefix informs every chunk without
    /// polluting any chunk's vector with its own literal text.
    ///
    /// Qwen3 normally pools the *last* token of a sequence, because its causal
    /// attention only lets that position see the whole input. There is no
    /// per-chunk position with that property here, so — like every other
    /// architecture — we mean-pool the chunk's token states. Mean pooling over
    /// the chunk span is the late-chunking convention (Günther et al., 2024);
    /// it produces a different vector space than last-token pooling, which is
    /// exactly why [`CandleEmbeddingService::model_identity`] carries the
    /// `late-chunking-v1` marker whenever this path is active.
    ///
    /// # Errors
    ///
    /// Returns [`LateChunkingError::SpanTooLong`] when the span does not fit
    /// the model window, and [`LateChunkingError::EmptyChunkSpan`] /
    /// [`LateChunkingError::InvalidRange`] when the ranges cannot be pooled.
    /// All three satisfy [`LateChunkingError::allows_fallback`]: the caller
    /// re-embeds that span with the ordinary per-chunk path.
    pub async fn embed_late_chunked(
        &self,
        span_text: &str,
        chunk_char_ranges: &[Range<usize>],
    ) -> std::result::Result<Vec<Vec<f32>>, LateChunkingError> {
        if chunk_char_ranges.is_empty() {
            return Ok(Vec::new());
        }
        validate_chunk_ranges(span_text, chunk_char_ranges)?;

        // Padding would add tokens with meaningless offsets; a span is a single
        // sequence, so there is nothing to pad to.
        let mut tokenizer = Tokenizer::clone(&self.tokenizer);
        tokenizer.with_padding(None);
        let encoding = tokenizer.encode(span_text, true).map_err(|e| {
            LateChunkingError::Embedding(LoadError::Tokenizer(e.to_string()).into())
        })?;
        let token_count = encoding.get_ids().len();
        if token_count == 0 {
            return Err(LateChunkingError::EmptyChunkSpan { chunk: 0 });
        }
        if token_count > self.input_policy.max_tokens {
            return Err(LateChunkingError::SpanTooLong {
                tokens: token_count,
                limit: self.input_policy.max_tokens,
            });
        }

        let groups = pooling_token_indices(
            encoding.get_offsets(),
            encoding.get_special_tokens_mask(),
            chunk_char_ranges,
        )?;

        let hidden = self.span_hidden_states(&encoding).await?;
        let mut vectors = Vec::with_capacity(groups.len());
        for (chunk, group) in groups.iter().enumerate() {
            let mut pooled = mean_pool_rows(&hidden, group)
                .ok_or(LateChunkingError::EmptyChunkSpan { chunk })?;
            l2_normalize_in_place(&mut pooled);
            vectors.push(pooled);
        }
        Ok(vectors)
    }

    /// Late chunk a span of any length, one forward pass per window of whole
    /// chunks.
    ///
    /// A span that fits the model window is a single window and behaves exactly
    /// as [`embed_late_chunked`](Self::embed_late_chunked) always did. A longer
    /// span used to fall back to chunk-first in its entirety, which threw away
    /// the whole point of late chunking; now only the conditioning that would
    /// have crossed a window edge is lost. Each window carries the span's shared
    /// context prefix — the bytes before the first chunk range — so every chunk
    /// stays conditioned on the document's identity.
    async fn embed_late_chunked_windows(
        &self,
        span_text: &str,
        chunk_ranges: &[Range<usize>],
    ) -> std::result::Result<Vec<Vec<f32>>, LateChunkingError> {
        validate_chunk_ranges(span_text, chunk_ranges)?;
        let Some(prefix_end) = chunk_ranges.first().map(|range| range.start) else {
            return Ok(Vec::new());
        };
        match self.embed_late_chunked(span_text, chunk_ranges).await {
            Err(LateChunkingError::SpanTooLong { .. }) => {}
            outcome => return outcome,
        }

        // Windowing reads a window as one contiguous slice, which only holds
        // for ranges in chunk order. Anything else keeps the old whole-span
        // fallback rather than pooling the wrong bytes.
        if chunk_ranges.windows(2).any(|pair| match pair {
            [a, b] => b.start < a.start || b.end < a.end,
            _ => false,
        }) {
            return Err(LateChunkingError::InvalidRange { chunk: 0 });
        }

        let slice = |range: Range<usize>| {
            span_text
                .get(range)
                .ok_or(LateChunkingError::InvalidRange { chunk: 0 })
        };
        let prefix = slice(0..prefix_end)?;
        let count = |text: &str| {
            self.input_policy
                .count(text)
                .map_err(LateChunkingError::Embedding)
        };
        let mut chunk_tokens = Vec::with_capacity(chunk_ranges.len());
        for range in chunk_ranges {
            chunk_tokens.push(count(slice(range.clone())?)?);
        }
        let budget = self.input_policy.max_tokens.saturating_sub(count(prefix)?);

        let mut vectors = Vec::with_capacity(chunk_ranges.len());
        for window in window_groups(&chunk_tokens, budget) {
            // Ranges arrive in chunk order, so the window's text is one
            // contiguous slice behind the shared prefix.
            let Some(group) = chunk_ranges.get(window) else {
                continue;
            };
            let (start, end) = match (group.first(), group.last()) {
                (Some(first), Some(last)) => (first.start, last.end),
                _ => continue,
            };
            let text = format!("{prefix}{}", slice(start..end)?);
            let shifted: Vec<Range<usize>> = group
                .iter()
                .map(|range| prefix_end + (range.start - start)..prefix_end + (range.end - start))
                .collect();
            match self.embed_late_chunked(&text, &shifted).await {
                Ok(pooled) => vectors.extend(pooled),
                // One window that still will not fit answers for itself rather
                // than dropping the whole span back to chunk-first.
                Err(error) if error.allows_fallback() => {
                    tracing::debug!(
                        chunks = shifted.len(),
                        reason = %error,
                        "Late-chunking window is not poolable; embedding its chunks individually"
                    );
                    let texts =
                        span_chunk_texts(&text, &shifted).map_err(LateChunkingError::Embedding)?;
                    vectors.extend(
                        <Self as EmbeddingPort>::embed_batch(self, &texts)
                            .await
                            .map_err(LateChunkingError::Embedding)?,
                    );
                }
                Err(error) => return Err(error),
            }
        }
        Ok(vectors)
    }

    /// Late chunk when the strategy is on and the span is eligible; otherwise
    /// embed each chunk on its own. This is what the port and service traits
    /// expose, so callers never have to handle the fallback themselves.
    async fn embed_span(
        &self,
        span_text: &str,
        chunk_ranges: &[Range<usize>],
    ) -> Result<Vec<Vec<f32>>> {
        // The model's document prefix conditions the span the same way the
        // chunker's context prefix does: it goes in front of everything and is
        // pooled into nothing. Shifting the ranges past it keeps them pointing
        // at the same characters, and keeps them out of the prefix, so the
        // per-chunk fallback below re-adds the prefix per chunk rather than
        // slicing a piece of it into one.
        if !self.prefixes.document.is_empty() && !chunk_ranges.is_empty() {
            let offset = self.prefixes.document.len();
            let shifted: Vec<Range<usize>> = chunk_ranges
                .iter()
                .map(|range| range.start + offset..range.end + offset)
                .collect();
            let prefixed = self.prefixes.document_input(span_text);
            return self.embed_span_inner(&prefixed, &shifted).await;
        }
        self.embed_span_inner(span_text, chunk_ranges).await
    }

    /// [`Self::embed_span`] once the document prefix has been applied.
    async fn embed_span_inner(
        &self,
        span_text: &str,
        chunk_ranges: &[Range<usize>],
    ) -> Result<Vec<Vec<f32>>> {
        if self.strategy.is_late_chunking() && !chunk_ranges.is_empty() {
            match self
                .embed_late_chunked_windows(span_text, chunk_ranges)
                .await
            {
                Ok(vectors) => return Ok(vectors),
                Err(error) if error.allows_fallback() => {
                    tracing::debug!(
                        chunks = chunk_ranges.len(),
                        reason = %error,
                        "Span is not late chunkable; embedding its chunks individually"
                    );
                }
                Err(error) => return Err(error.into()),
            }
        }
        let texts = span_chunk_texts(span_text, chunk_ranges)?;
        <Self as EmbeddingPort>::embed_batch(self, &texts).await
    }

    /// One forward pass over one already-tokenized sequence, returned as
    /// (seq, hidden) rows. Only late chunking needs per-token states.
    async fn span_hidden_states(&self, encoding: &tokenizers::Encoding) -> Result<Vec<Vec<f32>>> {
        let guard = Arc::clone(&self.model).lock_owned().await;
        let device = self.device.clone();
        let encoding = encoding.clone();
        run_inference(move || {
            let ids = encoding.get_ids();
            let length = ids.len();
            let hidden_states = match &*guard {
                // No KV cache to clear, so no clone: the encoder is stateless
                // across passes, exactly as the batch path needs it to be.
                ModelVariant::Qwen3(model) => {
                    let input = Tensor::new(ids, &device)
                        .and_then(|t| t.unsqueeze(0))
                        .map_err(|e| LoadError::Candle(e.to_string()))?;
                    model
                        .forward(&input)
                        .map_err(|e| LoadError::Candle(e.to_string()))?
                }
                model => {
                    let to_i64 =
                        |values: &[u32]| values.iter().map(|&v| v as i64).collect::<Vec<_>>();
                    let tensor = |values: Vec<i64>| {
                        Tensor::from_vec(values, (1, length), &device).map_err(|e| {
                            AppError::EmbeddingFailed {
                                reason: format!("span tensor: {}", e),
                            }
                        })
                    };
                    let input_ids_t = tensor(to_i64(ids))?;
                    let attention_mask_t = tensor(to_i64(encoding.get_attention_mask()))?;
                    let token_type_ids_t = tensor(to_i64(encoding.get_type_ids()))?;
                    run_encoder(model, &input_ids_t, &attention_mask_t, &token_type_ids_t)?
                }
            };
            drop(guard);

            hidden_states
                .squeeze(0)
                .and_then(|t| t.to_dtype(DType::F32))
                .and_then(|t| t.contiguous())
                .and_then(|t| t.to_vec2::<f32>())
                .map_err(|e| AppError::EmbeddingFailed {
                    reason: format!("hidden states → rows: {}", e),
                })
        })
        .await
    }

    /// The padded-token ceiling for one forward pass on this architecture.
    fn padded_token_budget(&self) -> usize {
        match self.architecture {
            ModelArchitecture::Qwen3 => QWEN3_PADDED_TOKEN_BUDGET,
            _ => ENCODER_PADDED_TOKEN_BUDGET,
        }
    }

    /// Group `texts` into batches of similar length, returning each batch as
    /// indices into `texts`.
    ///
    /// Padding is what a batch actually costs: every row is padded out to the
    /// longest row in it, so a 12-token query batched with a 2000-token passage
    /// pays for 2000 tokens. Sorting by length first puts similar rows
    /// together, and budgeting by `rows × longest row` rather than by a fixed
    /// row count means a batch of long passages cannot blow up memory while a
    /// batch of short ones is free to be much larger than 16.
    ///
    /// This is also the single place inputs are validated against the model
    /// window; the forward pass takes its texts as already checked.
    fn plan_batches(&self, texts: &[String]) -> Result<Vec<Vec<usize>>> {
        let mut by_length: Vec<(usize, usize)> = Vec::with_capacity(texts.len());
        for (index, text) in texts.iter().enumerate() {
            let tokens = self.input_policy.count(text)?;
            if tokens > self.input_policy.max_tokens {
                // `count` already tokenized; `validate` is here only to raise
                // the one over-long-input message the splitter also uses.
                self.input_policy.validate(text)?;
            }
            by_length.push((tokens, index));
        }
        by_length.sort_unstable();

        let budget = self.padded_token_budget();
        let mut batches: Vec<Vec<usize>> = Vec::new();
        let mut current: Vec<usize> = Vec::new();
        for (tokens, index) in by_length {
            // Ascending order, so `tokens` is this batch's longest row once the
            // row joins it.
            let full = current.len() >= MAX_BATCH_ROWS
                || tokens.saturating_mul(current.len() + 1) > budget;
            if full && !current.is_empty() {
                batches.push(std::mem::take(&mut current));
            }
            current.push(index);
        }
        if !current.is_empty() {
            batches.push(current);
        }
        Ok(batches)
    }

    /// Embed `texts` in length-bucketed batches and hand the vectors back in
    /// the caller's order.
    ///
    /// Each batch takes and releases the model lock on its own, so a query
    /// embedding can slip between two indexing batches instead of waiting out
    /// the whole run.
    async fn forward_batched(
        &self,
        texts: Vec<String>,
        sparse_terms: Option<usize>,
    ) -> Result<(Vec<Vec<f32>>, Vec<SparseEmbedding>)> {
        if texts.is_empty() {
            return Ok((vec![], vec![]));
        }
        let plan = self.plan_batches(&texts)?;
        let mut dense = vec![Vec::new(); texts.len()];
        let mut sparse = vec![SparseEmbedding::empty(); texts.len()];
        // Each text is handed to exactly one batch, so moving it out of the
        // pending list is both the cheapest way to build the batch and the
        // check that the plan covered every input exactly once.
        let mut pending: Vec<Option<String>> = texts.into_iter().map(Some).collect();
        for batch in plan {
            let inputs: Vec<String> = batch
                .iter()
                .filter_map(|&index| pending.get_mut(index).and_then(Option::take))
                .collect();
            if inputs.len() != batch.len() {
                return Err(AppError::InvalidState(
                    "Embedding batch plan referenced an input twice".into(),
                ));
            }
            let (batch_dense, batch_sparse) =
                self.forward_with_sparse(inputs, sparse_terms).await?;
            if batch_dense.len() != batch.len() {
                return Err(AppError::EmbeddingFailed {
                    reason: format!(
                        "forward pass returned {} vectors for {} inputs",
                        batch_dense.len(),
                        batch.len()
                    ),
                });
            }
            for (&index, vector) in batch.iter().zip(batch_dense) {
                if let Some(slot) = dense.get_mut(index) {
                    *slot = vector;
                }
            }
            for (&index, terms) in batch.iter().zip(batch_sparse) {
                if let Some(slot) = sparse.get_mut(index) {
                    *slot = terms;
                }
            }
        }
        Ok((dense, sparse))
    }

    /// Embed one string that already carries whichever prefix its role calls
    /// for. Every public entry point funnels through here once it has decided
    /// between the query and the document prefix.
    async fn embed_prepared(&self, text: String) -> Result<Vec<f32>> {
        let mut out = self.forward_batched(vec![text], None).await?.0;
        out.pop().ok_or_else(|| AppError::EmbeddingFailed {
            reason: "Forward pass returned no embeddings".into(),
        })
    }

    /// A previously embedded query, if this service has seen it.
    fn cached_query(&self, prefixed: &str) -> Option<Arc<Vec<f32>>> {
        let mut cache = self
            .query_cache
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        cache.get(prefixed).map(Arc::clone)
    }

    fn cache_query(&self, prefixed: String, vector: Arc<Vec<f32>>) {
        let mut cache = self
            .query_cache
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        cache.put(prefixed, vector);
    }

    /// The dense forward pass, optionally also reading the learned sparse head
    /// off the *same* hidden states.
    ///
    /// `sparse_terms` is the per-passage term budget; `None` skips the sparse
    /// head entirely. When it is `Some`, the encoder runs exactly once and
    /// both representations come out of one set of hidden states — the whole
    /// reason `embed_batch_with_sparse` exists rather than two calls.
    ///
    /// The returned sparse vector is empty (never absent) for a passage whose
    /// every token was special or scored zero, so the two returned vectors are
    /// always the same length and stay aligned with `texts`.
    ///
    /// `texts` is one padded batch and must already fit the model window;
    /// [`Self::plan_batches`] is what decides both.
    async fn forward_with_sparse(
        &self,
        texts: Vec<String>,
        sparse_terms: Option<usize>,
    ) -> Result<(Vec<Vec<f32>>, Vec<SparseEmbedding>)> {
        if texts.is_empty() {
            return Ok((vec![], vec![]));
        }
        let sparse_head = match sparse_terms {
            Some(_) => Some(
                self.sparse_head
                    .as_ref()
                    .ok_or_else(|| sparse_not_supported(self.identity.as_str()))?,
            ),
            None => None,
        };

        let guard = Arc::clone(&self.model).lock_owned().await;
        let tokenizer = Arc::clone(&self.tokenizer);
        let device = self.device.clone();
        let pooling = self.pooling;
        let sparse_head = sparse_head.cloned();
        run_inference(move || match &*guard {
            ModelVariant::Qwen3(template) => {
                let dense = qwen3_embed(template, &tokenizer, &device, &texts)?;
                let sparse = vec![SparseEmbedding::empty(); dense.len()];
                Ok((dense, sparse))
            }
            model => encoder_embed(
                model,
                &tokenizer,
                &device,
                pooling,
                sparse_head.as_deref(),
                sparse_terms,
                &texts,
            ),
        })
        .await
    }

    /// Dense vectors and learned sparse term weights from a single forward
    /// pass per batch.
    ///
    /// This is the indexing path for a model with a sparse head: running
    /// `embed_batch` and `embed_sparse_batch` separately would double the
    /// encoder work for identical hidden states.
    ///
    /// # Errors
    ///
    /// Returns the "not supported" error when this model has no sparse head.
    /// Check [`EmbeddingPort::supports_sparse`] first.
    pub async fn embed_batch_with_sparse(
        &self,
        texts: &[String],
    ) -> Result<(Vec<Vec<f32>>, Vec<SparseEmbedding>)> {
        if self.sparse_head.is_none() {
            return Err(sparse_not_supported(self.identity.as_str()));
        }
        self.forward_batched(self.as_documents(texts), Some(MAX_PASSAGE_TERMS))
            .await
    }

    /// `texts` with the model's document prefix in front of each, which is
    /// what a passage is actually embedded as.
    fn as_documents(&self, texts: &[String]) -> Vec<String> {
        texts
            .iter()
            .map(|text| self.prefixes.document_input(text).into_owned())
            .collect()
    }
}

#[async_trait]
impl EmbeddingPort for CandleEmbeddingService {
    async fn embed_single(&self, text: &str) -> Result<Vec<f32>> {
        if text.is_empty() {
            return Err(AppError::InvalidInput("Cannot embed empty text".into()));
        }
        self.embed_prepared(self.prefixes.document_input(text).into_owned())
            .await
    }

    async fn embed_query(&self, text: &str) -> Result<Vec<f32>> {
        let query = self.prefixes.query_input(text).into_owned();
        if let Some(cached) = self.cached_query(&query) {
            return Ok(Vec::clone(&cached));
        }
        let vector = Arc::new(self.embed_prepared(query.clone()).await?);
        self.cache_query(query, Arc::clone(&vector));
        Ok(Vec::clone(&vector))
    }

    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        Ok(self
            .forward_batched(self.as_documents(texts), None)
            .await?
            .0)
    }

    fn split_text(
        &self,
        text: &str,
        prefix: &str,
    ) -> Result<Vec<crate::application::ports::embedding_port::EmbeddingTextChunk>> {
        // The model's document prefix is part of every passage the runtime
        // embeds, so it has to be part of what the splitter measures against
        // the window — otherwise a chunk sized to exactly fit overflows by the
        // prefix's tokens at embedding time. It goes outside the chunker's
        // context prefix, matching the order `embed_batch` builds.
        if self.prefixes.document.is_empty() {
            return self.input_policy.split(text, prefix);
        }
        self.input_policy
            .split(text, &format!("{}{prefix}", self.prefixes.document))
    }

    fn uses_late_chunking(&self) -> bool {
        self.strategy.is_late_chunking()
    }

    async fn embed_span_chunks(
        &self,
        span_text: &str,
        chunk_ranges: &[Range<usize>],
    ) -> Result<Vec<Vec<f32>>> {
        self.embed_span(span_text, chunk_ranges).await
    }

    fn supports_sparse(&self) -> bool {
        self.sparse_head.is_some()
    }

    async fn embed_sparse_batch(&self, texts: &[String]) -> Result<Vec<SparseEmbedding>> {
        Ok(self.embed_batch_with_sparse(texts).await?.1)
    }

    async fn embed_batch_with_sparse(
        &self,
        texts: &[String],
    ) -> Result<(Vec<Vec<f32>>, Vec<SparseEmbedding>)> {
        CandleEmbeddingService::embed_batch_with_sparse(self, texts).await
    }

    async fn embed_sparse_query(&self, text: &str) -> Result<SparseEmbedding> {
        if self.sparse_head.is_none() {
            return Err(sparse_not_supported(self.identity.as_str()));
        }
        // A query is pruned much harder than a passage: every surviving term
        // becomes a row in the scoring join.
        let (_, mut sparse) = self
            .forward_batched(
                vec![self.prefixes.query_input(text).into_owned()],
                Some(MAX_QUERY_TERMS),
            )
            .await?;
        Ok(sparse.pop().unwrap_or_default())
    }

    fn model_identity(&self) -> String {
        let identity = strategy_identity(self.identity.as_str(), self.strategy);
        // A document prefix goes into every stored vector, so it belongs in the
        // key that decides whether those vectors are still the live generation.
        // A policy that only prefixes queries leaves stored vectors alone and
        // so leaves the identity alone — which is why Qwen3, whose instruction
        // is query-side only, keeps the identity it already had.
        if self.prefixes.document.is_empty() {
            identity
        } else {
            format!("{identity}+prefix-{}", self.prefixes.id)
        }
    }

    fn dimension(&self) -> usize {
        self.dimension
    }

    async fn is_ready(&self) -> Result<bool> {
        Ok(true)
    }
}

#[async_trait]
impl EmbeddingServiceTrait for CandleEmbeddingService {
    fn model_identity(&self) -> String {
        <Self as EmbeddingPort>::model_identity(self)
    }
    fn split_text(
        &self,
        text: &str,
        prefix: &str,
    ) -> Result<Vec<crate::application::ports::embedding_port::EmbeddingTextChunk>> {
        <Self as EmbeddingPort>::split_text(self, text, prefix)
    }

    async fn embed_query(&self, text: &str) -> Result<Vec<f32>> {
        <Self as EmbeddingPort>::embed_query(self, text).await
    }

    async fn embed_single(&self, text: &str) -> Result<Vec<f32>> {
        <Self as EmbeddingPort>::embed_single(self, text).await
    }

    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        <Self as EmbeddingPort>::embed_batch(self, texts).await
    }

    async fn embed_contextualized_chunks(
        &self,
        chunks: &[crate::features::indexing::engine::chunker::ContextualizedChunk],
    ) -> Result<Vec<Vec<f32>>> {
        self.embed_contextualized_chunks(chunks).await
    }

    fn uses_late_chunking(&self) -> bool {
        self.strategy.is_late_chunking()
    }

    async fn embed_span_chunks(
        &self,
        span_text: &str,
        chunk_ranges: &[Range<usize>],
    ) -> Result<Vec<Vec<f32>>> {
        self.embed_span(span_text, chunk_ranges).await
    }

    fn supports_sparse(&self) -> bool {
        <Self as EmbeddingPort>::supports_sparse(self)
    }

    async fn embed_sparse_batch(&self, texts: &[String]) -> Result<Vec<SparseEmbedding>> {
        <Self as EmbeddingPort>::embed_sparse_batch(self, texts).await
    }

    async fn embed_batch_with_sparse(
        &self,
        texts: &[String],
    ) -> Result<(Vec<Vec<f32>>, Vec<SparseEmbedding>)> {
        CandleEmbeddingService::embed_batch_with_sparse(self, texts).await
    }

    async fn embed_sparse_query(&self, text: &str) -> Result<SparseEmbedding> {
        <Self as EmbeddingPort>::embed_sparse_query(self, text).await
    }
}

/// Read pooling strategy. Tries in order:
/// 1. `1_Pooling/config.json` (sentence-transformers explicit config)
/// 2. `config.json` for a sentence-transformers heuristic (`_name_or_path`
///    pointing at a known mean-pooled architecture)
/// 3. CLS default — what BGE/mxbai/UAE use.
fn read_pooling_strategy(model_dir: &Path) -> PoolingStrategy {
    let pooling_path = model_dir.join("1_Pooling").join("config.json");
    if let Ok(bytes) = std::fs::read(&pooling_path) {
        if let Ok(cfg) = serde_json::from_slice::<PoolingConfig>(&bytes) {
            if cfg.pooling_mode_cls_token {
                return PoolingStrategy::Cls;
            }
            if cfg.pooling_mode_mean_tokens {
                return PoolingStrategy::Mean;
            }
        }
    }

    let main_config = model_dir.join("config.json");
    if let Ok(bytes) = std::fs::read(&main_config) {
        if let Ok(cfg) = serde_json::from_slice::<serde_json::Value>(&bytes) {
            let name = cfg
                .get("_name_or_path")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            if name.contains("sentence-transformers")
                || name.contains("all-minilm")
                || name == "nreimers/minilm-l6-h384-uncased"
                || name.contains("all-mpnet")
                || name.contains("paraphrase-")
                || name.contains("multi-qa-")
            {
                tracing::info!(
                    name = %name,
                    "Detected sentence-transformers model from config.json; defaulting to mean pooling"
                );
                return PoolingStrategy::Mean;
            }
        }
    }

    PoolingStrategy::Cls
}

/// The dtype to load weights in.
///
/// Half precision only pays off where the model is large enough to be memory
/// bound and the backend has half-precision kernels, which today means Qwen3 on
/// a GPU: it halves the 1.2 GB of resident weights and measured about 7% faster
/// on Metal. Its vectors match the F32 ones to a worst-case cosine of 0.999994
/// over the v2 fixture's queries with no NaN or Inf
/// (`qwen3_live_f16_matches_f32`), and pooling casts back to F32 before
/// normalizing, where a sum of 1024 squared hidden states would otherwise
/// overflow the f16 range.
///
/// Everything else stays F32. The BERT-family models are small enough that the
/// dtype barely moves their throughput, and CPU has no half-precision kernels
/// worth the conversion.
fn weights_dtype(architecture: ModelArchitecture, device: &Device) -> DType {
    match architecture {
        ModelArchitecture::Qwen3 if !device.is_cpu() => DType::F16,
        _ => DType::F32,
    }
}

/// The instruction prefixes this checkpoint was trained with.
///
/// Resolution order is `config.json::_name_or_path`, then the directory the
/// weights sit in, then what the architecture implies. Matching the recorded
/// name first keeps the answer stable when a model directory is moved, which
/// matters because a document prefix is part of the vector identity.
fn resolve_prefixes(
    dir: &Path,
    config_bytes: &[u8],
    architecture: ModelArchitecture,
) -> &'static EmbeddingPrefixes {
    let recorded = serde_json::from_slice::<serde_json::Value>(config_bytes)
        .ok()
        .and_then(|config| {
            config
                .get("_name_or_path")
                .and_then(|value| value.as_str())
                .map(str::to_owned)
        });

    recorded
        .as_deref()
        .and_then(super::prefixes::prefixes_for)
        .or_else(|| {
            dir.file_name()
                .and_then(|name| name.to_str())
                .and_then(super::prefixes::prefixes_for)
        })
        .unwrap_or(match architecture {
            // Every Qwen3 checkpoint this runtime accepts is an embedding
            // model, and the whole Qwen3-Embedding line is instruction-tuned.
            ModelArchitecture::Qwen3 => &super::prefixes::QWEN3_INSTRUCT,
            _ => &super::prefixes::NONE,
        })
}

/// Run the encoder for one padded batch, returning (batch, seq, hidden).
/// Shared by the pooled batch path and by late chunking, which needs the
/// per-token states rather than a pooled vector.
/// Run one inference step off the async runtime.
///
/// Metal work blocks for as long as the GPU takes, so it belongs on the
/// blocking pool rather than a runtime worker. It also autoreleases
/// Objective-C objects on every dispatch, which the step's own autorelease
/// pool releases on return; without it they stayed parked on the worker
/// thread for the life of the process (see `shared::utils::autorelease`).
async fn run_inference<T, F>(f: F) -> Result<T>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T> + Send + 'static,
{
    tokio::task::spawn_blocking(move || with_autorelease_pool(f))
        .await
        .map_err(|e| AppError::EmbeddingFailed {
            reason: format!("inference task: {}", e),
        })?
}

/// Last-token embeddings for one right-padded batch, from a single pass.
///
/// Right padding is what makes a batch exact rather than merely close. Qwen3 is
/// causal: position `i` attends to `0..=i` and never to the pad tokens sitting
/// to its right, so the hidden state at each row's last real token — the one
/// last-token pooling reads — is the state that row would have produced alone.
/// The padded rows do compute values past their own end; nothing reads them.
fn qwen3_embed(
    model: &Qwen3Encoder,
    tokenizer: &Tokenizer,
    device: &Device,
    texts: &[String],
) -> Result<Vec<Vec<f32>>> {
    let encodings =
        tokenizer
            .encode_batch(texts.to_vec(), true)
            .map_err(|e| AppError::EmbeddingFailed {
                reason: format!("tokenization: {}", e),
            })?;

    // The attention mask, not the id count, is each row's real length: the ids
    // now run out to the longest row in the batch.
    let lengths: Vec<usize> = encodings
        .iter()
        .map(|encoding| {
            encoding
                .get_attention_mask()
                .iter()
                .filter(|&&flag| flag == 1)
                .count()
        })
        .collect();
    if lengths.contains(&0) {
        return Err(AppError::InvalidInput(
            "Cannot embed an empty token sequence".into(),
        ));
    }

    let batch_size = encodings.len();
    let max_len = encodings
        .iter()
        .map(|encoding| encoding.get_ids().len())
        .max()
        .unwrap_or(0);
    let mut input_ids = Vec::with_capacity(batch_size * max_len);
    for encoding in &encodings {
        input_ids.extend_from_slice(encoding.get_ids());
    }

    let input = Tensor::from_vec(input_ids, (batch_size, max_len), device).map_err(|e| {
        AppError::EmbeddingFailed {
            reason: format!("input_ids tensor: {}", e),
        }
    })?;
    let hidden = model
        .forward(&input)
        .map_err(|e| LoadError::Candle(format!("Qwen3 forward: {}", e)))?;

    let last_tokens = lengths
        .iter()
        .enumerate()
        .map(|(row, &length)| hidden.i((row, length - 1, ..)))
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|e| LoadError::Candle(format!("last-token pooling: {}", e)))?;
    // Back to F32 before the norm: a sum of 1024 squared hidden states
    // overflows f16 long before it overflows anything else.
    let pooled = Tensor::stack(&last_tokens, 0)
        .and_then(|t| t.to_dtype(DType::F32))
        .map_err(|e| LoadError::Candle(format!("pooling: {}", e)))?;
    let normalized = l2_normalize(&pooled).map_err(|e| LoadError::Candle(e.to_string()))?;
    tensor_to_vec_of_vec(&normalized).map_err(|e| LoadError::Candle(e.to_string()).into())
}

/// One padded batch through an encoder model: dense vectors, plus the sparse
/// head read off the same hidden states when `sparse_head` and
/// `sparse_terms` are both given.
fn encoder_embed(
    model: &ModelVariant,
    tokenizer: &Tokenizer,
    device: &Device,
    pooling: PoolingStrategy,
    sparse_head: Option<&SparseHead>,
    sparse_terms: Option<usize>,
    texts: &[String],
) -> Result<(Vec<Vec<f32>>, Vec<SparseEmbedding>)> {
    let encodings =
        tokenizer
            .encode_batch(texts.to_vec(), true)
            .map_err(|e| AppError::EmbeddingFailed {
                reason: format!("tokenization: {}", e),
            })?;

    let batch_size = encodings.len();
    let max_len = encodings
        .iter()
        .map(|e| e.get_ids().len())
        .max()
        .unwrap_or(0);

    let mut input_ids = Vec::with_capacity(batch_size * max_len);
    let mut attention_mask = Vec::with_capacity(batch_size * max_len);
    let mut token_type_ids = Vec::with_capacity(batch_size * max_len);
    for enc in &encodings {
        input_ids.extend(enc.get_ids().iter().map(|&x| x as i64));
        attention_mask.extend(enc.get_attention_mask().iter().map(|&x| x as i64));
        token_type_ids.extend(enc.get_type_ids().iter().map(|&x| x as i64));
    }

    let input_ids_t = Tensor::from_vec(input_ids, (batch_size, max_len), device).map_err(|e| {
        AppError::EmbeddingFailed {
            reason: format!("input_ids tensor: {}", e),
        }
    })?;
    let attention_mask_t = Tensor::from_vec(attention_mask, (batch_size, max_len), device)
        .map_err(|e| AppError::EmbeddingFailed {
            reason: format!("attention_mask tensor: {}", e),
        })?;
    let token_type_ids_t = Tensor::from_vec(token_type_ids, (batch_size, max_len), device)
        .map_err(|e| AppError::EmbeddingFailed {
            reason: format!("token_type_ids tensor: {}", e),
        })?;

    let hidden_states = run_encoder(model, &input_ids_t, &attention_mask_t, &token_type_ids_t)?;

    // Read the sparse head off the hidden states we already have. Padding
    // is dropped by trimming each row back to its own token count before
    // aggregation, so a short passage in a long batch cannot pick up terms
    // from the pad token.
    let sparse_vectors = match (sparse_head, sparse_terms) {
        (Some(head), Some(max_terms)) => {
            let rows = head.token_weights(&hidden_states)?;
            encodings
                .iter()
                .zip(rows.iter())
                .map(|(encoding, row)| {
                    let ids = encoding.get_ids();
                    let usable = row.len().min(ids.len());
                    head.aggregate(
                        ids.get(..usable).unwrap_or_default(),
                        row.get(..usable).unwrap_or_default(),
                        encoding.get_special_tokens_mask(),
                        max_terms,
                    )
                })
                .collect()
        }
        _ => Vec::new(),
    };

    let pooled = match pooling {
        PoolingStrategy::Cls => cls_pool(&hidden_states),
        PoolingStrategy::Mean => mean_pool(&hidden_states, &attention_mask_t),
    }
    .map_err(|e| AppError::EmbeddingFailed {
        reason: format!("pooling: {}", e),
    })?;

    let normalized = l2_normalize(&pooled).map_err(|e| AppError::EmbeddingFailed {
        reason: format!("L2 normalize: {}", e),
    })?;

    let vectors = tensor_to_vec_of_vec(&normalized).map_err(|e| AppError::EmbeddingFailed {
        reason: format!("tensor → Vec<Vec<f32>>: {}", e),
    })?;

    Ok((vectors, sparse_vectors))
}

fn run_encoder(
    model: &ModelVariant,
    input_ids: &Tensor,
    attention_mask: &Tensor,
    token_type_ids: &Tensor,
) -> Result<Tensor> {
    let hidden_states = match model {
        ModelVariant::Bert(model) => model
            .forward(input_ids, token_type_ids, Some(attention_mask))
            .map_err(|e| AppError::EmbeddingFailed {
                reason: format!("BertModel forward: {}", e),
            })?,
        ModelVariant::DistilBert(model) => {
            model
                .forward(input_ids, attention_mask)
                .map_err(|e| AppError::EmbeddingFailed {
                    reason: format!("DistilBertModel forward: {}", e),
                })?
        }
        ModelVariant::XlmRoberta(model) => model
            .forward(input_ids, attention_mask, token_type_ids, None, None, None)
            .map_err(|e| AppError::EmbeddingFailed {
                reason: format!("XLMRobertaModel forward: {}", e),
            })?,
        ModelVariant::JinaBert(model) => {
            model
                .forward(input_ids)
                .map_err(|e| AppError::EmbeddingFailed {
                    reason: format!("JinaBertModel forward: {}", e),
                })?
        }
        ModelVariant::NomicBert(model) => model
            .forward(input_ids, Some(token_type_ids), Some(attention_mask))
            .map_err(|e| AppError::EmbeddingFailed {
                reason: format!("NomicBertModel forward: {}", e),
            })?,
        ModelVariant::ModernBert(model) => {
            model
                .forward(input_ids, attention_mask)
                .map_err(|e| AppError::EmbeddingFailed {
                    reason: format!("ModernBert forward: {}", e),
                })?
        }
        ModelVariant::Qwen3(_) => unreachable!("Qwen runs its own causal forward pass"),
    };
    Ok(hidden_states)
}

/// CLS pooling: take token at index 0 from each row of (batch, seq, hidden).
fn cls_pool(hidden_states: &Tensor) -> std::result::Result<Tensor, candle_core::Error> {
    hidden_states.i((.., 0, ..))
}

/// Mean pooling: sum hidden states over the seq dim, weighted by attention mask,
/// then divide by the per-row mask sum. Matches sentence-transformers reference.
fn mean_pool(
    hidden_states: &Tensor,
    attention_mask: &Tensor,
) -> std::result::Result<Tensor, candle_core::Error> {
    // hidden_states: (B, S, H), attention_mask: (B, S) i64
    let mask = attention_mask.to_dtype(DType::F32)?.unsqueeze(2)?; // (B, S, 1)
    let masked = hidden_states.broadcast_mul(&mask)?;
    let summed = masked.sum(1)?; // (B, H)
    let counts = mask.sum(1)?.clamp(1f32, f32::INFINITY)?; // (B, 1) avoid div-by-zero
    summed.broadcast_div(&counts)
}

/// L2-normalize each row of a (batch, hidden) tensor.
fn l2_normalize(t: &Tensor) -> std::result::Result<Tensor, candle_core::Error> {
    let norm = t.sqr()?.sum_keepdim(1)?.sqrt()?;
    t.broadcast_div(&norm)
}

/// Convert a (batch, hidden) f32 tensor into the Vec<Vec<f32>> shape callers
/// expect.
fn tensor_to_vec_of_vec(t: &Tensor) -> std::result::Result<Vec<Vec<f32>>, candle_core::Error> {
    let dims = t.dims();
    let (batch, hidden) = match dims {
        [b, h] => (*b, *h),
        other => {
            return Err(candle_core::Error::Msg(format!(
                "expected (batch, hidden) tensor, got {:?}",
                other
            )))
        }
    };
    let flat = t.to_dtype(DType::F32)?.flatten_all()?.to_vec1::<f32>()?;
    let expected_len = batch.checked_mul(hidden).ok_or_else(|| {
        candle_core::Error::Msg("embedding tensor dimensions overflowed usize".to_string())
    })?;
    if hidden == 0 || flat.len() != expected_len {
        return Err(candle_core::Error::Msg(format!(
            "embedding tensor data length {} does not match shape ({batch}, {hidden})",
            flat.len()
        )));
    }
    Ok(flat
        .chunks_exact(hidden)
        .take(batch)
        .map(<[f32]>::to_vec)
        .collect())
}

// Tensor indexing helper trait (candle uses an extension-trait pattern for `i`).
use candle_core::IndexOp;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::embedding::artifact_identity;
    use tempfile::TempDir;

    /// A one-layer, four-wide BERT with zero weights: enough for the loader
    /// to accept, never run.
    fn write_tiny_bert(dir: &Path) {
        std::fs::write(
            dir.join("config.json"),
            r#"{"model_type":"bert","vocab_size":2,"hidden_size":4,"num_hidden_layers":1,
                "num_attention_heads":1,"intermediate_size":8,"hidden_act":"gelu",
                "hidden_dropout_prob":0.0,"max_position_embeddings":8,"type_vocab_size":1,
                "initializer_range":0.02,"layer_norm_eps":1e-12,"pad_token_id":0}"#,
        )
        .unwrap();
        let vocab = [("[PAD]".to_owned(), 0), ("[UNK]".to_owned(), 1)]
            .into_iter()
            .collect();
        Tokenizer::new(
            tokenizers::models::wordlevel::WordLevel::builder()
                .vocab(vocab)
                .unk_token("[UNK]".into())
                .build()
                .unwrap(),
        )
        .save(dir.join("tokenizer.json"), false)
        .unwrap();

        let layer = "encoder.layer.0";
        let shapes: Vec<(String, Vec<usize>)> = vec![
            ("embeddings.word_embeddings.weight".into(), vec![2, 4]),
            ("embeddings.position_embeddings.weight".into(), vec![8, 4]),
            ("embeddings.token_type_embeddings.weight".into(), vec![1, 4]),
            ("embeddings.LayerNorm.weight".into(), vec![4]),
            ("embeddings.LayerNorm.bias".into(), vec![4]),
            (format!("{layer}.attention.self.query.weight"), vec![4, 4]),
            (format!("{layer}.attention.self.query.bias"), vec![4]),
            (format!("{layer}.attention.self.key.weight"), vec![4, 4]),
            (format!("{layer}.attention.self.key.bias"), vec![4]),
            (format!("{layer}.attention.self.value.weight"), vec![4, 4]),
            (format!("{layer}.attention.self.value.bias"), vec![4]),
            (format!("{layer}.attention.output.dense.weight"), vec![4, 4]),
            (format!("{layer}.attention.output.dense.bias"), vec![4]),
            (
                format!("{layer}.attention.output.LayerNorm.weight"),
                vec![4],
            ),
            (format!("{layer}.attention.output.LayerNorm.bias"), vec![4]),
            (format!("{layer}.intermediate.dense.weight"), vec![8, 4]),
            (format!("{layer}.intermediate.dense.bias"), vec![8]),
            (format!("{layer}.output.dense.weight"), vec![4, 8]),
            (format!("{layer}.output.dense.bias"), vec![4]),
            (format!("{layer}.output.LayerNorm.weight"), vec![4]),
            (format!("{layer}.output.LayerNorm.bias"), vec![4]),
        ];
        let tensors: std::collections::HashMap<String, Tensor> = shapes
            .into_iter()
            .map(|(name, shape)| {
                let tensor = Tensor::zeros(shape, DType::F32, &Device::Cpu).unwrap();
                (name, tensor)
            })
            .collect();
        candle_core::safetensors::save(&tensors, dir.join(WEIGHTS_SAFETENSORS)).unwrap();
    }

    #[test]
    fn open_labels_the_service_with_the_identity_it_was_given() {
        let dir = TempDir::new().unwrap();
        write_tiny_bert(dir.path());
        // Deliberately not the digest of these files: `open` takes the stored
        // identity as given and never rehashes.
        let stored = ArtifactIdentity::from_digest(&[7; 32]);
        assert_ne!(stored, artifact_identity::compute(dir.path()).unwrap());

        let service = CandleEmbeddingService::open(dir.path(), stored.clone()).unwrap();
        assert_eq!(service.embedding_strategy(), EmbeddingStrategy::ChunkFirst);
        assert_eq!(EmbeddingPort::model_identity(&service), stored.as_str());

        let late = service.with_strategy(EmbeddingStrategy::LateChunking);
        assert_eq!(
            EmbeddingPort::model_identity(&late),
            strategy_identity(stored.as_str(), EmbeddingStrategy::LateChunking)
        );
    }

    #[test]
    fn open_unregistered_labels_the_service_with_the_computed_identity() {
        let dir = TempDir::new().unwrap();
        write_tiny_bert(dir.path());
        let service = CandleEmbeddingService::open_unregistered(dir.path()).unwrap();
        assert_eq!(
            EmbeddingPort::model_identity(&service),
            artifact_identity::compute(dir.path()).unwrap().as_str()
        );
    }

    #[test]
    fn pooling_defaults_to_cls_when_no_hints() {
        let dir = TempDir::new().unwrap();
        std::fs::write(
            dir.path().join("config.json"),
            r#"{"model_type":"bert","hidden_size":768}"#,
        )
        .unwrap();
        assert_eq!(read_pooling_strategy(dir.path()), PoolingStrategy::Cls);
    }

    #[test]
    fn pooling_reads_explicit_1_pooling_config() {
        let dir = TempDir::new().unwrap();
        let pooling_dir = dir.path().join("1_Pooling");
        std::fs::create_dir(&pooling_dir).unwrap();
        std::fs::write(
            pooling_dir.join("config.json"),
            r#"{"pooling_mode_mean_tokens":true}"#,
        )
        .unwrap();
        std::fs::write(
            dir.path().join("config.json"),
            r#"{"model_type":"bert","hidden_size":384}"#,
        )
        .unwrap();
        assert_eq!(read_pooling_strategy(dir.path()), PoolingStrategy::Mean);
    }

    #[test]
    fn pooling_infers_mean_for_sentence_transformers() {
        let dir = TempDir::new().unwrap();
        std::fs::write(
            dir.path().join("config.json"),
            r#"{"model_type":"bert","hidden_size":384,"_name_or_path":"sentence-transformers/all-MiniLM-L6-v2"}"#,
        )
        .unwrap();
        assert_eq!(read_pooling_strategy(dir.path()), PoolingStrategy::Mean);
    }

    #[test]
    fn pooling_keeps_cls_for_bge() {
        let dir = TempDir::new().unwrap();
        std::fs::write(
            dir.path().join("config.json"),
            r#"{"model_type":"bert","hidden_size":1024,"_name_or_path":"BAAI/bge-m3"}"#,
        )
        .unwrap();
        assert_eq!(read_pooling_strategy(dir.path()), PoolingStrategy::Cls);
    }

    #[test]
    fn weights_path_prefers_safetensors_and_accepts_the_pickle() {
        let dir = TempDir::new().unwrap();
        assert!(weights_path(dir.path()).is_none());
        assert!(!has_loadable_weights(dir.path()));

        std::fs::write(dir.path().join(WEIGHTS_PYTORCH_BIN), b"pickle").unwrap();
        assert_eq!(
            weights_path(dir.path()),
            Some(dir.path().join(WEIGHTS_PYTORCH_BIN))
        );
        assert!(has_loadable_weights(dir.path()));

        std::fs::write(dir.path().join(WEIGHTS_SAFETENSORS), b"tensors").unwrap();
        assert_eq!(
            weights_path(dir.path()),
            Some(dir.path().join(WEIGHTS_SAFETENSORS)),
            "safetensors wins when both are present"
        );
    }

    #[test]
    fn missing_weights_error_names_both_candidates() {
        let dir = TempDir::new().unwrap();
        std::fs::write(
            dir.path().join("config.json"),
            r#"{"model_type":"bert","hidden_size":8}"#,
        )
        .unwrap();
        std::fs::write(dir.path().join("tokenizer.json"), "{}").unwrap();

        let identity = ArtifactIdentity::from_digest(&[0; 32]);
        let message = match CandleEmbeddingService::open(dir.path(), identity) {
            Ok(_) => panic!("a directory with no weights must not load"),
            Err(error) => error.to_string(),
        };
        assert!(message.contains("model.safetensors"), "{message}");
        assert!(message.contains("pytorch_model.bin"), "{message}");
        assert!(
            message.contains(&dir.path().display().to_string()),
            "{message}"
        );
    }

    #[test]
    fn length_buckets_stay_inside_the_padded_token_budget() {
        let dir = TempDir::new().unwrap();
        write_tiny_bert(dir.path());
        let service =
            CandleEmbeddingService::open(dir.path(), ArtifactIdentity::from_digest(&[1; 32]))
                .unwrap();

        // The tiny tokenizer is whitespace-free word-level, so one "word" per
        // text: length is the word count, plus nothing (no special tokens).
        let texts: Vec<String> = [1usize, 900, 3, 400, 2, 1200, 5]
            .iter()
            .map(|words| vec!["word"; *words].join(" "))
            .collect();
        let batches = service.plan_batches(&texts).unwrap();

        let mut seen: Vec<usize> = batches.iter().flatten().copied().collect();
        seen.sort_unstable();
        assert_eq!(
            seen,
            (0..texts.len()).collect::<Vec<_>>(),
            "every input lands in exactly one batch"
        );
        for batch in &batches {
            assert!(batch.len() <= MAX_BATCH_ROWS);
            let longest = batch
                .iter()
                .map(|&index| service.input_policy.count(&texts[index]).unwrap())
                .max()
                .unwrap();
            assert!(
                batch.len() == 1 || longest * batch.len() <= ENCODER_PADDED_TOKEN_BUDGET,
                "batch of {} rows × {longest} tokens exceeds the budget",
                batch.len()
            );
        }
        // Short texts share a batch; the 1200-token one is too wide to share.
        assert!(batches.iter().any(|batch| batch.len() > 1));
    }

    #[test]
    fn qwen3_falls_back_to_its_instruction_when_the_directory_says_nothing() {
        let dir = TempDir::new().unwrap();
        assert_eq!(
            resolve_prefixes(dir.path(), b"{}", ModelArchitecture::Qwen3).id,
            "qwen3-instruct"
        );
        assert_eq!(
            resolve_prefixes(dir.path(), b"{}", ModelArchitecture::Bert).id,
            "none"
        );
        // A recorded name wins over the directory it happens to sit in.
        assert_eq!(
            resolve_prefixes(
                dir.path(),
                br#"{"_name_or_path":"intfloat/e5-base-v2"}"#,
                ModelArchitecture::Bert
            )
            .id,
            "e5"
        );
    }

    #[test]
    fn a_document_prefix_joins_the_identity_and_a_query_only_one_does_not() {
        let dir = TempDir::new().unwrap();
        write_tiny_bert(dir.path());
        let stored = ArtifactIdentity::from_digest(&[3; 32]);
        let mut service = CandleEmbeddingService::open(dir.path(), stored.clone()).unwrap();

        assert_eq!(EmbeddingPort::model_identity(&service), stored.as_str());
        service.prefixes = &super::super::prefixes::QWEN3_INSTRUCT;
        assert_eq!(
            EmbeddingPort::model_identity(&service),
            stored.as_str(),
            "a query-side instruction leaves stored vectors untouched"
        );
        service.prefixes = &super::super::prefixes::E5;
        assert_eq!(
            EmbeddingPort::model_identity(&service),
            format!("{}+prefix-e5", stored.as_str())
        );
    }

    #[test]
    fn token_limit_and_pooling_do_not_depend_on_the_weights_file_name() {
        let dir = TempDir::new().unwrap();
        std::fs::write(
            dir.path().join("config.json"),
            r#"{"model_type":"xlm-roberta","hidden_size":1024,"max_position_embeddings":8194}"#,
        )
        .unwrap();
        let pooling = read_pooling_strategy(dir.path());
        let limit = super::super::input_policy::model_token_limit(dir.path()).unwrap();

        std::fs::write(dir.path().join(WEIGHTS_PYTORCH_BIN), b"pickle").unwrap();
        assert_eq!(read_pooling_strategy(dir.path()), pooling);
        assert_eq!(
            super::super::input_policy::model_token_limit(dir.path()).unwrap(),
            limit
        );

        std::fs::write(dir.path().join(WEIGHTS_SAFETENSORS), b"tensors").unwrap();
        assert_eq!(read_pooling_strategy(dir.path()), pooling);
        assert_eq!(
            super::super::input_policy::model_token_limit(dir.path()).unwrap(),
            limit
        );
    }
}

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
mod qwen3_live_tests {
    use super::*;
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
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../evals/retrieval/synthetic-library-v2.json");
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
}

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
use serde::Deserialize;
use std::sync::Arc;
use tokenizers::{PaddingParams, PaddingStrategy, Tokenizer};
use tokio::sync::Mutex;

use crate::application::ports::embedding_port::{span_chunk_texts, sparse_not_supported};
use crate::application::ports::EmbeddingPort;
use crate::domain::value_objects::{ArtifactIdentity, SparseEmbedding};
use crate::features::embedding::late_chunking::{
    l2_normalize_in_place, mean_pool_rows, pooling_token_indices, strategy_identity,
    validate_chunk_ranges, EmbeddingStrategy, LateChunkingError,
};
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
    Qwen3(candle_transformers::models::qwen3::Model),
}

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
        with_autorelease_pool(|| Self::open_in_pool(model_dir.as_ref(), identity))
    }

    fn open_in_pool(dir: &Path, identity: ArtifactIdentity) -> Result<Self> {
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

        let device = crate::shared::utils::best_available_compute_device("embedding");
        tracing::info!(
            device = ?device,
            model_type = %config.model_type,
            hidden_size = config.hidden_size,
            pooling = ?pooling,
            "Loading Candle embedding model"
        );

        let mut tokenizer = super::input_policy::load_tokenizer(&tokenizer_path)?;

        let input_policy = super::input_policy::InputPolicy::new(
            tokenizer.clone(),
            super::input_policy::model_token_limit(dir)?.min(2048),
        )?;
        // Preserve model-specific padding IDs (RoBERTa uses 1, BERT usually 0).
        let padding = tokenizer.get_padding().cloned().unwrap_or_else(|| {
            let pad_id = tokenizer
                .token_to_id("[PAD]")
                .or_else(|| tokenizer.token_to_id("<pad>"))
                .unwrap_or(0);
            PaddingParams {
                pad_id,
                pad_token: tokenizer
                    .id_to_token(pad_id)
                    .unwrap_or_else(|| "[PAD]".into()),
                ..Default::default()
            }
        });
        tokenizer.with_padding(Some(PaddingParams {
            strategy: PaddingStrategy::BatchLongest,
            ..padding
        }));
        tokenizer
            .with_truncation(None)
            .map_err(|e| LoadError::Tokenizer(e.to_string()))?;
        if architecture == ModelArchitecture::Qwen3 {
            // Qwen3 runs one unpadded sequence per causal pass, so the shared
            // tokenizer is configured for that once instead of cloned per batch.
            tokenizer.with_padding(None);
        }

        let var_builder = if weights_path
            .file_name()
            .is_some_and(|name| name == WEIGHTS_PYTORCH_BIN)
        {
            // `torch.save` pickle, read through `candle_core::pickle` — the
            // same reader `sparse_head` already uses for `sparse_linear.pt`.
            // It cannot be mmapped (tensors are pickled, not laid out flat),
            // so this materializes the weights once at load time.
            VarBuilder::from_pth(&weights_path, DType::F32, &device)
                .map_err(|e| LoadError::Candle(format!("pytorch_model.bin load: {}", e)))?
        } else {
            // SAFETY: we mmap a model file that we own and never mutate after
            // download. Candle's loader is built around mmap; the alternative
            // (`VarBuilder::from_buffered_safetensors`) reads the entire 100MB-1GB
            // file into RAM, which is wasteful for model weights we'll mostly
            // stream into GPU buffers anyway.
            #[allow(unsafe_code)]
            let mmaped = unsafe {
                VarBuilder::from_mmaped_safetensors(&[&weights_path], DType::F32, &device)
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
                    candle_transformers::models::qwen3::Model::new(&cfg, vb)
                        .map_err(|e| LoadError::Candle(e.to_string()))?,
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

    /// Late chunk when the strategy is on and the span is eligible; otherwise
    /// embed each chunk on its own. This is what the port and service traits
    /// expose, so callers never have to handle the fallback themselves.
    async fn embed_span(
        &self,
        span_text: &str,
        chunk_ranges: &[Range<usize>],
    ) -> Result<Vec<Vec<f32>>> {
        if self.strategy.is_late_chunking() && !chunk_ranges.is_empty() {
            match self.embed_late_chunked(span_text, chunk_ranges).await {
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
                // A fresh lightweight clone shares weights but starts with an empty
                // KV cache, exactly as the batch path does.
                ModelVariant::Qwen3(template) => {
                    let mut model = template.clone();
                    let input = Tensor::new(ids, &device)
                        .and_then(|t| t.unsqueeze(0))
                        .map_err(|e| LoadError::Candle(e.to_string()))?;
                    model
                        .forward(&input, 0)
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

    /// Run a forward pass + pool + L2-normalize for a batch of texts.
    /// Single hot path used by both `embed_single` and `embed_batch`.
    async fn forward(&self, texts: Vec<String>) -> Result<Vec<Vec<f32>>> {
        Ok(self.forward_with_sparse(texts, None).await?.0)
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

        for text in &texts {
            self.input_policy.validate(text)?;
        }

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
        let mut dense = Vec::with_capacity(texts.len());
        let mut sparse = Vec::with_capacity(texts.len());
        for batch in texts.chunks(16) {
            let (batch_dense, batch_sparse) = self
                .forward_with_sparse(batch.to_vec(), Some(MAX_PASSAGE_TERMS))
                .await?;
            dense.extend(batch_dense);
            sparse.extend(batch_sparse);
        }
        Ok((dense, sparse))
    }
}

#[async_trait]
impl EmbeddingPort for CandleEmbeddingService {
    async fn embed_single(&self, text: &str) -> Result<Vec<f32>> {
        if text.is_empty() {
            return Err(AppError::InvalidInput("Cannot embed empty text".into()));
        }
        let mut out = self.forward(vec![text.to_string()]).await?;
        out.pop().ok_or_else(|| AppError::EmbeddingFailed {
            reason: "Forward pass returned no embeddings".into(),
        })
    }

    async fn embed_query(&self, text: &str) -> Result<Vec<f32>> {
        let query = if self.architecture == ModelArchitecture::Qwen3 {
            format!("Instruct: Given a web search query, retrieve relevant passages that answer the query\nQuery:{text}")
        } else {
            text.to_owned()
        };
        <Self as EmbeddingPort>::embed_single(self, &query).await
    }

    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        let mut result = Vec::with_capacity(texts.len());
        for batch in texts.chunks(16) {
            result.extend(self.forward(batch.to_vec()).await?);
        }
        Ok(result)
    }

    fn split_text(
        &self,
        text: &str,
        prefix: &str,
    ) -> Result<Vec<crate::application::ports::embedding_port::EmbeddingTextChunk>> {
        self.input_policy.split(text, prefix)
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
            .forward_with_sparse(vec![text.to_owned()], Some(MAX_QUERY_TERMS))
            .await?;
        Ok(sparse.pop().unwrap_or_default())
    }

    fn model_identity(&self) -> String {
        strategy_identity(self.identity.as_str(), self.strategy)
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

/// Last-token embeddings for `texts`, one causal pass each. `tokenizer` is
/// the unpadded one `open` configures for Qwen3.
fn qwen3_embed(
    template: &candle_transformers::models::qwen3::Model,
    tokenizer: &Tokenizer,
    device: &Device,
    texts: &[String],
) -> Result<Vec<Vec<f32>>> {
    let mut result = Vec::with_capacity(texts.len());
    for text in texts {
        let encoding = tokenizer
            .encode(text.as_str(), true)
            .map_err(|e| LoadError::Tokenizer(e.to_string()))?;
        let ids = encoding.get_ids();
        if ids.is_empty() {
            return Err(AppError::InvalidInput(
                "Cannot embed an empty token sequence".into(),
            ));
        }
        // A fresh lightweight clone per input shares weights but starts with an
        // empty KV cache. No padding enters causal attention and no state leaks
        // between passages.
        let mut model = template.clone();
        let input = Tensor::new(ids, device)
            .and_then(|t| t.unsqueeze(0))
            .map_err(|e| LoadError::Candle(e.to_string()))?;
        let hidden = model
            .forward(&input, 0)
            .map_err(|e| LoadError::Candle(e.to_string()))?;
        let pooled = hidden
            .narrow(1, ids.len() - 1, 1)
            .and_then(|t| t.squeeze(1))
            .map_err(|e| LoadError::Candle(e.to_string()))?;
        let normalized = l2_normalize(&pooled).map_err(|e| LoadError::Candle(e.to_string()))?;
        result.extend(
            tensor_to_vec_of_vec(&normalized).map_err(|e| LoadError::Candle(e.to_string()))?,
        );
    }
    Ok(result)
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

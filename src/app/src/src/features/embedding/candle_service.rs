//! Candle-backed embedding service.
//!
//! Pure-Rust ONNX-free embedding inference. Replaces `OnnxEmbeddingService` for
//! the full-cutover migration. Loads BERT-family `safetensors` checkpoints
//! directly via `candle_transformers`. Falls back from Metal → CPU if the GPU
//! device is unavailable.
//!
//! # Architecture
//!
//! - `ModelArchitecture`: enum dispatch for the BERT variants we support today
//!   (vanilla BERT, DistilBERT, XLM-RoBERTa, MPNet, Jina v2, Nomic, ModernBERT).
//!   Decoder-style models (Gemma3, Qwen3, Llama) are explicitly rejected at
//!   load time and surfaced via `LoadError::UnsupportedArchitecture`.
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

use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use candle_core::{DType, Device, Module, Tensor};
use candle_nn::VarBuilder;
use candle_transformers::models::bert::{BertModel, Config as BertConfig, HiddenAct};
use candle_transformers::models::distilbert::{Config as DistilBertConfig, DistilBertModel};
use candle_transformers::models::gemma3::{Model as GemmaModel, Config as Gemma3Config};
use candle_transformers::models::jina_bert::{
    BertModel as JinaBertModel, Config as JinaBertConfig,
};
use candle_transformers::models::modernbert::{Config as ModernBertConfig, ModernBert};
use candle_transformers::models::nomic_bert::{Config as NomicBertConfig, NomicBertModel};
use candle_transformers::models::xlm_roberta::{Config as XlmRobertaConfig, XLMRobertaModel};
use serde::Deserialize;
use tokenizers::{PaddingParams, PaddingStrategy, Tokenizer, TruncationParams, TruncationStrategy};
use tokio::sync::Mutex;

use crate::application::ports::EmbeddingPort;
use crate::features::embedding::EmbeddingServiceTrait;
use crate::shared::error::AppError;
use crate::shared::result::Result;

/// Maximum sequence length used during tokenization. Most BERT-family
/// embedding models are trained on 512 tokens; longer text is truncated.
const MAX_SEQUENCE_LENGTH: usize = 512;

/// BERT-family architectures supported by this service. Decoder-style
/// architectures (Gemma3, Qwen3, Llama) are intentionally absent — those are
/// planned for a follow-up PR with last-token pooling.
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
    /// Decoder-style placeholder — not yet wired into `forward()`. Loading
    /// is rejected upstream until last-token pooling is implemented.
    Gemma3(GemmaModel),
}

/// Errors specific to model loading. Surfaced through `AppError::EmbeddingFailed`
/// at the trait boundary but enumerated here so callers can match if needed.
#[derive(Debug, thiserror::Error)]
pub enum LoadError {
    #[error("config.json not found at {0}")]
    MissingConfig(PathBuf),
    #[error("tokenizer.json not found at {0}")]
    MissingTokenizer(PathBuf),
    #[error("model.safetensors not found at {0}")]
    MissingWeights(PathBuf),
    #[error("unsupported architecture: {0} (only BERT-family is supported)")]
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
    model: Mutex<ModelVariant>,
    tokenizer: Tokenizer,
    device: Device,
    dimension: usize,
    pooling: PoolingStrategy,
    architecture: ModelArchitecture,
}

impl CandleEmbeddingService {
    /// Load a model from a directory containing `config.json`,
    /// `tokenizer.json`, and `model.safetensors`.
    ///
    /// Tries Metal first on macOS, falls back to CPU on failure. Reads the
    /// model dimension from `config.json::hidden_size` and the pooling
    /// strategy from `1_Pooling/config.json` if present (defaults to CLS).
    pub fn new(model_dir: impl AsRef<Path>) -> Result<Self> {
        let dir = model_dir.as_ref();

        let config_path = dir.join("config.json");
        let tokenizer_path = dir.join("tokenizer.json");
        let weights_path = dir.join("model.safetensors");

        if !config_path.exists() {
            return Err(LoadError::MissingConfig(config_path).into());
        }
        if !tokenizer_path.exists() {
            return Err(LoadError::MissingTokenizer(tokenizer_path).into());
        }
        if !weights_path.exists() {
            return Err(LoadError::MissingWeights(weights_path).into());
        }

        let config_bytes = std::fs::read(&config_path).map_err(|e| AppError::FileRead {
            path: config_path.display().to_string(),
            reason: e.to_string(),
        })?;
        let config: ModelConfig = serde_json::from_slice(&config_bytes)
            .map_err(|e| LoadError::BadConfig(e.to_string()))?;

        let architecture = ModelArchitecture::from_model_type(&config.model_type)
            .ok_or_else(|| LoadError::UnsupportedArchitecture(config.model_type.clone()))?;

        // Decoder-style architectures (Gemma3, Qwen3, Llama) need last-token
        // pooling and a different KV-cache-aware forward path. Reject them
        // here until that scaffolding lands. MPNet has no Candle loader at
        // all today — same outcome.
        match architecture {
            ModelArchitecture::Bert
            | ModelArchitecture::DistilBert
            | ModelArchitecture::XlmRoberta
            | ModelArchitecture::JinaBert
            | ModelArchitecture::NomicBert
            | ModelArchitecture::ModernBert => {}
            ModelArchitecture::Mpnet => {
                return Err(LoadError::UnsupportedArchitecture(format!(
                    "MPNet (model_type='{}') — Candle has no MPNet loader; planned",
                    config.model_type
                ))
                .into());
            }
        }

        let pooling = read_pooling_strategy(dir);

        let device = best_device();
        tracing::info!(
            device = ?device,
            model_type = %config.model_type,
            hidden_size = config.hidden_size,
            pooling = ?pooling,
            "Loading Candle embedding model"
        );

        let mut tokenizer = Tokenizer::from_file(&tokenizer_path)
            .map_err(|e| LoadError::Tokenizer(e.to_string()))?;

        // Configure padding + truncation to match the model's expected sequence
        // length. Padding to the longest in the batch (not max) keeps inference
        // fast for short inputs.
        tokenizer.with_padding(Some(PaddingParams {
            strategy: PaddingStrategy::BatchLongest,
            pad_token: "[PAD]".to_string(),
            pad_id: 0,
            ..Default::default()
        }));
        tokenizer
            .with_truncation(Some(TruncationParams {
                max_length: MAX_SEQUENCE_LENGTH,
                strategy: TruncationStrategy::LongestFirst,
                ..Default::default()
            }))
            .map_err(|e| LoadError::Tokenizer(e.to_string()))?;

        // SAFETY: we mmap a model file that we own and never mutate after
        // download. Candle's loader is built around mmap; the alternative
        // (`VarBuilder::from_buffered_safetensors`) reads the entire 100MB-1GB
        // file into RAM, which is wasteful for model weights we'll mostly
        // stream into GPU buffers anyway.
        #[allow(unsafe_code)]
        let var_builder = unsafe {
            VarBuilder::from_mmaped_safetensors(&[&weights_path], DType::F32, &device)
                .map_err(|e| LoadError::Candle(format!("safetensors load: {}", e)))?
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
            ModelArchitecture::Mpnet => unreachable!("rejected above"),
        };

        Ok(Self {
            model: Mutex::new(model),
            tokenizer,
            device,
            dimension: config.hidden_size,
            pooling,
            architecture,
        })
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
        chunks: &[crate::infrastructure::indexing::chunker::ContextualizedChunk],
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

    /// Run a forward pass + pool + L2-normalize for a batch of texts.
    /// Single hot path used by both `embed_single` and `embed_batch`.
    async fn forward(&self, texts: Vec<String>) -> Result<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(vec![]);
        }

        let encodings = self
            .tokenizer
            .encode_batch(texts.clone(), true)
            .map_err(|e| AppError::EmbeddingFailed {
                reason: format!("tokenization: {}", e),
            })?;

        let batch_size = encodings.len();
        let max_len = encodings
            .iter()
            .map(|e| e.get_ids().len())
            .max()
            .unwrap_or(0);

        // Build (batch, seq) tensors for token IDs, attention mask, and token
        // type IDs. All three are required by the BERT forward signature.
        let mut input_ids = Vec::with_capacity(batch_size * max_len);
        let mut attention_mask = Vec::with_capacity(batch_size * max_len);
        let mut token_type_ids = Vec::with_capacity(batch_size * max_len);
        for enc in &encodings {
            input_ids.extend(enc.get_ids().iter().map(|&x| x as i64));
            attention_mask.extend(enc.get_attention_mask().iter().map(|&x| x as i64));
            token_type_ids.extend(enc.get_type_ids().iter().map(|&x| x as i64));
        }

        let device = self.device.clone();
        let pooling = self.pooling;

        let input_ids_t = Tensor::from_vec(input_ids, (batch_size, max_len), &device)
            .map_err(|e| AppError::EmbeddingFailed {
                reason: format!("input_ids tensor: {}", e),
            })?;
        let attention_mask_t = Tensor::from_vec(attention_mask, (batch_size, max_len), &device)
            .map_err(|e| AppError::EmbeddingFailed {
                reason: format!("attention_mask tensor: {}", e),
            })?;
        let token_type_ids_t = Tensor::from_vec(token_type_ids, (batch_size, max_len), &device)
            .map_err(|e| AppError::EmbeddingFailed {
                reason: format!("token_type_ids tensor: {}", e),
            })?;

        // Serialize Metal access — concurrent kernel dispatch can crash.
        let guard = self.model.lock().await;

        // Each architecture's forward takes a slightly different signature.
        // DistilBERT skips token_type_ids entirely. XLM-RoBERTa accepts them
        // but reorders the args + has 3 None tail arguments for KV-cache /
        // cross-attention features we don't use. ModernBert names its first
        // arg `xs` but treats it as input_ids. Nomic + Jina take optional
        // token_type_ids — we pass them when available since they're cheap.
        let hidden_states = match &*guard {
            ModelVariant::Bert(model) => model
                .forward(&input_ids_t, &token_type_ids_t, Some(&attention_mask_t))
                .map_err(|e| AppError::EmbeddingFailed {
                    reason: format!("BertModel forward: {}", e),
                })?,
            ModelVariant::DistilBert(model) => model
                .forward(&input_ids_t, &attention_mask_t)
                .map_err(|e| AppError::EmbeddingFailed {
                    reason: format!("DistilBertModel forward: {}", e),
                })?,
            ModelVariant::XlmRoberta(model) => model
                .forward(
                    &input_ids_t,
                    &attention_mask_t,
                    &token_type_ids_t,
                    None,
                    None,
                    None,
                )
                .map_err(|e| AppError::EmbeddingFailed {
                    reason: format!("XLMRobertaModel forward: {}", e),
                })?,
            ModelVariant::JinaBert(model) => model
                .forward(&input_ids_t)
                .map_err(|e| AppError::EmbeddingFailed {
                    reason: format!("JinaBertModel forward: {}", e),
                })?,
            ModelVariant::NomicBert(model) => model
                .forward(
                    &input_ids_t,
                    Some(&token_type_ids_t),
                    Some(&attention_mask_t),
                )
                .map_err(|e| AppError::EmbeddingFailed {
                    reason: format!("NomicBertModel forward: {}", e),
                })?,
            ModelVariant::ModernBert(model) => model
                .forward(&input_ids_t, &attention_mask_t)
                .map_err(|e| AppError::EmbeddingFailed {
                    reason: format!("ModernBert forward: {}", e),
                })?,
            ModelVariant::Gemma3(_model) => {
                return Err(AppError::EmbeddingFailed {
                    reason: "Gemma3 not wired yet — needs last-token pooling + decoder-style runner".into(),
                })
            }
        };

        drop(guard);

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

        Ok(vectors)
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

    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        self.forward(texts.to_vec()).await
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
    async fn embed_single(&self, text: &str) -> Result<Vec<f32>> {
        <Self as EmbeddingPort>::embed_single(self, text).await
    }

    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        <Self as EmbeddingPort>::embed_batch(self, texts).await
    }

    async fn embed_contextualized_chunks(
        &self,
        chunks: &[crate::infrastructure::indexing::chunker::ContextualizedChunk],
    ) -> Result<Vec<Vec<f32>>> {
        self.embed_contextualized_chunks(chunks).await
    }
}

// ============================================================================
// Helpers
// ============================================================================

/// Try Metal first on macOS, fall back to CPU.
fn best_device() -> Device {
    #[cfg(target_os = "macos")]
    {
        match Device::new_metal(0) {
            Ok(d) => return d,
            Err(e) => tracing::warn!(error = %e, "Metal unavailable, using CPU"),
        }
    }
    Device::Cpu
}

/// Read pooling strategy from `1_Pooling/config.json` if present.
/// Defaults to CLS — that's what BGE/mxbai/UAE use, and it's the safer default
/// for "I don't know" cases (mean-pool of a model expecting CLS produces noise
/// rather than a clear failure mode).
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
    PoolingStrategy::Cls
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
    let mask = attention_mask
        .to_dtype(DType::F32)?
        .unsqueeze(2)?; // (B, S, 1)
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
    let mut out = Vec::with_capacity(batch);
    for i in 0..batch {
        out.push(flat[i * hidden..(i + 1) * hidden].to_vec());
    }
    Ok(out)
}

// Tensor indexing helper trait (candle uses an extension-trait pattern for `i`).
use candle_core::IndexOp;

//! Loading a checkpoint directory: device, dtype, pooling, prefixes and weights.

use std::num::NonZeroUsize;
use std::path::Path;
use std::sync::Arc;

use candle_core::DType;
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
use tokenizers::{PaddingDirection, PaddingParams, PaddingStrategy};
use tokio::sync::Mutex;

use crate::features::embedding::late_chunking::EmbeddingStrategy;
use crate::features::embedding::qwen3_encoder::Qwen3Encoder;
use crate::features::embedding::sparse_head::SparseHead;
use lattice_core::domain::models::checkpoint::{weights_path, WEIGHTS_PYTORCH_BIN};
use lattice_core::domain::value_objects::ArtifactIdentity;
use lattice_core::shared::error::AppError;
use lattice_core::shared::error::Result;
use lattice_core::shared::runtime::with_autorelease_pool;

use super::config::{read_pooling_strategy, resolve_prefixes, weights_dtype};
use super::{
    CandleEmbeddingService, LoadError, ModelArchitecture, ModelConfig, ModelVariant,
    QUERY_CACHE_CAPACITY,
};

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
    /// (`features/embedding/loading.rs`) and call `with_strategy`.
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
    pub(super) fn open_as(
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

        let device =
            crate::infrastructure::ml::compute_device::best_available_compute_device("embedding");
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

        let mut tokenizer = crate::infrastructure::ml::tokenizer::load_tokenizer(&tokenizer_path)?;

        let input_policy = super::super::input_policy::InputPolicy::new(
            tokenizer.clone(),
            super::super::input_policy::model_token_limit(dir)?.min(2048),
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

        let accelerator_bytes = accelerator_weight_bytes(&weights_path, dtype, &device);
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
            accelerator_bytes,
        })
    }

    /// Compute the artifact identity of `model_dir`, then [`open`](Self::open)
    /// it. For directories that have no `models` row: env-configured structure
    /// models, eval tooling, tests. Never call this from a launch or
    /// activation path; it streams every weights file through SHA-256.
    pub fn open_unregistered(model_dir: impl AsRef<Path>) -> Result<Self> {
        let dir = model_dir.as_ref();
        let identity = crate::infrastructure::ml::artifact_identity::compute(dir)?;
        Self::open(dir, identity)
    }
}

/// Bytes the weights at `weights_path` take once loaded as `dtype` on
/// `device`, when that is an accelerator a local LLM server could share; 0 on
/// the CPU. A safetensors header gives every tensor's shape without reading
/// the weights; any other file counts as its size on disk.
fn accelerator_weight_bytes(
    weights_path: &Path,
    dtype: DType,
    device: &candle_core::Device,
) -> u64 {
    if device.is_cpu() {
        return 0;
    }
    safetensors_elements(weights_path).map_or_else(
        || std::fs::metadata(weights_path).map_or(0, |file| file.len()),
        |elements| elements.saturating_mul(dtype.size_in_bytes() as u64),
    )
}

/// The elements of every tensor a safetensors file declares in its header.
/// `None` for a file that is not safetensors.
fn safetensors_elements(path: &Path) -> Option<u64> {
    use std::io::Read;

    /// A header past this is not one; refusing it bounds the allocation.
    const HEADER_MAX_BYTES: u64 = 100 * 1024 * 1024;

    let mut file = std::fs::File::open(path).ok()?;
    let mut length = [0u8; 8];
    file.read_exact(&mut length).ok()?;
    let length = u64::from_le_bytes(length);
    if length > HEADER_MAX_BYTES {
        return None;
    }
    let mut header = vec![0u8; usize::try_from(length).ok()?];
    file.read_exact(&mut header).ok()?;
    let header: serde_json::Map<String, serde_json::Value> =
        serde_json::from_slice(&header).ok()?;
    Some(
        header
            .iter()
            .filter(|(name, _)| name.as_str() != "__metadata__")
            .filter_map(|(_, tensor)| {
                tensor
                    .get("shape")?
                    .as_array()?
                    .iter()
                    .map(serde_json::Value::as_u64)
                    .product::<Option<u64>>()
            })
            .sum(),
    )
}

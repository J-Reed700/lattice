//! Reading pooling, dtype and instruction prefixes from a model directory.

use std::path::Path;

use candle_core::{DType, Device};

use crate::features::embedding::prefixes::EmbeddingPrefixes;

use super::{ModelArchitecture, ModelConfig, PoolingConfig, PoolingStrategy};

/// Read pooling strategy. Tries in order:
/// 1. `1_Pooling/config.json` (sentence-transformers explicit config)
/// 2. `config.json` for a sentence-transformers heuristic (`_name_or_path`
///    pointing at a known mean-pooled architecture)
/// 3. CLS default — what BGE/mxbai/UAE use.
pub(super) fn read_pooling_strategy(model_dir: &Path) -> PoolingStrategy {
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
pub(super) fn weights_dtype(architecture: ModelArchitecture, device: &Device) -> DType {
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
pub(super) fn resolve_prefixes(
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
        .and_then(super::super::prefixes::prefixes_for)
        .or_else(|| {
            dir.file_name()
                .and_then(|name| name.to_str())
                .and_then(super::super::prefixes::prefixes_for)
        })
        .unwrap_or(match architecture {
            // Every Qwen3 checkpoint this runtime accepts is an embedding
            // model, and the whole Qwen3-Embedding line is instruction-tuned.
            ModelArchitecture::Qwen3 => &super::super::prefixes::QWEN3_INSTRUCT,
            _ => &super::super::prefixes::NONE,
        })
}

/// Run the encoder for one padded batch, returning (batch, seq, hidden).
/// Shared by the pooled batch path and by late chunking, which needs the
/// per-token states rather than a pooled vector.
/// The prefixes a model directory would load with, without loading it. Search
/// binds its index under [`vector_identity`] before the model is opened, and
/// that key must match the one the opened service reports.
pub fn prefixes_for_model_dir(dir: &Path) -> &'static EmbeddingPrefixes {
    let config_bytes = std::fs::read(dir.join("config.json")).unwrap_or_default();
    // An unreadable config or an unknown `model_type` only loses the
    // architecture fallback, and none of those fallbacks prefix documents.
    let architecture = serde_json::from_slice::<ModelConfig>(&config_bytes)
        .ok()
        .and_then(|config| ModelArchitecture::from_model_type(&config.model_type))
        .unwrap_or(ModelArchitecture::Bert);
    resolve_prefixes(dir, &config_bytes, architecture)
}

//! Curated embedding models, and which of them support Matryoshka truncation.

use crate::domain::models::checkpoint::TOKENIZER_JSON;
use crate::domain::models::metadata::ModelFileMetadata;
use crate::domain::models::selection::ModelFormat;
use crate::domain::models::selection::{ModelCategory, ModelMetadata, PerformanceTier};

/// The curated embedding models for semantic search.
///
/// One entry: **Qwen3 Embedding 0.6B**, multilingual instruction-aware
/// retrieval at 1024 dimensions, the first-run default on every machine. It
/// runs in the bundled `llama-server` from an F16 GGUF, the only precision that
/// keeps the vectors Candle produced. `tokenizer.json` comes from the base
/// repository and cuts passages with the model's own tokenizer.
///
/// Both files are pinned to a commit and carry their SHA-256, so the download
/// engine verifies them and the GGUF's digest is the model's identity.
pub fn get_curated_embedding_models() -> Vec<ModelMetadata> {
    vec![ModelMetadata {
        id: "qwen3-embedding-0.6b".into(),
        name: "Qwen3 Embedding 0.6B".into(),
        category: ModelCategory::Embedding,
        description: "Instruction-aware multilingual retrieval, the default on every machine. Runs in the bundled llama.cpp server (F16) and embeds up to 2048 tokens per passage.".into(),
        size_gb: 1.2,
        minimum_ram_gb: 6.0,
        recommended_ram_gb: 8.0,
        context_length: 2048,
        performance_tier: PerformanceTier::Balanced,
        supported_quantizations: vec!["F16".into()],
        capabilities: vec!["embedding".into(), "retrieval".into(), "multilingual".into()],
        download_url: Some("https://huggingface.co/Qwen/Qwen3-Embedding-0.6B-GGUF".into()),
        license: "Apache-2.0".into(),
        requires_auth: false,
        model_id: Some("Qwen/Qwen3-Embedding-0.6B".into()),
        default_filename: None,
        files: vec![
            ModelFileMetadata::with_checksum(
                "Qwen3-Embedding-0.6B-f16.gguf".into(),
                "https://huggingface.co/Qwen/Qwen3-Embedding-0.6B-GGUF/resolve/370f27d7550e0def9b39c1f16d3fbaa13aa67728/Qwen3-Embedding-0.6B-f16.gguf".into(),
                QWEN3_EMBEDDING_GGUF_BYTES,
                "421a27e58d165478cc7acb984a688c2aa41404968b0203e7cd743ece44c54340".into(),
            ),
            ModelFileMetadata::with_checksum(
                TOKENIZER_JSON.into(),
                "https://huggingface.co/Qwen/Qwen3-Embedding-0.6B/resolve/97b0c614be4d77ee51c0cef4e5f07c00f9eb65b3/tokenizer.json".into(),
                QWEN3_EMBEDDING_TOKENIZER_BYTES,
                "def76fb086971c7867b829c23a26261e38d9d74e02139253b38aeb9df8b4b50a".into(),
            ),
        ],
        total_size_bytes: QWEN3_EMBEDDING_GGUF_BYTES + QWEN3_EMBEDDING_TOKENIZER_BYTES,
        embedding_dimensions: Some(1024),
        embedding_compatibility: None,
        format: ModelFormat::Gguf,
    }]
}

const QWEN3_EMBEDDING_GGUF_BYTES: u64 = 1_197_629_632;
const QWEN3_EMBEDDING_TOKENIZER_BYTES: u64 = 11_423_705;

/// Curated embedding entries whose training documents Matryoshka Representation
/// Learning, i.e. whose leading coordinates are individually usable as a
/// shorter embedding.
///
/// A per-entry flag rather than a guess: truncating a model that was not
/// trained this way silently destroys its geometry, and the failure looks like
/// "search got worse" rather than like an error. Qwen3-Embedding ships MRL as a
/// documented feature (its card advertises user-defined output dimensions).
///
/// Matched against the *catalog* id, and also against the HuggingFace
/// `model_id`, because a downloaded model records whichever the download path
/// gave it.
const MATRYOSHKA_EMBEDDING_MODELS: &[&str] = &["qwen3-embedding-0.6b", "qwen/qwen3-embedding-0.6b"];

/// Whether a curated embedding entry advertises Matryoshka support.
///
/// `false` for anything not in the catalog: an unknown model has made no such
/// promise, and the safe reading of silence is "do not truncate".
pub fn embedding_model_supports_matryoshka(model_id: &str) -> bool {
    let normalized = model_id.trim().to_lowercase();
    MATRYOSHKA_EMBEDDING_MODELS.contains(&normalized.as_str())
}

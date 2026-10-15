//! Curated embedding models, and which of them support Matryoshka truncation.

use crate::domain::models::metadata::ModelFileMetadata;
use crate::domain::models::selection::ModelFormat;
use crate::domain::models::selection::{ModelCategory, ModelMetadata, PerformanceTier};

/// Get curated list of embedding models for semantic search.
///
/// Returns models optimized for:
/// - **Qwen3 Embedding 0.6B**: multilingual instruction-aware retrieval (1024 dim).
///   The first-run default wherever a GPU backend is available.
/// - **all-MiniLM-L6-v2**: Compact, fast general-purpose embeddings (384 dim).
///   The first-run default on CPU-only machines, where 0.6B parameters would
///   make bulk indexing impractical.
///
/// These models generate vector representations for semantic similarity.
/// All entries use safetensors files compatible with the Candle runtime.
pub fn get_curated_embedding_models() -> Vec<ModelMetadata> {
    vec![
        ModelMetadata {
            id: "qwen3-embedding-0.6b".into(),
            name: "Qwen3 Embedding 0.6B".into(),
            category: ModelCategory::Embedding,
            description: "Instruction-aware multilingual retrieval, and the default wherever a GPU can run it. Local runtime uses up to 2048 tokens per passage and half precision on Metal.".into(),
            size_gb: 1.2,
            minimum_ram_gb: 6.0,
            recommended_ram_gb: 8.0,
            context_length: 2048,
            performance_tier: PerformanceTier::Balanced,
            supported_quantizations: vec!["F16".into(), "F32".into()],
            capabilities: vec!["embedding".into(), "retrieval".into(), "multilingual".into()],
            download_url: Some("https://huggingface.co/Qwen/Qwen3-Embedding-0.6B".into()),
            license: "Apache-2.0".into(),
            requires_auth: false,
            model_id: Some("Qwen/Qwen3-Embedding-0.6B".into()),
            default_filename: None,
            files: ["model.safetensors", "tokenizer.json", "config.json", "tokenizer_config.json", "1_Pooling/config.json"].into_iter().map(|name| {
                ModelFileMetadata::new(name.into(), format!("https://huggingface.co/Qwen/Qwen3-Embedding-0.6B/resolve/main/{name}"), if name == "model.safetensors" { 1_200_000_000 } else { 1_000 })
            }).collect(),
            total_size_bytes: 1_210_000_000,
            embedding_dimensions: Some(1024),
            embedding_compatibility: None,
            format: ModelFormat::Safetensors,
        },
        ModelMetadata {
            id: "all-minilm-l6-v2".into(),
            name: "all-MiniLM-L6-v2".into(),
            category: ModelCategory::Embedding,
            description: "Compact embedding model (384 dimensions). Very fast inference, good quality for most use cases.".into(),
            size_gb: 0.09,
            minimum_ram_gb: 1.0,
            recommended_ram_gb: 2.0,
            context_length: 512,
            performance_tier: PerformanceTier::Fast,
            supported_quantizations: vec!["F16".into(), "F32".into()],
            capabilities: vec!["embedding".into(), "retrieval".into(), "fast".into()],
            download_url: Some("https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2".into()),
            license: "Apache-2.0".into(),
            requires_auth: false,
            model_id: Some("sentence-transformers/all-MiniLM-L6-v2".into()),
            default_filename: None, // Multi-file model bundle (ONNX + tokenizer metadata)
            files: vec![
                ModelFileMetadata::new(
                    "model.safetensors".to_string(),
                    "https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2/resolve/main/model.safetensors".to_string(),
                    90_900_000,
                ),
                ModelFileMetadata::new(
                    "tokenizer.json".to_string(),
                    "https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2/resolve/main/tokenizer.json".to_string(),
                    700_000,
                ),
                ModelFileMetadata::new(
                    "config.json".to_string(),
                    "https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2/resolve/main/config.json".to_string(),
                    1_000,
                ),
                ModelFileMetadata::new("tokenizer_config.json".into(), "https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2/resolve/main/tokenizer_config.json".into(), 1000),
                ModelFileMetadata::new("sentence_bert_config.json".into(), "https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2/resolve/main/sentence_bert_config.json".into(), 1000),
                ModelFileMetadata::new("1_Pooling/config.json".into(), "https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2/resolve/main/1_Pooling/config.json".into(), 1000),
            ],
            total_size_bytes: 91_700_000,
            embedding_dimensions: Some(384),
            embedding_compatibility: None,
            format: ModelFormat::Safetensors,
        },
    ]
}

/// Curated embedding entries whose training documents Matryoshka Representation
/// Learning, i.e. whose leading coordinates are individually usable as a
/// shorter embedding.
///
/// A per-entry flag rather than a guess: truncating a model that was not
/// trained this way silently destroys its geometry, and the failure looks like
/// "search got worse" rather than like an error. Qwen3-Embedding ships MRL as a
/// documented feature (its card advertises user-defined output dimensions).
/// all-MiniLM-L6-v2 predates the technique entirely.
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

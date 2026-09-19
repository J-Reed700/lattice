//! Embedding-related constants for the domain layer.
//!
//! These are the canonical defaults for embedding configuration.
//!
//! The `DEFAULT_EMBEDDING_*` constants describe all-MiniLM-L6-v2, which is the
//! fallback dimension and name wherever nothing better is known — not, any
//! more, what a fresh install downloads. First-run picks its model through
//! [`default_embedding_model`], which takes the hardware into account.

/// Default embedding dimension for the standard model.
///
/// This is the "nothing is loaded yet" answer, not a claim about the active
/// model: every place that can ask a loaded model for its real dimension does
/// (`config.json::hidden_size`), and this is what remains when there is nothing
/// to ask. It stays at MiniLM's 384 because an install that never downloads a
/// model has no vectors of any width.
pub const DEFAULT_EMBEDDING_DIM: usize = 384;

/// Default embedding model identifier (Hugging Face).
pub const DEFAULT_EMBEDDING_MODEL_NAME: &str = "sentence-transformers/all-MiniLM-L6-v2";

/// Default embedding model display name (human-friendly).
pub const DEFAULT_EMBEDDING_MODEL_DISPLAY_NAME: &str = "all-MiniLM-L6-v2";

/// Default embedding model curated catalog id. This is what the download
/// use case looks up against `ModelMetadata.id` (case-sensitive). Must
/// match the `id` field of the corresponding entry in `curated_models.rs`,
/// NOT the display name and NOT the Hugging Face repo id.
pub const DEFAULT_EMBEDDING_MODEL_CURATED_ID: &str = "all-minilm-l6-v2";

/// Qwen3-Embedding-0.6B: 1024 dimensions, a 2048-token window in this runtime,
/// instruction-aware, multilingual, and Matryoshka-trained.
pub const QWEN3_EMBEDDING_MODEL_NAME: &str = "Qwen/Qwen3-Embedding-0.6B";
pub const QWEN3_EMBEDDING_MODEL_DISPLAY_NAME: &str = "Qwen3 Embedding 0.6B";
pub const QWEN3_EMBEDDING_MODEL_CURATED_ID: &str = "qwen3-embedding-0.6b";
pub const QWEN3_EMBEDDING_DIM: usize = 1024;

/// The embedding model a fresh install should download, named three ways.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DefaultEmbeddingModel {
    /// Curated catalog id — what the download use case looks up.
    pub curated_id: &'static str,
    /// Human-facing name.
    pub display_name: &'static str,
    /// Hugging Face repo id.
    pub hugging_face_id: &'static str,
    pub dimension: usize,
}

/// The first-run embedding model for this machine.
///
/// Qwen3-Embedding-0.6B is a far better retriever than all-MiniLM-L6-v2 — 1024
/// dimensions against 384, a 2048-token window against 256, instruction-aware
/// and multilingual, and Matryoshka-trained so the index can be compressed
/// without losing the geometry. It is also 0.6B parameters, and on CPU that
/// turns bulk indexing of a library into an overnight job. So it is the default
/// exactly where there is a GPU backend to run it, and MiniLM stays the default
/// everywhere else: a slower, older model that finishes beats a better one that
/// does not.
///
/// `gpu_accelerated` is whether the embedding runtime will actually get an
/// accelerator, not whether the machine has a GPU — see
/// `shared::utils::gpu_acceleration_available`, which answers the same question
/// the model loader will ask later.
pub fn default_embedding_model(gpu_accelerated: bool) -> DefaultEmbeddingModel {
    if gpu_accelerated {
        DefaultEmbeddingModel {
            curated_id: QWEN3_EMBEDDING_MODEL_CURATED_ID,
            display_name: QWEN3_EMBEDDING_MODEL_DISPLAY_NAME,
            hugging_face_id: QWEN3_EMBEDDING_MODEL_NAME,
            dimension: QWEN3_EMBEDDING_DIM,
        }
    } else {
        DefaultEmbeddingModel {
            curated_id: DEFAULT_EMBEDDING_MODEL_CURATED_ID,
            display_name: DEFAULT_EMBEDDING_MODEL_DISPLAY_NAME,
            hugging_face_id: DEFAULT_EMBEDDING_MODEL_NAME,
            dimension: DEFAULT_EMBEDDING_DIM,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_defaults_name_a_curated_catalog_entry() {
        let ids: Vec<String> = crate::domain::curated_models::get_curated_embedding_models()
            .into_iter()
            .map(|model| model.id)
            .collect();
        for accelerated in [true, false] {
            let default = default_embedding_model(accelerated);
            assert!(
                ids.iter().any(|id| id == default.curated_id),
                "{} is not in the curated embedding catalog",
                default.curated_id
            );
        }
    }

    #[test]
    fn a_gpu_gets_qwen3_and_a_cpu_keeps_minilm() {
        assert_eq!(
            default_embedding_model(true).curated_id,
            QWEN3_EMBEDDING_MODEL_CURATED_ID
        );
        assert_eq!(default_embedding_model(true).dimension, 1024);
        assert_eq!(
            default_embedding_model(false).curated_id,
            DEFAULT_EMBEDDING_MODEL_CURATED_ID
        );
        assert_eq!(default_embedding_model(false).dimension, 384);
    }

    /// The Matryoshka gate keys on the model id, so the accelerated default
    /// turning compression on for fresh installs is a property of this pairing,
    /// not a coincidence.
    #[test]
    fn the_accelerated_default_is_a_model_compression_is_allowed_to_truncate() {
        use crate::domain::curated_models::embedding_model_supports_matryoshka;
        assert!(embedding_model_supports_matryoshka(
            default_embedding_model(true).curated_id
        ));
        assert!(!embedding_model_supports_matryoshka(
            default_embedding_model(false).curated_id
        ));
    }
}

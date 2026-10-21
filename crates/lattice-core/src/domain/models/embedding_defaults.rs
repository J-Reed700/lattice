//! Embedding-related constants for the domain layer.
//!
//! The `DEFAULT_EMBEDDING_*` constants describe Qwen3-Embedding-0.6B, the one
//! curated embedding model and what every fresh install downloads.

/// Default embedding dimension: Qwen3-Embedding-0.6B's 1024.
///
/// This is the "nothing is loaded yet" answer, not a claim about the active
/// model: every place that can ask a loaded model for its real dimension does,
/// and this is what remains when there is nothing to ask.
pub const DEFAULT_EMBEDDING_DIM: usize = 1024;

/// Default embedding model identifier (Hugging Face).
pub const DEFAULT_EMBEDDING_MODEL_NAME: &str = "Qwen/Qwen3-Embedding-0.6B";

/// Default embedding model display name (human-friendly).
pub const DEFAULT_EMBEDDING_MODEL_DISPLAY_NAME: &str = "Qwen3 Embedding 0.6B";

/// Default embedding model curated catalog id. This is what the download
/// use case looks up against `ModelMetadata.id` (case-sensitive). Must
/// match the `id` field of the corresponding entry in `curated/embedding.rs`,
/// NOT the display name and NOT the Hugging Face repo id.
pub const DEFAULT_EMBEDDING_MODEL_CURATED_ID: &str = "qwen3-embedding-0.6b";

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

/// The first-run embedding model, the same on every machine.
///
/// Qwen3-Embedding-0.6B runs in the bundled `llama-server`: on the GPU where
/// the sidecar lists one (Metal, Vulkan) and on the CPU otherwise. The device
/// decides only how many layers are offloaded, never which model is offered.
pub fn default_embedding_model() -> DefaultEmbeddingModel {
    DefaultEmbeddingModel {
        curated_id: DEFAULT_EMBEDDING_MODEL_CURATED_ID,
        display_name: DEFAULT_EMBEDDING_MODEL_DISPLAY_NAME,
        hugging_face_id: DEFAULT_EMBEDDING_MODEL_NAME,
        dimension: DEFAULT_EMBEDDING_DIM,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::models::curated::get_curated_embedding_models;

    #[test]
    fn the_default_is_the_curated_entry_it_names() {
        let default = default_embedding_model();
        let entry = get_curated_embedding_models()
            .into_iter()
            .find(|model| model.id == default.curated_id)
            .expect("the default is in the curated embedding catalog");
        assert_eq!(entry.model_id.as_deref(), Some(default.hugging_face_id));
        assert_eq!(entry.name, default.display_name);
        assert_eq!(entry.embedding_dimensions, Some(default.dimension));
    }

    /// The Matryoshka gate keys on the model id, so the default turning
    /// compression on for fresh installs is a property of this entry, not a
    /// coincidence.
    #[test]
    fn the_default_is_a_model_compression_is_allowed_to_truncate() {
        use crate::domain::models::curated::embedding_model_supports_matryoshka;
        assert!(embedding_model_supports_matryoshka(
            default_embedding_model().curated_id
        ));
    }
}

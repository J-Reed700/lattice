//! Embedding-related constants for the domain layer.
//!
//! These are the canonical defaults for embedding configuration.

/// Default embedding dimension for the standard model.
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

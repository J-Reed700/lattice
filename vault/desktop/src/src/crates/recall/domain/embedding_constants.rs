//! Embedding-related constants for the domain layer.
//!
//! These are the canonical defaults for embedding configuration.

/// Default embedding dimension for the standard model.
pub const DEFAULT_EMBEDDING_DIM: usize = 768;

/// Default embedding model identifier (Hugging Face).
pub const DEFAULT_EMBEDDING_MODEL_NAME: &str = "sentence-transformers/all-mpnet-base-v2";

/// Default embedding model display name (human-friendly).
pub const DEFAULT_EMBEDDING_MODEL_DISPLAY_NAME: &str = "all-mpnet-base-v2";

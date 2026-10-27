//! # Embedding Generation DTOs
//!
//! Data Transfer Objects for text embedding operations.
//!
//! These DTOs define the contracts for:
//! - Single text embedding generation
//! - Batch embedding generation
//! - Embedding model information
//!
//! ## Design
//!
//! - **Minimal**: Only essential fields for embedding operations
//! - **Clear**: Descriptive names and types
//! - **Validated**: Strong typing with reasonable limits
//! - **Efficient**: Supports batch operations for performance

use serde::{Deserialize, Serialize};

/// Request to generate an embedding for a single text.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerateSingleEmbeddingRequestDto {
    /// The text to generate an embedding for (max 10,000 characters)
    pub text: String,
}

/// Response containing the generated embedding.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerateSingleEmbeddingResponseDto {
    /// The generated embedding vector
    pub embedding: Vec<f32>,

    /// Dimensionality of the embedding (e.g., DEFAULT_EMBEDDING_DIM)
    pub dimension: usize,
}

/// Request to generate embeddings for multiple texts.
///
/// Batch processing is more efficient than calling single embedding
/// generation multiple times, especially for GPU-based models.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerateBatchEmbeddingsRequestDto {
    /// The texts to generate embeddings for (1-100 items, each max 10,000 chars)
    pub texts: Vec<String>,
}

/// Response containing the generated embeddings.
///
/// Embeddings are returned in the same order as the input texts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerateBatchEmbeddingsResponseDto {
    /// The generated embeddings (same order as input texts)
    pub embeddings: Vec<Vec<f32>>,
}

/// Embedding model metadata.
///
/// Provides information about the currently loaded embedding model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingModelInfoDto {
    /// Model name/identifier
    pub model_name: String,

    /// Embedding dimensionality
    pub dimension: usize,

    /// Maximum number of tokens the model can process
    pub max_tokens: usize,
}

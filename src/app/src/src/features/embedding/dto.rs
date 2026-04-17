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

// =============================================================================
// Generate Single Embedding
// =============================================================================

/// Request to generate an embedding for a single text.
///
/// # Example
/// ```rust
/// use vault_desktop::application::dtos::embedding_dto::GenerateSingleEmbeddingRequestDto;
///
/// let request = GenerateSingleEmbeddingRequestDto {
///     text: "This is the text to embed".to_string(),
/// };
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerateSingleEmbeddingRequestDto {
    /// The text to generate an embedding for (max 10,000 characters)
    pub text: String,
}

/// Response containing the generated embedding.
///
/// # Example
/// ```rust
/// # use vault_desktop::application::dtos::embedding_dto::GenerateSingleEmbeddingResponseDto;
/// use vault_desktop::domain::embedding_constants::DEFAULT_EMBEDDING_DIM;
/// let response = GenerateSingleEmbeddingResponseDto {
///     embedding: vec![0.1, 0.2, 0.3], // DEFAULT_EMBEDDING_DIM values
///     dimension: DEFAULT_EMBEDDING_DIM,
/// };
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerateSingleEmbeddingResponseDto {
    /// The generated embedding vector
    pub embedding: Vec<f32>,

    /// Dimensionality of the embedding (e.g., DEFAULT_EMBEDDING_DIM)
    pub dimension: usize,
}

// =============================================================================
// Generate Batch Embeddings
// =============================================================================

/// Request to generate embeddings for multiple texts.
///
/// Batch processing is more efficient than calling single embedding
/// generation multiple times, especially for GPU-based models.
///
/// # Example
/// ```rust
/// use vault_desktop::application::dtos::embedding_dto::GenerateBatchEmbeddingsRequestDto;
///
/// let request = GenerateBatchEmbeddingsRequestDto {
///     texts: vec![
///         "First text to embed".to_string(),
///         "Second text to embed".to_string(),
///         "Third text to embed".to_string(),
///     ],
/// };
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerateBatchEmbeddingsRequestDto {
    /// The texts to generate embeddings for (1-100 items, each max 10,000 chars)
    pub texts: Vec<String>,
}

/// Response containing the generated embeddings.
///
/// Embeddings are returned in the same order as the input texts.
///
/// # Example
/// ```rust
/// # use vault_desktop::application::dtos::embedding_dto::GenerateBatchEmbeddingsResponseDto;
/// let response = GenerateBatchEmbeddingsResponseDto {
///     embeddings: vec![
///         vec![0.1, 0.2, 0.3], // First text embedding
///         vec![0.4, 0.5, 0.6], // Second text embedding
///         vec![0.7, 0.8, 0.9], // Third text embedding
///     ],
/// };
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerateBatchEmbeddingsResponseDto {
    /// The generated embeddings (same order as input texts)
    pub embeddings: Vec<Vec<f32>>,
}

// =============================================================================
// Get Embedding Model Info
// =============================================================================

/// Embedding model metadata.
///
/// Provides information about the currently loaded embedding model.
///
/// # Example
/// ```rust
/// # use vault_desktop::application::dtos::embedding_dto::EmbeddingModelInfoDto;
/// use vault_desktop::domain::embedding_constants::{
///     DEFAULT_EMBEDDING_DIM, DEFAULT_EMBEDDING_MODEL_NAME
/// };
/// let info = EmbeddingModelInfoDto {
///     model_name: DEFAULT_EMBEDDING_MODEL_NAME.to_string(),
///     dimension: DEFAULT_EMBEDDING_DIM,
///     max_tokens: 512,
/// };
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingModelInfoDto {
    /// Model name/identifier
    pub model_name: String,

    /// Embedding dimensionality
    pub dimension: usize,

    /// Maximum number of tokens the model can process
    pub max_tokens: usize,
}

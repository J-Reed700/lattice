use serde::{Deserialize, Serialize};

/// Request to generate an embedding for a single text.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerateSingleEmbeddingRequestDto {
    /// The text to generate an embedding for.
    pub text: String,
}

/// Response containing the generated embedding.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerateSingleEmbeddingResponseDto {
    /// The generated embedding vector.
    pub embedding: Vec<f32>,
    /// Dimensionality of the embedding vector.
    pub dimension: usize,
}

/// Request to generate embeddings for multiple texts in batch.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerateBatchEmbeddingsRequestDto {
    /// The texts to generate embeddings for.
    pub texts: Vec<String>,
}

/// Response containing the generated embeddings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerateBatchEmbeddingsResponseDto {
    /// The generated embeddings in the same order as input texts.
    pub embeddings: Vec<Vec<f32>>,
}

/// Embedding model metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingModelInfoDto {
    /// Model name/identifier.
    pub model_name: String,
    /// Embedding dimensionality.
    pub dimension: usize,
    /// Maximum number of tokens the model can process.
    pub max_tokens: usize,
}

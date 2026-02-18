//! # Embedding Generation Use Cases
//!
//! Use cases for text embedding operations:
//! - **Generate Single Embedding**: Create embedding for one text
//! - **Generate Batch Embeddings**: Create embeddings for multiple texts efficiently
//! - **Get Model Info**: Retrieve embedding model metadata
//!
//! ## Module Organization
//!
//! Each use case is self-contained with:
//! - Business logic orchestration
//! - Input validation
//! - Service coordination
//! - Comprehensive tests

pub mod generate_batch_embeddings;
pub mod generate_single_embedding;
pub mod get_model_info;

// Re-export use cases
pub use generate_batch_embeddings::GenerateBatchEmbeddingsUseCase;
pub use generate_single_embedding::GenerateSingleEmbeddingUseCase;
pub use get_model_info::GetEmbeddingModelInfoUseCase;

//! Embedding service module
//!
//! Re-exports embedding services from the ML infrastructure layer

pub mod builder;
pub mod dynamic_embedding_service;

// Re-export the ONNX embedding service as the primary embedding service
pub use crate::infrastructure::ml::OnnxEmbeddingService as EmbeddingService;
pub use dynamic_embedding_service::DynamicEmbeddingService;

// Export MODEL_NAME constant for compatibility
pub const MODEL_NAME: &str = crate::domain::embedding_constants::DEFAULT_EMBEDDING_MODEL_NAME;

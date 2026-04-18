//! Embedding service module
//!
//! Re-exports embedding services. This mod.rs is loaded as
//! `crate::infrastructure::services::embedding` during migration.

pub mod builder;
#[path = "dynamic.rs"]
pub mod dynamic_embedding_service;
pub mod dynamic_port;

// Re-export the ONNX embedding service as the primary embedding service
pub use crate::infrastructure::ml::OnnxEmbeddingService as EmbeddingService;
pub use dynamic_embedding_service::DynamicEmbeddingService;
pub use dynamic_port::DynamicEmbedding;

// Export MODEL_NAME constant for compatibility
pub const MODEL_NAME: &str = crate::domain::embedding_constants::DEFAULT_EMBEDDING_MODEL_NAME;

//! Machine Learning Infrastructure
//!
//! This module contains ML model infrastructure:
//! - **ONNX Embedding Service**: Embedding generation using ONNX Runtime
//! - **Model Manager**: Model loading, caching, and lifecycle management
//! - **Tokenizer**: Text tokenization for ML models
//!
//! # Migration Status
//! - [ ] onnx_embedding_service.rs - Will move from `services/onnx_embedding_service.rs`
//! - [ ] model_manager.rs - Will move from `services/model_manager.rs`
//! - [ ] tokenizer.rs - To be extracted from embedding service
//!
//! # Dependencies
//! - Uses: ONNX Runtime, Tokenizers library
//! - Provides: Embedding generation port implementation

pub mod generator;
pub mod model_manager;
pub mod onnx_embedding_service;
pub mod remote_embedding_service;
pub mod tokenizer;
pub mod validator;

// Temporarily disabled during refactoring
// #[cfg(test)]
// mod tests;

// Re-export public types
pub use generator::{EmbeddingGenerator, ModelConfig};
pub use onnx_embedding_service::OnnxEmbeddingService;
pub use remote_embedding_service::{
    RemoteEmbeddingService, DEFAULT_REMOTE_EMBEDDING_MODEL, DEFAULT_REMOTE_EMBEDDING_URL,
};
pub use validator::{validate_embedding_dimension, DimensionMismatchError};

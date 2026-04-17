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

// Orphan stubs (model_manager, tokenizer) stay here - nothing registered.
pub mod model_manager;
pub mod tokenizer;

// Re-export embedding types from the features slice (generator, onnx,
// remote, validator all live in features/embedding/).
pub use crate::features::embedding::generator::{EmbeddingGenerator, ModelConfig};
pub use crate::features::embedding::onnx_service::OnnxEmbeddingService;
pub use crate::features::embedding::remote_service::{
    RemoteEmbeddingService, DEFAULT_REMOTE_EMBEDDING_MODEL, DEFAULT_REMOTE_EMBEDDING_URL,
};
pub use crate::features::embedding::validator::{validate_embedding_dimension, DimensionMismatchError};

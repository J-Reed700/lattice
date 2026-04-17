//! Machine Learning Infrastructure aggregator.
//!
//! The embedding engine lives in `features/embedding/` (generator,
//! onnx_service, remote_service, validator). This module re-exports
//! its public types so legacy consumer paths
//! (`crate::infrastructure::ml::OnnxEmbeddingService` etc.) continue
//! to resolve.

pub use crate::features::embedding::generator::{EmbeddingGenerator, ModelConfig};
pub use crate::features::embedding::onnx_service::OnnxEmbeddingService;
pub use crate::features::embedding::remote_service::{
    RemoteEmbeddingService, DEFAULT_REMOTE_EMBEDDING_MODEL, DEFAULT_REMOTE_EMBEDDING_URL,
};
pub use crate::features::embedding::validator::{
    validate_embedding_dimension, DimensionMismatchError,
};

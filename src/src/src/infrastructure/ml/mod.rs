//! Machine Learning Infrastructure aggregator.
//!
//! The embedding engine lives in `features/embedding/`. This module
//! re-exports its public types so legacy consumer paths
//! (`crate::infrastructure::ml::*`) continue to resolve.

pub use crate::features::embedding::candle_service::CandleEmbeddingService;
pub use crate::features::embedding::generator::{EmbeddingGenerator, ModelConfig};
pub use crate::features::embedding::remote_service::{
    RemoteEmbeddingService, DEFAULT_REMOTE_EMBEDDING_MODEL, DEFAULT_REMOTE_EMBEDDING_URL,
};
pub use crate::features::embedding::validator::{
    validate_embedding_dimension, DimensionMismatchError,
};

//! Embedding service module
//!
//! Re-exports embedding services. This mod.rs is loaded as
//! `crate::infrastructure::services::embedding` during migration.

pub mod dynamic_port;

// `EmbeddingService` is the primary local embedding implementation.
// Was `OnnxEmbeddingService`; now Candle. Alias keeps every caller compiling.
pub use crate::features::embedding::candle_service::CandleEmbeddingService as EmbeddingService;
pub use dynamic_port::DynamicEmbedding;

//! # Q&A Domain Module
//!
//! Domain models for question-answering and HyDE (Hypothetical Document Embeddings).

pub mod hyde;

// Re-export public types
pub use hyde::{
    ChatResponse, ChunkMetadata, DocumentChunk, EnrichedContext, HyDEInterpretation, QueryType,
    ResponseMetadata, SearchStrategy, Source, ToolIntent,
};

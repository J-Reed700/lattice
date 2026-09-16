//! # Domain Entities
//!
//! Core business entities representing domain concepts.
//!
//! Entities have identity and lifecycle, distinct from value objects which are
//! defined by their attributes.

pub mod chunk;
pub mod document;
pub mod embedding;
pub mod model;
pub mod model_file;
pub mod search_result;

pub use chunk::Chunk;
pub use document::{Document, DocumentStatus};
pub use embedding::Embedding;
pub use model::Model;
pub use model_file::ModelFile;
pub use search_result::SearchResult;
